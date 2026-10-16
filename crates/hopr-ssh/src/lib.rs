//! SSH transport. `spawn_session` starts one actor per connection (ADR 0007): it connects,
//! authenticates, opens a PTY shell, then pumps bytes until either side closes.

#![warn(missing_docs)]

use std::sync::Arc;
use std::time::Duration;

use hopr_core::session::{COMMAND_CAPACITY, FlowControl, SessionCommand, SessionHandle};
use hopr_core::{AppError, CloseReason, ErrorCode, SessionId, SessionState};
use russh::client::{self, Handle};
use russh::keys::{HashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, Disconnect};
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info, warn};

/// Everything needed to open an interactive shell.
pub struct ConnectParams {
    /// Hostname or IP.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Remote user.
    pub username: String,
    /// Password (never logged).
    pub password: SecretString,
    /// Initial terminal columns.
    pub cols: u32,
    /// Initial terminal rows.
    pub rows: u32,
    /// Deadline for TCP connect + auth + shell.
    pub connect_timeout: Duration,
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
    let (state_tx, state_rx) = watch::channel(SessionState::Connecting { hop: 1, total: 1 });
    let span = tracing::info_span!("session", session_id = %id, host = %params.host);
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

struct Client;

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Phase 1 data path: trust on every connect. Known-hosts TOFU with a changed-key
        // hard stop replaces this in Phase 2 (HOPR_PLAN §9.15).
        if let PublicKeyOrCertificate::PublicKey { key, .. } = key {
            warn!(fingerprint = %key.fingerprint(HashAlg::Sha256), "host key accepted without verification");
        }
        Ok(true)
    }
}

async fn run(
    params: ConnectParams,
    mut commands: mpsc::Receiver<SessionCommand>,
    state: &watch::Sender<SessionState>,
    output: mpsc::Sender<Vec<u8>>,
) -> SessionState {
    let established = tokio::time::timeout(params.connect_timeout, establish(&params, state)).await;
    let (handle, channel) = match established {
        Ok(Ok(pair)) => pair,
        Ok(Err(error)) => return SessionState::Failed { error },
        Err(_) => {
            let error = AppError::new(
                ErrorCode::Timeout,
                format!("Timed out connecting to {}:{}", params.host, params.port),
            );
            return SessionState::Failed { error };
        }
    };
    drop(params); // release the password as soon as auth is done
    state.send_replace(SessionState::Ready);

    let (mut reader, writer) = channel.split();
    let mut exited = false;
    loop {
        tokio::select! {
            cmd = commands.recv() => match cmd {
                Some(SessionCommand::Write(data)) => {
                    if writer.data_bytes(data).await.is_err() {
                        return closed(CloseReason::ConnectionLost);
                    }
                }
                Some(SessionCommand::Resize { cols, rows }) => {
                    let _ = writer.window_change(cols, rows, 0, 0).await;
                }
                Some(SessionCommand::Close) | None => {
                    let _ = writer.eof().await;
                    let _ = writer.close().await;
                    let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
                    return closed(CloseReason::UserClosed);
                }
            },
            msg = reader.wait() => match msg {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if output.send(data.to_vec()).await.is_err() {
                        // Nobody is rendering this session any more.
                        let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
                        return closed(CloseReason::UserClosed);
                    }
                }
                Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::ExitSignal { .. }) => {
                    exited = true;
                }
                Some(ChannelMsg::Close) | None => {
                    let reason = if exited { CloseReason::RemoteExited } else { CloseReason::ConnectionLost };
                    return closed(reason);
                }
                Some(_) => {}
            },
        }
    }
}

fn closed(reason: CloseReason) -> SessionState {
    SessionState::Closed { reason }
}

async fn establish(
    params: &ConnectParams,
    state: &watch::Sender<SessionState>,
) -> Result<(Handle<Client>, russh::Channel<client::Msg>), AppError> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        nodelay: true,
        ..Default::default()
    });

    let mut handle = client::connect(config, (params.host.as_str(), params.port), Client)
        .await
        .map_err(|e| connect_error(params, &e))?;

    state.send_replace(SessionState::Authenticating {
        method: "password".into(),
    });
    let auth = handle
        .authenticate_password(params.username.as_str(), params.password.expose_secret())
        .await
        .map_err(|e| {
            AppError::new(ErrorCode::AuthFailed, "Authentication failed").with_detail(e.to_string())
        })?;
    if !auth.success() {
        return Err(AppError::new(
            ErrorCode::AuthFailed,
            format!("The server rejected the password for {}", params.username),
        ));
    }

    let channel = handle.channel_open_session().await.map_err(shell_error)?;
    channel
        .request_pty(true, "xterm-256color", params.cols, params.rows, 0, 0, &[])
        .await
        .map_err(shell_error)?;
    channel.request_shell(true).await.map_err(shell_error)?;
    Ok((handle, channel))
}

fn connect_error(params: &ConnectParams, e: &russh::Error) -> AppError {
    let code = match e {
        russh::Error::IO(_) | russh::Error::Disconnect | russh::Error::ConnectionTimeout => {
            ErrorCode::ConnectFailed
        }
        _ => ErrorCode::Internal,
    };
    AppError::new(
        code,
        format!("Couldn't connect to {}:{}", params.host, params.port),
    )
    .with_detail(e.to_string())
}

fn shell_error(e: russh::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "The server refused to open a shell")
        .with_detail(e.to_string())
}
