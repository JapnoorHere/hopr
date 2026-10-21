//! OpenSSH-style `ProxyCommand`: run a local command and speak SSH over its stdin/stdout
//! (e.g. `cloudflared access ssh --hostname %h`, `ssh -W %h:%p bastion`, `nc proxy %p`).
//! The command is killed when the connection ends.

use std::io;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader, Join, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::warn;

/// `%h` host, `%p` port, `%r` remote user, `%%` a literal percent sign.
pub fn expand(command: &str, host: &str, port: u16, user: &str) -> String {
    let mut out = String::with_capacity(command.len() + host.len());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('h') => out.push_str(host),
            Some('p') => out.push_str(&port.to_string()),
            Some('r') => out.push_str(user),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// The running command as one duplex stream. Dropping it kills the command.
pub struct CommandStream {
    io: Join<ChildStdout, ChildStdin>,
    // Held only so the process lives as long as the stream (`kill_on_drop`).
    _child: Child,
}

/// Start `command` through the system shell (`cmd /C` on Windows, `sh -c` elsewhere).
/// Its stderr goes to the log, so a failing command can be diagnosed.
pub fn spawn(command: &str) -> io::Result<CommandStream> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        // Verbatim: quoting it as one argument would escape the quotes inside it.
        c.arg("/C").raw_arg(command);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // No console window flashing up for the command.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("ProxyCommand has no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("ProxyCommand has no stdout"))?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!(target: "hopr_ssh::proxy_command", "{line}");
            }
        });
    }
    Ok(CommandStream {
        io: tokio::io::join(stdout, stdin),
        _child: child,
    })
}

impl AsyncRead for CommandStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for CommandStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn expands_tokens() {
        assert_eq!(
            expand("ssh -W %h:%p -l %r jump", "db", 2222, "ann"),
            "ssh -W db:2222 -l ann jump"
        );
        assert_eq!(expand("100%% %x %", "h", 1, "u"), "100% %x %");
    }

    #[tokio::test]
    async fn reads_the_command_output() {
        // Both directions are covered by the SSH-over-ProxyCommand integration test.
        let mut s = spawn("echo hopr-proxy").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        assert_eq!(out.trim(), "hopr-proxy");
        // Writing after the command exited fails instead of hanging.
        let _ = s.write_all(b"x").await;
    }
}
