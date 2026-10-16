//! `hopr-devsshd`: a development SSH server bound to 127.0.0.1 that serves a real local
//! shell through a PTY. Lets Hopr be exercised end to end on a machine with no sshd.
//!
//! Env: HOPR_DEVSSHD_PORT (default 2222), HOPR_DEVSSHD_PASSWORD (default "hopr"),
//! HOPR_DEVSSHD_SHELL (default: powershell.exe on Windows, $SHELL or /bin/sh elsewhere).
//! Any username is accepted. Never expose this beyond localhost.

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, Pty};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let port: u16 = std::env::var("HOPR_DEVSSHD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2222);
    let password = std::env::var("HOPR_DEVSSHD_PASSWORD").unwrap_or_else(|_| "hopr".into());

    let config = Arc::new(server::Config {
        keys: vec![russh::keys::PrivateKey::random(
            &mut rand::rng(),
            russh::keys::Algorithm::Ed25519,
        )?],
        auth_rejection_time: Duration::from_millis(300),
        auth_rejection_time_initial: Some(Duration::ZERO),
        inactivity_timeout: None,
        ..Default::default()
    });
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!(
        "hopr-devsshd listening on 127.0.0.1:{port} (any user, password from HOPR_DEVSSHD_PASSWORD)"
    );
    let mut server = DevServer {
        password: Arc::new(password),
    };
    server.run_on_socket(config, &listener).await?;
    Ok(())
}

#[derive(Clone)]
struct DevServer {
    password: Arc<String>,
}

impl server::Server for DevServer {
    type Handler = Conn;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Conn {
        Conn {
            password: self.password.clone(),
            size: PtySize::default(),
            pty: None,
        }
    }
}

struct LivePty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

struct Conn {
    password: Arc<String>,
    size: PtySize,
    pty: Option<LivePty>,
}

fn pty_size(cols: u32, rows: u32) -> PtySize {
    PtySize {
        cols: cols.clamp(1, u16::MAX as u32) as u16,
        rows: rows.clamp(1, u16::MAX as u32) as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn shell_command() -> CommandBuilder {
    let spec = std::env::var("HOPR_DEVSSHD_SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "powershell.exe -NoLogo".into()
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
        }
    });
    let mut parts = spec.split_whitespace();
    let mut cmd = CommandBuilder::new(parts.next().unwrap_or("sh"));
    cmd.args(parts);
    cmd.env("TERM", "xterm-256color");
    cmd
}

impl server::Handler for Conn {
    type Error = BoxError;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == self.password.as_str() {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        cols: u32,
        rows: u32,
        _pw: u32,
        _ph: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.size = pty_size(cols, rows);
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let pair = native_pty_system().openpty(self.size)?;
        let mut child = pair.slave.spawn_command(shell_command())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        self.pty = Some(LivePty {
            master: pair.master,
            writer,
        });
        session.channel_success(channel)?;

        // PTY reads are blocking: a thread reads, a task forwards into the SSH channel.
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.blocking_send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let (exit_tx, mut exit_rx) = mpsc::channel::<u32>(1);
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code()).unwrap_or(1);
            let _ = exit_tx.blocking_send(code);
        });

        let handle = session.handle();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    chunk = rx.recv() => match chunk {
                        Some(data) => {
                            if handle.data(channel, data).await.is_err() {
                                return;
                            }
                        }
                        None => break,
                    },
                    code = exit_rx.recv() => {
                        // Flush whatever output is already queued, then report the exit.
                        while let Ok(data) = rx.try_recv() {
                            let _ = handle.data(channel, data).await;
                        }
                        let _ = handle.exit_status_request(channel, code.unwrap_or(1)).await;
                        break;
                    }
                }
            }
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(pty) = self.pty.as_mut() {
            pty.writer.write_all(data)?;
            pty.writer.flush()?;
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        cols: u32,
        rows: u32,
        _pw: u32,
        _ph: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.size = pty_size(cols, rows);
        if let Some(pty) = self.pty.as_ref() {
            pty.master.resize(self.size)?;
        }
        Ok(())
    }
}
