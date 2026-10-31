//! Non-interactive channels: one-shot remote commands (deploy key, later run-on-group) and
//! subsystems (SFTP). Nothing is asked: an unverified host key or a prompt Hopr can't
//! answer from saved values fails with a clear error instead.

use std::sync::{Arc, Mutex};

use hopr_core::{AppError, ErrorCode, HostKeyDecision, SessionState};
use russh::client::{Handle, Msg};
use russh::{ChannelMsg, ChannelStream, Disconnect};
use tokio::sync::{mpsc, watch};

use crate::ConnectParams;
use crate::connect::{self, Client, Question};

/// Most output kept per stream; the rest is dropped.
const MAX_OUTPUT: usize = 256 * 1024;

/// Result of a remote command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    /// Exit status, if the server reported one.
    pub exit_status: Option<u32>,
    /// Standard output (lossy UTF-8).
    pub stdout: String,
    /// Standard error (lossy UTF-8).
    pub stderr: String,
}

#[derive(Default)]
struct Asked {
    /// Host whose key isn't trusted yet.
    host_key: Option<String>,
    /// A login prompt came up that couldn't be answered from saved values.
    prompt: bool,
}

/// Keeps a chain's connections open; dropping it closes them.
pub struct Connection {
    handles: Vec<Handle<Client>>,
}

impl Connection {
    /// Disconnect every hop, innermost first.
    pub async fn disconnect(&self) {
        for h in self.handles.iter().rev() {
            let _ = h.disconnect(Disconnect::ByApplication, "", "en").await;
        }
    }

    /// The connection to the last hop is still up.
    pub fn is_open(&self) -> bool {
        self.handles.last().is_some_and(|h| !h.is_closed())
    }

    /// Open a channel to `host:port` as seen from the last hop (local and SOCKS forwards).
    pub async fn direct_tcpip(
        &self,
        host: &str,
        port: u16,
        origin_addr: &str,
        origin_port: u32,
    ) -> Result<ChannelStream<Msg>, AppError> {
        let channel = self
            .target()?
            .channel_open_direct_tcpip(host, u32::from(port), origin_addr, origin_port)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ConnectFailed,
                    format!("The server couldn't reach {host}:{port}"),
                )
                .with_detail(e.to_string())
            })?;
        Ok(channel.into_stream())
    }

    /// Ask the last hop to listen on `addr:port` (0 = any free port) and forward
    /// connections back to us. Returns the port it listens on.
    pub async fn tcpip_forward(&self, addr: &str, port: u16) -> Result<u32, AppError> {
        let bound = self
            .target()?
            .tcpip_forward(addr, u32::from(port))
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ConnectFailed,
                    format!("The server refused to listen on {addr}:{port}"),
                )
                .with_detail(e.to_string())
            })?;
        Ok(if bound == 0 { u32::from(port) } else { bound })
    }

    /// Stop a remote listener.
    pub async fn cancel_tcpip_forward(&self, addr: &str, port: u32) {
        if let Ok(h) = self.target() {
            let _ = h.cancel_tcpip_forward(addr, port).await;
        }
    }

    fn target(&self) -> Result<&Handle<Client>, AppError> {
        self.handles
            .last()
            .ok_or_else(|| AppError::invalid("Nothing to connect to"))
    }
}

/// A subsystem channel (e.g. `sftp`) and the connection it runs on.
pub struct Subsystem {
    /// The channel as a byte stream.
    pub stream: ChannelStream<Msg>,
    /// Keep alive as long as the stream is used.
    pub connection: Connection,
}

/// Dial and authenticate the whole chain without asking the user anything.
pub async fn connect_quiet(params: &ConnectParams) -> Result<Connection, AppError> {
    let total = u32::try_from(params.hops.len()).unwrap_or(1);
    let (state, _keep) = watch::channel(SessionState::Connecting { hop: 1, total });
    let (q_tx, mut q_rx) = mpsc::channel::<Question>(4);
    let asked = Arc::new(Mutex::new(Asked::default()));
    let answerer = {
        let asked = asked.clone();
        tokio::spawn(async move {
            while let Some(q) = q_rx.recv().await {
                match q {
                    Question::HostKey { prompt, reply } => {
                        if let Ok(mut a) = asked.lock() {
                            a.host_key = Some(prompt.host.clone());
                        }
                        let _ = reply.send(HostKeyDecision::Reject);
                    }
                    Question::Prompt { reply, .. } => {
                        if let Ok(mut a) = asked.lock() {
                            a.prompt = true;
                        }
                        let _ = reply.send(None);
                    }
                }
            }
        })
    };
    let result = connect::dial_chain(params, &state, q_tx).await;
    answerer.abort();
    result.map(|handles| Connection { handles }).map_err(|end| {
        let a = asked.lock().map(|a| (a.host_key.clone(), a.prompt));
        match (end, a) {
            // A changed key keeps its own (stronger) message.
            (SessionState::Failed { error }, _) if error.code == ErrorCode::HostKeyMismatch => {
                error
            }
            (_, Ok((Some(host), _))) => AppError::new(
                ErrorCode::HostKeyMismatch,
                format!("Hopr hasn't verified the host key of {host} yet. Connect to it once, then try again."),
            ),
            (_, Ok((None, true))) => AppError::new(
                ErrorCode::AuthFailed,
                "The server asked for a login Hopr has no saved answer for. Save the password or use a key, then try again.",
            ),
            (SessionState::Failed { error }, _) => error,
            (other, _) => AppError::new(ErrorCode::Internal, "Couldn't connect")
                .with_detail(format!("{other:?}")),
        }
    })
}

/// Open a subsystem (e.g. `sftp`) on the last hop.
pub async fn subsystem(params: ConnectParams, name: &str) -> Result<Subsystem, AppError> {
    let connection = connect_quiet(&params).await?;
    let fail = |e: russh::Error| {
        AppError::new(
            ErrorCode::Internal,
            format!("The server refused the {name} subsystem"),
        )
        .with_detail(e.to_string())
    };
    let channel = connection
        .target()?
        .channel_open_session()
        .await
        .map_err(fail)?;
    channel.request_subsystem(true, name).await.map_err(fail)?;
    Ok(Subsystem {
        stream: channel.into_stream(),
        connection,
    })
}

/// Run `command` on the last hop of `params`, feed it `stdin`, and collect its output.
pub async fn exec(
    params: ConnectParams,
    command: &str,
    stdin: &[u8],
) -> Result<ExecOutput, AppError> {
    let connection = connect_quiet(&params).await?;
    let result = run(&params, &connection, command, stdin).await;
    connection.disconnect().await;
    result
}

async fn run(
    params: &ConnectParams,
    connection: &Connection,
    command: &str,
    stdin: &[u8],
) -> Result<ExecOutput, AppError> {
    let fail = |e: russh::Error| AppError::internal("The server refused to run the command", e);
    let mut channel = connection
        .target()?
        .channel_open_session()
        .await
        .map_err(fail)?;
    channel.exec(true, command).await.map_err(fail)?;
    if !stdin.is_empty() {
        channel.data(stdin).await.map_err(fail)?;
    }
    channel.eof().await.map_err(fail)?;

    let mut out = ExecOutput {
        exit_status: None,
        stdout: String::new(),
        stderr: String::new(),
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let collect = async {
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } if stdout.len() < MAX_OUTPUT => {
                    stdout.extend_from_slice(&data);
                }
                ChannelMsg::ExtendedData { data, .. } if stderr.len() < MAX_OUTPUT => {
                    stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
    };
    if tokio::time::timeout(params.connect_timeout * 3, collect)
        .await
        .is_err()
    {
        return Err(AppError::new(
            ErrorCode::Timeout,
            "The command didn't finish in time",
        ));
    }
    out.stdout = String::from_utf8_lossy(&stdout).into_owned();
    out.stderr = String::from_utf8_lossy(&stderr).into_owned();
    Ok(out)
}
