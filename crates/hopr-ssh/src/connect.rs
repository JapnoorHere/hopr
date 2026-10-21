//! Dialing a chain of hops: each hop after the first is reached through a `direct-tcpip`
//! channel on the previous one, so only the first hop is dialed from this machine.

use std::sync::{Arc, Mutex};

use data_encoding::BASE64;
use hopr_core::model::KnownHost;
use hopr_core::session::{PromptAnswer, SessionCommand};
use hopr_core::{
    AppError, AuthPrompt, CloseReason, ErrorCode, HostKeyDecision, HostKeyPrompt, SessionState,
};
use russh::client::{self, Handle};
use russh::keys::{HashAlg, PublicKeyBase64, PublicKeyOrCertificate};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::warn;

use crate::known::{self, HostKeyStatus};
use crate::{ConnectParams, Hop, TrustedKey, agent, auth, command, proxy};

/// Something the connection needs the user to decide.
pub(crate) enum Question {
    HostKey {
        prompt: HostKeyPrompt,
        reply: oneshot::Sender<HostKeyDecision>,
    },
    Prompt {
        prompt: AuthPrompt,
        reply: oneshot::Sender<Option<PromptAnswer>>,
    },
}

enum Pending {
    HostKey(oneshot::Sender<HostKeyDecision>),
    Prompt(oneshot::Sender<Option<PromptAnswer>>),
}

/// Why a handler refused the host key.
#[derive(Clone, Copy)]
enum Refusal {
    UserRejectedNew,
    UserRejectedChanged,
    Certificate,
}

/// A live chain: every hop's handle (kept alive for the session) and the shell channel.
pub(crate) struct Connection {
    pub handles: Vec<Handle<Client>>,
    pub channel: russh::Channel<client::Msg>,
}

pub(crate) struct Client {
    host: String,
    port: u16,
    known: Vec<KnownHost>,
    questions: mpsc::Sender<Question>,
    on_trust: Option<Arc<dyn Fn(TrustedKey) + Send + Sync>>,
    refusal: Arc<Mutex<Option<Refusal>>>,
    forward_agent: bool,
    /// Where connections from remote forwards go (last hop only).
    remote_forwards: Option<crate::forward::RemoteForwards>,
}

impl Client {
    fn refuse(&self, why: Refusal) {
        if let Ok(mut r) = self.refusal.lock() {
            *r = Some(why);
        }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            // Host certificates arrive with SSH CA support (Phase 4).
            self.refuse(Refusal::Certificate);
            return Ok(false);
        };
        let key_type = key.algorithm().as_str().to_owned();
        let key_b64 = BASE64.encode(&key.public_key_bytes());
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let (changed, previous) =
            match known::check(&self.host, self.port, &key_type, &key_b64, &self.known) {
                HostKeyStatus::Trusted => return Ok(true),
                HostKeyStatus::Unknown => (false, Vec::new()),
                HostKeyStatus::Changed { previous } => {
                    warn!(%fingerprint, host = %self.host, "host key differs from the trusted key");
                    (true, previous)
                }
            };
        let prompt = HostKeyPrompt {
            host: self.host.clone(),
            port: self.port,
            key_type: key_type.clone(),
            fingerprint: fingerprint.clone(),
            changed,
            previous,
        };
        let (tx, rx) = oneshot::channel();
        if self
            .questions
            .send(Question::HostKey { prompt, reply: tx })
            .await
            .is_err()
        {
            return Ok(false);
        }
        match rx.await {
            Ok(HostKeyDecision::Trust) => {
                if let Some(hook) = &self.on_trust {
                    hook(TrustedKey {
                        host: self.host.clone(),
                        port: self.port,
                        key_type,
                        key_b64,
                        fingerprint,
                        replace: changed,
                    });
                }
                Ok(true)
            }
            Ok(HostKeyDecision::Once) => Ok(true),
            Ok(HostKeyDecision::Reject) | Err(_) => {
                self.refuse(if changed {
                    Refusal::UserRejectedChanged
                } else {
                    Refusal::UserRejectedNew
                });
                Ok(false)
            }
        }
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: russh::Channel<client::Msg>,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if self.forward_agent {
            reply.accept().await;
            tokio::spawn(agent::bridge(channel));
        } else {
            // Only hosts with forwarding enabled may use the local agent.
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }

    /// A connection to one of our remote forwards (`-R`): pipe it to its local destination.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let routed = self
            .remote_forwards
            .as_ref()
            .is_some_and(|r| r.has(connected_port));
        if routed {
            reply.accept().await;
            if let Some(r) = &self.remote_forwards {
                r.dispatch(connected_port, channel.into_stream());
            }
        } else {
            // A forward we never asked for.
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }
}

/// Establish while answering the user's questions (host keys, prompts) and honoring Close.
pub(crate) async fn establish_interactive(
    params: ConnectParams,
    state: &watch::Sender<SessionState>,
    commands: &mut mpsc::Receiver<SessionCommand>,
) -> Result<Connection, SessionState> {
    let (q_tx, mut q_rx) = mpsc::channel::<Question>(4);
    let mut pending: Option<Pending> = None;
    let mut resume_state = state.borrow().clone();
    let setup = establish(params, state, q_tx);
    tokio::pin!(setup);
    loop {
        tokio::select! {
            res = &mut setup => return res,
            Some(q) = q_rx.recv() => {
                resume_state = state.borrow().clone();
                match q {
                    Question::HostKey { prompt, reply } => {
                        state.send_replace(SessionState::VerifyingHostKey { prompt });
                        pending = Some(Pending::HostKey(reply));
                    }
                    Question::Prompt { prompt, reply } => {
                        state.send_replace(SessionState::Prompting { prompt });
                        pending = Some(Pending::Prompt(reply));
                    }
                }
            }
            cmd = commands.recv() => match cmd {
                Some(SessionCommand::HostKeyDecision(d)) => {
                    if let Some(Pending::HostKey(tx)) = pending.take() {
                        state.send_replace(resume_state.clone());
                        let _ = tx.send(d);
                    }
                }
                Some(SessionCommand::PromptResponse(answer)) => {
                    if let Some(Pending::Prompt(tx)) = pending.take() {
                        state.send_replace(resume_state.clone());
                        let _ = tx.send(answer);
                    }
                }
                Some(SessionCommand::Close) | None => {
                    return Err(SessionState::Closed { reason: CloseReason::UserClosed });
                }
                // Input before the shell exists has nowhere to go.
                Some(SessionCommand::Write(_) | SessionCommand::Resize { .. }) => {}
            },
        }
    }
}

fn failed(code: ErrorCode, message: impl Into<String>) -> SessionState {
    SessionState::Failed {
        error: AppError::new(code, message),
    }
}

fn target(hop: &Hop) -> String {
    if hop.port == 22 {
        hop.host.clone()
    } else {
        format!("{}:{}", hop.host, hop.port)
    }
}

async fn establish(
    params: ConnectParams,
    state: &watch::Sender<SessionState>,
    questions: mpsc::Sender<Question>,
) -> Result<Connection, SessionState> {
    let handles = dial_chain(&params, state, questions).await?;
    let shell_err = |e: russh::Error| SessionState::Failed {
        error: AppError::new(ErrorCode::Internal, "The server refused to open a shell")
            .with_detail(e.to_string()),
    };
    let Some(target_handle) = handles.last() else {
        return Err(failed(ErrorCode::InvalidInput, "Nothing to connect to"));
    };
    state.send_replace(SessionState::OpeningShell);
    let channel = target_handle
        .channel_open_session()
        .await
        .map_err(shell_err)?;
    if params.agent_forwarding {
        // Best effort: a server that refuses forwarding still gets a shell.
        let _ = channel.agent_forward(false).await;
    }
    channel
        .request_pty(true, "xterm-256color", params.cols, params.rows, 0, 0, &[])
        .await
        .map_err(shell_err)?;
    channel.request_shell(true).await.map_err(shell_err)?;
    Ok(Connection { handles, channel })
}

/// Dial and authenticate every hop, outermost first. The first hop goes through the proxy
/// when one is set.
pub(crate) async fn dial_chain(
    params: &ConnectParams,
    state: &watch::Sender<SessionState>,
    questions: mpsc::Sender<Question>,
) -> Result<Vec<Handle<Client>>, SessionState> {
    let total = u32::try_from(params.hops.len()).unwrap_or(1);
    let last = params.hops.len().saturating_sub(1);
    let config = Arc::new(client::Config {
        // Probe every 15 s; three missed replies (~45 s) drop the connection for reconnect.
        keepalive_interval: Some(std::time::Duration::from_secs(15)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });
    let mut handles: Vec<Handle<Client>> = Vec::with_capacity(params.hops.len());

    for (i, hop) in params.hops.iter().enumerate() {
        let hop_no = u32::try_from(i + 1).unwrap_or(total);
        state.send_replace(SessionState::Connecting { hop: hop_no, total });
        // Once the byte stream is open, the handshake starts; the time to open it is
        // about one round trip, which the connecting card shows.
        let shaking = |rtt: Option<std::time::Duration>| {
            state.send_replace(SessionState::Handshaking {
                hop: hop_no,
                total,
                rtt_ms: rtt.map(|d| u32::try_from(d.as_millis()).unwrap_or(u32::MAX)),
            });
        };
        let dial_started = std::time::Instant::now();
        let refusal = Arc::new(Mutex::new(None));
        let handler = Client {
            host: hop.host.clone(),
            port: hop.port,
            known: hop.known_hosts.clone(),
            questions: questions.clone(),
            on_trust: params.hooks.on_trust.clone(),
            refusal: refusal.clone(),
            forward_agent: params.agent_forwarding && i == last,
            remote_forwards: if i == last {
                params.hooks.remote_forwards.clone()
            } else {
                None
            },
        };

        let connected = match handles.last() {
            None if params.proxy_command.is_some() => {
                let cmd = params.proxy_command.as_deref().unwrap_or_default();
                let line = command::expand(cmd, &hop.host, hop.port, &hop.username);
                let stream = match command::spawn(&line) {
                    Ok(s) => s,
                    Err(e) => {
                        return Err(SessionState::Failed {
                            error: AppError::new(
                                ErrorCode::ConnectFailed,
                                format!("Couldn't start the ProxyCommand for {}", target(hop)),
                            )
                            .with_detail(format!("{line}: {e}")),
                        });
                    }
                };
                shaking(None);
                client::connect_stream(config.clone(), stream, handler).await
            }
            None => {
                let dial = async {
                    match &params.proxy {
                        Some(p) => proxy::dial(p, &hop.host, hop.port).await,
                        None => tokio::net::TcpStream::connect((hop.host.as_str(), hop.port)).await,
                    }
                };
                let via = params
                    .proxy
                    .as_ref()
                    .map(|p| format!(" through proxy {p}"))
                    .unwrap_or_default();
                let stream = match tokio::time::timeout(params.connect_timeout, dial).await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        return Err(SessionState::Failed {
                            error: AppError::new(
                                ErrorCode::ConnectFailed,
                                format!("Couldn't connect to {}{via}", target(hop)),
                            )
                            .with_detail(e.to_string()),
                        });
                    }
                    Err(_) => {
                        return Err(failed(
                            ErrorCode::Timeout,
                            format!("Timed out connecting to {}{via}", target(hop)),
                        ));
                    }
                };
                let _ = stream.set_nodelay(true);
                shaking(Some(dial_started.elapsed()));
                client::connect_stream(config.clone(), stream, handler).await
            }
            Some(prev) => {
                let via = target(&params.hops[i - 1]);
                let channel = match tokio::time::timeout(
                    params.connect_timeout,
                    prev.channel_open_direct_tcpip(
                        hop.host.as_str(),
                        u32::from(hop.port),
                        "127.0.0.1",
                        0,
                    ),
                )
                .await
                {
                    Ok(Ok(c)) => c,
                    Ok(Err(e)) => {
                        return Err(SessionState::Failed {
                            error: AppError::new(
                                ErrorCode::ConnectFailed,
                                format!("{via} couldn't reach {}", target(hop)),
                            )
                            .with_detail(e.to_string()),
                        });
                    }
                    Err(_) => {
                        return Err(failed(
                            ErrorCode::Timeout,
                            format!("Timed out reaching {} through {via}", target(hop)),
                        ));
                    }
                };
                shaking(Some(dial_started.elapsed()));
                client::connect_stream(config.clone(), channel.into_stream(), handler).await
            }
        };

        let mut handle = match connected {
            Ok(h) => h,
            Err(e) => {
                let why = refusal.lock().ok().and_then(|r| *r);
                return Err(match why {
                    Some(Refusal::UserRejectedNew) => SessionState::Closed {
                        reason: CloseReason::UserClosed,
                    },
                    Some(Refusal::UserRejectedChanged) => failed(
                        ErrorCode::HostKeyMismatch,
                        format!(
                            "Connection blocked: the host key for {} changed and was not accepted",
                            target(hop)
                        ),
                    ),
                    Some(Refusal::Certificate) => failed(
                        ErrorCode::Internal,
                        format!(
                            "{} presents a host certificate, which Hopr doesn't support yet",
                            target(hop)
                        ),
                    ),
                    None => SessionState::Failed {
                        error: AppError::new(
                            ErrorCode::ConnectFailed,
                            format!("SSH handshake with {} failed", target(hop)),
                        )
                        .with_detail(e.to_string()),
                    },
                });
            }
        };
        auth::authenticate(&mut handle, hop, state, &questions).await?;
        handles.push(handle);
    }
    Ok(handles)
}
