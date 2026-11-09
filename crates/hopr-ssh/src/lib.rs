//! SSH transport. `spawn_session` starts one actor per connection (ADR 0007): it dials a
//! chain of hops (each through the previous one), verifies host keys, authenticates, opens a
//! PTY shell, then pumps bytes until either side closes. A dropped connection is retried
//! with backoff when the caller supplied a way to rebuild the parameters. Anything needing
//! the user (host key, passwords, OTP) is asked through the session state and answered with
//! a session command.

#![warn(missing_docs)]

mod agent;
mod auth;
mod command;
mod connect;
mod exec;
pub mod forward;
pub mod keys;
pub mod known;
mod proxy;
pub mod totp;

pub use exec::{
    Connection as QuietConnection, ExecOutput, Subsystem, connect_quiet, exec, subsystem,
};
pub use proxy::{Proxy, ProxyKind};

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hopr_core::model::KnownHost;
use hopr_core::session::{COMMAND_CAPACITY, FlowControl, SessionCommand, SessionHandle};
use hopr_core::{AppError, CloseReason, SessionId, SessionState};
use russh::keys::PrivateKey;
use russh::{ChannelMsg, Disconnect};
use secrecy::SecretString;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{Instrument, info, warn};

use crate::connect::Connection;

/// One way to authenticate, tried in the order given.
pub enum Credential {
    /// A known password.
    Password(SecretString),
    /// A private key.
    Key(Arc<PrivateKey>),
    /// A private key with an OpenSSH certificate signed for it (CA-based login).
    Certificate {
        /// The key the certificate was issued for.
        key: Arc<PrivateKey>,
        /// The certificate.
        cert: Box<russh::keys::Certificate>,
    },
    /// Keys held by the system SSH agent (Windows OpenSSH agent / Pageant, or SSH_AUTH_SOCK).
    Agent,
    /// Keyboard-interactive. Password and OTP prompts are answered automatically when the
    /// values are known; anything else is asked.
    KeyboardInteractive {
        /// Answer for password-like prompts.
        password: Option<SecretString>,
        /// Base32 TOTP seed for code-like prompts.
        totp_seed: Option<SecretString>,
    },
    /// Ask the user for a password (no saved one).
    PromptPassword,
}

impl Credential {
    fn name(&self) -> &'static str {
        match self {
            Self::Password(_) | Self::PromptPassword => "password",
            Self::Key(_) => "publickey",
            Self::Certificate { .. } => "certificate",
            Self::Agent => "agent",
            Self::KeyboardInteractive { .. } => "keyboard-interactive",
        }
    }
}

/// A host key the user chose to trust.
#[derive(Debug, Clone)]
pub struct TrustedKey {
    /// Host as dialed.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Algorithm.
    pub key_type: String,
    /// Base64 blob.
    pub key_b64: String,
    /// "SHA256:…".
    pub fingerprint: String,
    /// The key replaces a previously trusted (different) key.
    pub replace: bool,
}

/// Saves a password the user typed and asked to remember.
pub type SavePassword = Arc<dyn Fn(SecretString) + Send + Sync>;

/// One server in the chain. The last hop is where the shell opens.
pub struct Hop {
    /// Hostname or IP, resolved by the previous hop (or locally for the first).
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Remote user.
    pub username: String,
    /// Authentication methods, in order.
    pub credentials: Vec<Credential>,
    /// Trusted keys that may apply to this host (plain and hashed entries).
    pub known_hosts: Vec<KnownHost>,
    /// Remember a typed password for this hop.
    pub on_save_password: Option<SavePassword>,
}

/// Callbacks into the app (persistence). They must not block.
#[derive(Default, Clone)]
pub struct SessionHooks {
    /// Persist a newly trusted host key.
    pub on_trust: Option<Arc<dyn Fn(TrustedKey) + Send + Sync>>,
    /// The shell is live (e.g. record "last connected").
    pub on_ready: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Routes for remote forwards (`-R`) on the last hop.
    pub remote_forwards: Option<forward::RemoteForwards>,
}

/// Future returned by a [`Reconnect`] factory.
pub type ReconnectFuture = Pin<Box<dyn Future<Output = Result<ConnectParams, AppError>> + Send>>;

/// Rebuilds connection parameters for a reconnect (credentials are re-read then, never kept).
pub type Reconnect = Arc<dyn Fn() -> ReconnectFuture + Send + Sync>;

/// Everything needed to open an interactive shell.
pub struct ConnectParams {
    /// Servers to dial, outermost first. Must not be empty.
    pub hops: Vec<Hop>,
    /// Forward the local agent to the last hop.
    pub agent_forwarding: bool,
    /// Reach the first hop through this proxy.
    pub proxy: Option<Proxy>,
    /// Or through this command's stdin/stdout (OpenSSH `ProxyCommand`; `%h`, `%p`, `%r`
    /// are expanded for the first hop). Takes precedence over `proxy`.
    pub proxy_command: Option<String>,
    /// Initial terminal columns.
    pub cols: u32,
    /// Initial terminal rows.
    pub rows: u32,
    /// Deadline for each TCP/tunnel connect (time spent waiting for the user is not counted).
    pub connect_timeout: Duration,
    /// App callbacks.
    pub hooks: SessionHooks,
    /// How to rebuild these parameters after the connection drops; `None` disables
    /// auto-reconnect.
    pub reconnect: Option<Reconnect>,
}

/// Auto-reconnect attempts before giving up.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Delay before reconnect attempt `n` (1-based): 1, 2, 4, 8, 16, then 30 s.
pub fn reconnect_delay(attempt: u32) -> Duration {
    let secs = 1u64
        .checked_shl(attempt.saturating_sub(1))
        .unwrap_or(u64::MAX);
    Duration::from_secs(secs.min(30))
}

/// Start a session actor. Returns immediately; progress arrives on the state watch.
/// Remote output is pushed into `output` (bounded, so a slow consumer throttles the reader).
pub fn spawn_session(
    id: SessionId,
    params: ConnectParams,
    output: mpsc::Sender<Vec<u8>>,
    flow: Arc<FlowControl>,
) -> SessionHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_CAPACITY);
    let total = u32::try_from(params.hops.len()).unwrap_or(1);
    let (state_tx, state_rx) = watch::channel(SessionState::Connecting { hop: 1, total });
    let target = params
        .hops
        .last()
        .map(|h| h.host.clone())
        .unwrap_or_default();
    let span = tracing::info_span!("session", session_id = %id, host = %target, hops = total);
    tokio::spawn(
        async move {
            let end = run(params, cmd_rx, &state_tx, output).await;
            info!(state = ?end, "session ended");
            state_tx.send_replace(end);
        }
        .instrument(span),
    );
    SessionHandle::new(id, cmd_tx, state_rx, flow)
}

fn closed(reason: CloseReason) -> SessionState {
    SessionState::Closed { reason }
}

/// How a live shell stopped.
enum PumpEnd {
    /// Final; report this state.
    Done(SessionState),
    /// The transport dropped without the remote shell exiting.
    Lost,
}

async fn run(
    params: ConnectParams,
    mut commands: mpsc::Receiver<SessionCommand>,
    state: &watch::Sender<SessionState>,
    output: mpsc::Sender<Vec<u8>>,
) -> SessionState {
    if params.hops.is_empty() {
        return SessionState::Failed {
            error: AppError::invalid("Nothing to connect to"),
        };
    }
    let reconnect = params.reconnect.clone();
    let on_ready = params.hooks.on_ready.clone();
    // Latest terminal size, so a reconnected shell opens at the size on screen.
    let mut size = (params.cols, params.rows);
    let mut next = Some(params);
    let mut attempt = 0u32;
    loop {
        let params = match next.take() {
            Some(p) => p,
            None => {
                // Reconnecting: wait, then rebuild parameters from the app.
                let Some(factory) = &reconnect else {
                    return closed(CloseReason::ConnectionLost);
                };
                attempt += 1;
                if attempt > MAX_RECONNECT_ATTEMPTS {
                    return closed(CloseReason::ConnectionLost);
                }
                let delay = reconnect_delay(attempt);
                state.send_replace(SessionState::Reconnecting {
                    attempt,
                    max_attempts: MAX_RECONNECT_ATTEMPTS,
                    retry_in_secs: u32::try_from(delay.as_secs()).unwrap_or(30),
                });
                if !wait_or_close(delay, &mut commands).await {
                    return closed(CloseReason::UserClosed);
                }
                match factory().await {
                    Ok(mut p) => {
                        (p.cols, p.rows) = size;
                        p
                    }
                    Err(error) => return SessionState::Failed { error },
                }
            }
        };

        let conn = match connect::establish_interactive(params, state, &mut commands).await {
            Ok(conn) => conn,
            // A retry that can't reach the server waits and tries again; anything the user
            // decided (cancel, reject key) or a login failure ends the session.
            Err(SessionState::Failed { error }) if attempt > 0 && error.retryable => continue,
            Err(end) => return end,
        };
        attempt = 0;
        state.send_replace(SessionState::Ready);
        if let Some(hook) = &on_ready {
            hook();
        }
        match pump(conn, &mut commands, &output, &mut size).await {
            PumpEnd::Done(end) => return end,
            PumpEnd::Lost if reconnect.is_some() => warn!("connection lost; reconnecting"),
            PumpEnd::Lost => return closed(CloseReason::ConnectionLost),
        }
    }
}

/// Sleep for `delay` while still honoring Close. Returns `false` if the user closed.
async fn wait_or_close(delay: Duration, commands: &mut mpsc::Receiver<SessionCommand>) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            () = &mut sleep => return true,
            cmd = commands.recv() => match cmd {
                Some(SessionCommand::Close) | None => return false,
                Some(_) => {} // no shell to type into while reconnecting
            },
        }
    }
}

/// Writes queued for the writer task (keystrokes, paste chunks, resizes).
const WRITE_CAPACITY: usize = 256;

/// What the writer task sends to the remote shell.
enum WriteOp {
    Data(Vec<u8>),
    Resize(u32, u32),
    /// Send EOF and close the channel, then stop.
    Close,
}

async fn pump(
    conn: Connection,
    commands: &mut mpsc::Receiver<SessionCommand>,
    output: &mpsc::Sender<Vec<u8>>,
    size: &mut (u32, u32),
) -> PumpEnd {
    let Connection { handles, channel } = conn;
    let (mut reader, writer) = channel.split();
    // Writes run on their own task, in order. Awaiting one here would deadlock under a flood:
    // sending needs russh's session loop, which may itself be waiting for this loop to read.
    let (ops, mut queued) = mpsc::channel::<WriteOp>(WRITE_CAPACITY);
    let writer_task: JoinHandle<()> = tokio::spawn(async move {
        while let Some(op) = queued.recv().await {
            match op {
                WriteOp::Data(data) => {
                    if writer.data_bytes(data).await.is_err() {
                        return;
                    }
                }
                WriteOp::Resize(cols, rows) => {
                    let _ = writer.window_change(cols, rows, 0, 0).await;
                }
                WriteOp::Close => {
                    let _ = writer.eof().await;
                    let _ = writer.close().await;
                    return;
                }
            }
        }
    });
    let mut exited = false;
    // Output read but not yet taken by the batcher. While it waits, the channel isn't read
    // (backpressure), but commands are still handled, so Ctrl+C goes out at once.
    let mut held: Option<Vec<u8>> = None;
    // A write the writer had no room for. Until it goes, new commands wait in their own
    // queue (the remote isn't taking input), but output keeps flowing.
    let mut pending: Option<WriteOp> = None;
    let disconnect_all = || async {
        for h in handles.iter().rev() {
            let _ = h.disconnect(Disconnect::ByApplication, "", "en").await;
        }
    };
    // Ask the writer to send EOF and close, and give it a moment to finish.
    let close_writer = |ops: &mpsc::Sender<WriteOp>, task: JoinHandle<()>| {
        let _ = ops.try_send(WriteOp::Close);
        async move {
            let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
        }
    };
    loop {
        tokio::select! {
            biased;
            cmd = commands.recv(), if pending.is_none() => {
                let op = match cmd {
                    Some(SessionCommand::Write(data)) => WriteOp::Data(data),
                    Some(SessionCommand::Resize { cols, rows }) => {
                        *size = (cols, rows);
                        WriteOp::Resize(cols, rows)
                    }
                    Some(SessionCommand::HostKeyDecision(_) | SessionCommand::PromptResponse(_)) => {
                        continue;
                    }
                    Some(SessionCommand::Close) | None => {
                        close_writer(&ops, writer_task).await;
                        disconnect_all().await;
                        return PumpEnd::Done(closed(CloseReason::UserClosed));
                    }
                };
                match ops.try_send(op) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(op)) => pending = Some(op),
                    Err(mpsc::error::TrySendError::Closed(_)) => return PumpEnd::Lost,
                }
            },
            permit = ops.reserve(), if pending.is_some() => match (permit, pending.take()) {
                (Ok(permit), Some(op)) => permit.send(op),
                (Ok(_), None) => {}
                (Err(_), _) => return PumpEnd::Lost,
            },
            permit = output.reserve(), if held.is_some() => match permit {
                Ok(permit) => permit.send(held.take().unwrap_or_default()),
                Err(_) => {
                    // Nobody is rendering this session any more.
                    close_writer(&ops, writer_task).await;
                    disconnect_all().await;
                    return PumpEnd::Done(closed(CloseReason::UserClosed));
                }
            },
            msg = reader.wait(), if held.is_none() => match msg {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    match output.try_send(data.to_vec()) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(chunk)) => held = Some(chunk),
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            close_writer(&ops, writer_task).await;
                            disconnect_all().await;
                            return PumpEnd::Done(closed(CloseReason::UserClosed));
                        }
                    }
                }
                Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::ExitSignal { .. }) => {
                    exited = true;
                }
                Some(ChannelMsg::Close) | None => {
                    if exited {
                        disconnect_all().await;
                        return PumpEnd::Done(closed(CloseReason::RemoteExited));
                    }
                    return PumpEnd::Lost;
                }
                Some(_) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_backoff_doubles_then_caps() {
        let delays: Vec<u64> = (1..=8).map(|n| reconnect_delay(n).as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(reconnect_delay(100).as_secs(), 30);
    }
}
