//! `hopr-devsshd`: a development SSH server bound to 127.0.0.1 that serves a real local
//! shell through a PTY. Lets Hopr be exercised end to end on a machine with no sshd.
//!
//! Env: HOPR_DEVSSHD_PORT (default 2222), HOPR_DEVSSHD_PASSWORD (default "hopr"),
//! HOPR_DEVSSHD_SHELL (default: powershell.exe on Windows, $SHELL or /bin/sh elsewhere).
//! HOPR_DEVSSHD_AUTH_DELAY_MS slows each password check, to show a slow server's login.
//! Any username is accepted. Never expose this beyond localhost.
//!
//! HOPR_DEVSSHD_BENCH=1 replaces the shell with a tiny line shell for client throughput
//! tests: `flood <MiB>` streams text lines straight into the channel (no ConPTY, whose
//! own rendering caps Windows output at roughly a thousand lines per second), then prints
//! `FLOODDONE`.
//!
//! Exec requests run under `sh -c` (HOPR_DEVSSHD_SH, default `sh`) with HOME set to a
//! sandbox (HOPR_DEVSSHD_HOME, default a temp folder), never your real home. Public keys
//! listed in the sandbox's `.ssh/authorized_keys` may log in, so key deploys can be tested.
//!
//! HOPR_DEVSSHD_TELNET_PORT starts a scripted telnet server (loopback only): it negotiates
//! echo and window size, asks for a login, then echoes lines; `size` prints the last
//! window size the client reported. User certificates signed by a CA listed in the
//! sandbox's `.ssh/trusted_user_ca_keys` may log in (like OpenSSH's TrustedUserCAKeys).
//!
//! HOPR_DEVSSHD_PROXY_PORT starts a SOCKS5 + HTTP CONNECT proxy on that port. It only
//! reaches loopback, and resolves the made-up name `via-proxy.test` to 127.0.0.1, so a
//! connection to that name proves the proxy was used.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use russh::keys::PublicKeyBase64;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    let bench = std::env::var("HOPR_DEVSSHD_BENCH").is_ok_and(|v| v == "1");
    if bench {
        println!("bench mode: type `flood <MiB>`");
    }
    let home = std::env::var_os("HOPR_DEVSSHD_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("hopr-devsshd-home"));
    std::fs::create_dir_all(&home)?;
    println!(
        "sandbox HOME for exec and authorized_keys: {}",
        home.display()
    );
    if let Some(p) = std::env::var("HOPR_DEVSSHD_PROXY_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    {
        let proxy = TcpListener::bind(("127.0.0.1", p)).await?;
        println!("proxy (SOCKS5 + HTTP CONNECT, loopback only) on 127.0.0.1:{p}");
        tokio::spawn(run_proxy(proxy));
    }
    if let Some(p) = std::env::var("HOPR_DEVSSHD_TELNET_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    {
        let telnet = TcpListener::bind(("127.0.0.1", p)).await?;
        println!("telnet (scripted, loopback only) on 127.0.0.1:{p}");
        tokio::spawn(run_telnet(telnet));
    }
    let mut server = DevServer {
        password: Arc::new(password),
        bench,
        home: Arc::new(home),
        modes: hopr_sftp::server::Modes::default(),
    };
    server.run_on_socket(config, &listener).await?;
    Ok(())
}

/// Scripted telnet sessions: negotiation, a login prompt, then a line echo.
async fn run_telnet(listener: TcpListener) {
    while let Ok((client, _)) = listener.accept().await {
        tokio::spawn(async move {
            let _ = telnet_one(client).await;
        });
    }
}

async fn telnet_one(mut s: tokio::net::TcpStream) -> Result<(), BoxError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    const IAC: u8 = 255;
    const SB: u8 = 250;
    const SE: u8 = 240;
    // WILL ECHO, DO NAWS.
    s.write_all(&[IAC, 251, 1, IAC, 253, 31]).await?;
    s.write_all(b"Hopr dev telnet\r\nlogin: ").await?;
    let mut line = Vec::new();
    let mut user: Option<String> = None;
    let mut size = String::from("unknown");
    let mut buf = [0u8; 1024];
    let mut i_state = 0u8; // 0 data, 1 after IAC, 2 command byte, 3 in SB, 4 SB IAC
    let mut sub = Vec::new();
    loop {
        let n = s.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        let mut echo = Vec::new();
        for &b in &buf[..n] {
            match (i_state, b) {
                (0, IAC) => i_state = 1,
                (0, b'\r') => {
                    let text = String::from_utf8_lossy(&line).trim().to_owned();
                    line.clear();
                    echo.extend_from_slice(b"\r\n");
                    if user.is_none() {
                        user = Some(text.clone());
                        echo.extend_from_slice(format!("Welcome, {text}\r\n$ ").as_bytes());
                    } else if text == "size" {
                        echo.extend_from_slice(format!("size {size}\r\n$ ").as_bytes());
                    } else if text == "exit" {
                        s.write_all(&echo).await?;
                        s.write_all(b"bye\r\n").await?;
                        return Ok(());
                    } else {
                        echo.extend_from_slice(format!("you said: {text}\r\n$ ").as_bytes());
                    }
                }
                (0, 0 | b'\n') => {}
                (0, 127 | 8) => {
                    if line.pop().is_some() {
                        echo.extend_from_slice(b"\x08 \x08");
                    }
                }
                (0, b) => {
                    line.push(b);
                    echo.push(b);
                }
                (1, SB) => {
                    sub.clear();
                    i_state = 3;
                }
                (1, 251..=254) => i_state = 2,
                (1 | 2, _) => i_state = 0,
                (3, IAC) => i_state = 4,
                (3, b) => sub.push(b),
                (4, SE) => {
                    if sub.first() == Some(&31) && sub.len() >= 5 {
                        let cols = u16::from_be_bytes([sub[1], sub[2]]);
                        let rows = u16::from_be_bytes([sub[3], sub[4]]);
                        size = format!("{cols}x{rows}");
                    }
                    i_state = 0;
                }
                (4, _) => i_state = 3,
                _ => i_state = 0,
            }
        }
        if !echo.is_empty() {
            s.write_all(&echo).await?;
        }
    }
}

/// Loopback-only SOCKS5 / HTTP CONNECT proxy (protocol sniffed from the first byte).
async fn run_proxy(listener: TcpListener) {
    while let Ok((client, _)) = listener.accept().await {
        tokio::spawn(async move {
            let _ = proxy_one(client).await;
        });
    }
}

async fn proxy_one(mut s: tokio::net::TcpStream) -> Result<(), BoxError> {
    let first = s.read_u8().await?;
    let (host, port, socks) = if first == 5 {
        let n = s.read_u8().await?;
        let mut methods = vec![0u8; usize::from(n)];
        s.read_exact(&mut methods).await?;
        s.write_all(&[5, 0]).await?;
        let mut head = [0u8; 4];
        s.read_exact(&mut head).await?;
        let host = match head[3] {
            1 => {
                let mut ip = [0u8; 4];
                s.read_exact(&mut ip).await?;
                std::net::Ipv4Addr::from(ip).to_string()
            }
            3 => {
                let len = s.read_u8().await?;
                let mut name = vec![0u8; usize::from(len)];
                s.read_exact(&mut name).await?;
                String::from_utf8_lossy(&name).into_owned()
            }
            _ => return Err("unsupported address type".into()),
        };
        (host, s.read_u16().await?, true)
    } else {
        let mut head = vec![first];
        while !head.ends_with(b"\r\n\r\n") && head.len() < 8192 {
            head.push(s.read_u8().await?);
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let authority = text
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let (h, p) = authority.rsplit_once(':').ok_or("bad CONNECT")?;
        (h.trim_matches(['[', ']']).to_owned(), p.parse()?, false)
    };
    let real = match host.as_str() {
        "via-proxy.test" | "127.0.0.1" | "localhost" => "127.0.0.1",
        _ => {
            // Never a proxy to anywhere else.
            if socks {
                s.write_all(&[5, 2, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            } else {
                s.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await?;
            }
            return Ok(());
        }
    };
    let mut upstream = tokio::net::TcpStream::connect((real, port)).await?;
    if socks {
        s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await?;
    } else {
        s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
    }
    println!("proxy: {host}:{port}");
    tokio::io::copy_bidirectional(&mut s, &mut upstream).await?;
    Ok(())
}

#[derive(Clone)]
struct DevServer {
    password: Arc<String>,
    bench: bool,
    home: Arc<PathBuf>,
    modes: hopr_sftp::server::Modes,
}

impl server::Server for DevServer {
    type Handler = Conn;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Conn {
        Conn {
            password: self.password.clone(),
            size: PtySize::default(),
            pty: None,
            bench: self.bench,
            line: Vec::new(),
            shell: None,
            home: self.home.clone(),
            execs: HashMap::new(),
            channels: HashMap::new(),
            modes: self.modes.clone(),
        }
    }
}

const BENCH_PROMPT: &[u8] = b"bench> ";

/// 64 KiB of numbered, realistic-looking text lines.
fn flood_chunk() -> Vec<u8> {
    let mut out = Vec::with_capacity(64 * 1024 + 128);
    let mut i = 0u32;
    while out.len() < 64 * 1024 {
        out.extend_from_slice(
            format!(
                "\x1b[32m{i:>8}\x1b[0m  GET /api/v1/items?page={i} 200 12ms ................\r\n"
            )
            .as_bytes(),
        );
        i += 1;
    }
    out
}

fn run_bench_command(cmd: &str, channel: ChannelId, session: &mut Session) -> Result<(), BoxError> {
    let mib = cmd
        .strip_prefix("flood ")
        .and_then(|n| n.trim().parse::<usize>().ok());
    let Some(mib) = mib else {
        let mut msg = b"commands: flood <MiB>\r\n".to_vec();
        msg.extend_from_slice(BENCH_PROMPT);
        session.data(channel, msg)?;
        return Ok(());
    };
    let handle = session.handle();
    tokio::spawn(async move {
        let chunk = flood_chunk();
        let chunks = (mib * 1024 * 1024).div_ceil(chunk.len());
        for _ in 0..chunks {
            if handle.data(channel, chunk.clone()).await.is_err() {
                return;
            }
        }
        let mut done = b"FLOOD".to_vec();
        done.extend_from_slice(b"DONE\r\n");
        done.extend_from_slice(BENCH_PROMPT);
        let _ = handle.data(channel, done).await;
    });
    Ok(())
}

struct LivePty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

struct Conn {
    password: Arc<String>,
    size: PtySize,
    pty: Option<LivePty>,
    bench: bool,
    line: Vec<u8>,
    /// The shell channel. `data()` also sees jump-host tunnel traffic, which must not reach the PTY.
    shell: Option<ChannelId>,
    home: Arc<PathBuf>,
    /// Stdin of running exec commands, by channel.
    execs: HashMap<ChannelId, ChildStdin>,
    channels: HashMap<ChannelId, Channel<Msg>>,
    modes: hopr_sftp::server::Modes,
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
        if let Some(ms) = std::env::var("HOPR_DEVSSHD_AUTH_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        }
        Ok(if password == self.password.as_str() {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    /// Keys in the sandbox `authorized_keys` may log in.
    async fn auth_publickey(
        &mut self,
        _user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        let blob = key.public_key_base64();
        let file = self.home.join(".ssh").join("authorized_keys");
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let ok = text
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some(blob.as_str()));
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }

    /// User certificates signed by a CA in the sandbox's `trusted_user_ca_keys`, for this
    /// user (or any user when the certificate lists no principals), within their dates.
    async fn auth_openssh_certificate(
        &mut self,
        user: &str,
        cert: &russh::keys::Certificate,
    ) -> Result<Auth, Self::Error> {
        let file = self.home.join(".ssh").join("trusted_user_ca_keys");
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let trusted = text.lines().any(|l| {
            russh::keys::PublicKey::from_openssh(l.trim())
                .is_ok_and(|k| k.key_data() == cert.signature_key())
        });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let principal_ok =
            cert.valid_principals().is_empty() || cert.valid_principals().iter().any(|p| p == user);
        let ok = trusted && principal_ok && cert.valid_after() <= now && now < cert.valid_before();
        println!(
            "certificate login for {user}: {}",
            if ok { "accepted" } else { "refused" }
        );
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).into_owned();
        let sh = std::env::var("HOPR_DEVSSHD_SH").unwrap_or_else(|_| "sh".into());
        let spawned = Command::new(sh)
            .arg("-c")
            .arg(&command)
            .env("HOME", self.home.as_os_str())
            .current_dir(self.home.as_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                session.channel_success(channel)?;
                session.extended_data(channel, 1, format!("sh: {e}\n").into_bytes())?;
                session.exit_status_request(channel, 127)?;
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
        };
        println!("exec: {command}");
        if let Some(stdin) = child.stdin.take() {
            self.execs.insert(channel, stdin);
        }
        session.channel_success(channel)?;

        let handle = session.handle();
        let (tx, mut rx) = mpsc::channel::<(bool, Vec<u8>)>(64);
        for (is_err, mut pipe) in [
            (
                false,
                child
                    .stdout
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
            ),
            (
                true,
                child
                    .stderr
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
            ),
        ]
        .into_iter()
        .filter_map(|(e, p)| p.map(|p| (e, p)))
        {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 || tx.blocking_send((is_err, buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        tokio::spawn(async move {
            while let Some((is_err, data)) = rx.recv().await {
                let _ = if is_err {
                    handle.extended_data(channel, 1, data).await
                } else {
                    handle.data(channel, data).await
                };
            }
            let code =
                tokio::task::spawn_blocking(move || child.wait().ok().and_then(|s| s.code()))
                    .await
                    .ok()
                    .flatten()
                    .and_then(|c| u32::try_from(c).ok())
                    .unwrap_or(1);
            let _ = handle.exit_status_request(channel, code).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Dropping stdin lets the command see end of input.
        self.execs.remove(&channel);
        Ok(())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Kept until we know what it is for: a shell uses the id, SFTP needs the stream.
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match (name, self.channels.remove(&id)) {
            ("sftp", Some(channel)) => {
                session.channel_success(id)?;
                println!("sftp session (root {})", self.home.display());
                let fs = hopr_sftp::server::LocalFs::new(self.home.as_path(), self.modes.clone());
                tokio::spawn(russh_sftp::server::run(channel.into_stream(), fs));
            }
            _ => session.channel_failure(id)?,
        }
        Ok(())
    }

    /// Jump-host support for testing chains locally. Only loopback targets are allowed, so
    /// the dev server can never be used as a proxy to anywhere else.
    /// `-R`: listen on loopback and send each connection back as a forwarded-tcpip
    /// channel. Only loopback addresses are allowed.
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let bind = if matches!(address, "" | "localhost" | "127.0.0.1") {
            "127.0.0.1"
        } else {
            return Ok(false);
        };
        let Ok(listener) = TcpListener::bind((bind, u16::try_from(*port).unwrap_or(0))).await
        else {
            return Ok(false);
        };
        let bound = u32::from(listener.local_addr().map(|a| a.port()).unwrap_or(0));
        *port = bound;
        println!("remote forward listening on {bind}:{bound}");
        let handle = session.handle();
        let address = address.to_owned();
        tokio::spawn(async move {
            while let Ok((mut tcp, peer)) = listener.accept().await {
                let (handle, address) = (handle.clone(), address.clone());
                tokio::spawn(async move {
                    if let Ok(ch) = handle
                        .channel_open_forwarded_tcpip(
                            address,
                            bound,
                            peer.ip().to_string(),
                            u32::from(peer.port()),
                        )
                        .await
                    {
                        let mut stream = ch.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
                    }
                });
            }
        });
        Ok(true)
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let loopback = matches!(host_to_connect, "127.0.0.1" | "localhost" | "::1");
        let Ok(port) = u16::try_from(port_to_connect) else {
            reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            return Ok(());
        };
        if !loopback {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        match tokio::net::TcpStream::connect((host_to_connect, port)).await {
            Ok(mut tcp) => {
                reply.accept().await;
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
                });
            }
            Err(_) => reply.reject(russh::ChannelOpenFailure::ConnectFailed).await,
        }
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
        self.shell = Some(channel);
        if self.bench {
            session.channel_success(channel)?;
            session.data(channel, BENCH_PROMPT.to_vec())?;
            return Ok(());
        }
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
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(stdin) = self.execs.get_mut(&channel) {
            stdin.write_all(data)?;
            return Ok(());
        }
        if self.shell != Some(channel) {
            return Ok(());
        }
        if self.bench {
            for &b in data {
                if b == b'\r' {
                    session.data(channel, b"\r\n".to_vec())?;
                    let cmd = String::from_utf8_lossy(&self.line).into_owned();
                    self.line.clear();
                    run_bench_command(cmd.trim(), channel, session)?;
                } else {
                    self.line.push(b);
                    session.data(channel, vec![b])?; // echo
                }
            }
            return Ok(());
        }
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
