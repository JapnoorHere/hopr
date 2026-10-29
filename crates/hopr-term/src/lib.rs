//! Local terminals (HOPR_PLAN §9.16): shell detection and a PTY-backed session actor that
//! speaks the same `SessionHandle` protocol as SSH sessions, so the UI treats them alike.

#![warn(missing_docs)]

pub mod serial;
pub mod telnet;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hopr_core::session::{COMMAND_CAPACITY, FlowControl, SessionCommand, SessionHandle};
use hopr_core::{AppError, CloseReason, SessionId, SessionState};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info};

/// A launchable local shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShellProfile {
    /// Stable id, e.g. "pwsh", "wsl:Ubuntu".
    pub id: String,
    /// Display name.
    pub name: String,
    /// Executable path or name.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

fn profile(id: &str, name: &str, program: impl AsRef<Path>, args: &[&str]) -> ShellProfile {
    ShellProfile {
        id: id.into(),
        name: name.into(),
        program: program.as_ref().to_string_lossy().into_owned(),
        args: args.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// Decode `wsl -l -q` output (UTF-16LE on Windows) into distro names.
pub fn parse_wsl_list(raw: &[u8]) -> Vec<String> {
    let text = if raw.len() >= 2 && raw.iter().skip(1).step_by(2).take(8).all(|&b| b == 0) {
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    text.lines()
        .map(|l| {
            l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0')
                .to_owned()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// Shells available on this machine, best first.
pub fn detect_shells() -> Vec<ShellProfile> {
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Some(p) = find_in_path("pwsh.exe") {
            out.push(profile("pwsh", "PowerShell 7", p, &["-NoLogo"]));
        }
        if let Some(p) = find_in_path("powershell.exe") {
            out.push(profile("powershell", "Windows PowerShell", p, &["-NoLogo"]));
        }
        if let Some(p) = find_in_path("cmd.exe") {
            out.push(profile("cmd", "Command Prompt", p, &[]));
        }
        for base in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramW6432"),
        ]
        .into_iter()
        .flatten()
        {
            let bash = PathBuf::from(base).join("Git").join("bin").join("bash.exe");
            if bash.is_file() && !out.iter().any(|p| p.id == "git-bash") {
                out.push(profile("git-bash", "Git Bash", bash, &["--login", "-i"]));
            }
        }
        if let Some(wsl) = find_in_path("wsl.exe")
            && let Ok(o) = std::process::Command::new(&wsl).args(["-l", "-q"]).output()
            && o.status.success()
        {
            for distro in parse_wsl_list(&o.stdout) {
                out.push(profile(
                    &format!("wsl:{distro}"),
                    &format!("WSL: {distro}"),
                    &wsl,
                    &["-d", &distro],
                ));
            }
        }
    } else {
        let mut seen = Vec::new();
        if let Ok(sh) = std::env::var("SHELL") {
            seen.push(sh);
        }
        if let Ok(list) = std::fs::read_to_string("/etc/shells") {
            seen.extend(
                list.lines()
                    .map(str::trim)
                    .filter(|l| l.starts_with('/'))
                    .map(str::to_owned),
            );
        }
        for path in seen {
            if !Path::new(&path).is_file() || out.iter().any(|p: &ShellProfile| p.program == path) {
                continue;
            }
            let name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(profile(&name, &name, &path, &["-l"]));
        }
    }
    out
}

fn pty_size(cols: u32, rows: u32) -> PtySize {
    PtySize {
        cols: cols.clamp(1, 1000) as u16,
        rows: rows.clamp(1, 1000) as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn pty_error(e: impl std::fmt::Display) -> AppError {
    AppError::internal("Couldn't start the local shell", e)
}

/// Start a local shell in a PTY. Output goes to `output` (bounded).
pub fn spawn_local(
    id: SessionId,
    shell: &ShellProfile,
    cols: u32,
    rows: u32,
    output: mpsc::Sender<Vec<u8>>,
    flow: Arc<FlowControl>,
) -> Result<SessionHandle, AppError> {
    let pair = native_pty_system()
        .openpty(pty_size(cols, rows))
        .map_err(pty_error)?;
    let mut cmd = CommandBuilder::new(&shell.program);
    cmd.args(&shell.args);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        cmd.cwd(home);
    }
    let mut child = pair.slave.spawn_command(cmd).map_err(pty_error)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(pty_error)?;
    let writer = pair.master.take_writer().map_err(pty_error)?;
    let master = pair.master;
    let mut killer = child.clone_killer();

    // PTY reads and the child wait are blocking: dedicated threads feed async channels.
    std::thread::spawn(move || {
        let mut buf = [0u8; 32 * 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || output.blocking_send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let (exit_tx, mut exit_rx) = mpsc::channel::<()>(1);
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = exit_tx.blocking_send(());
    });
    // Writes can block on a full PTY; keep them off the async runtime.
    let (write_tx, write_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(COMMAND_CAPACITY);
    std::thread::spawn(move || {
        let mut writer = writer;
        while let Ok(data) = write_rx.recv() {
            if writer
                .write_all(&data)
                .and_then(|()| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });

    let (cmd_tx, mut cmd_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (state_tx, state_rx) = watch::channel(SessionState::Ready);
    let span = tracing::info_span!("local", session_id = %id, shell = %shell.id);
    tokio::spawn(
        async move {
            let end = loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => match cmd {
                        Some(SessionCommand::Write(data)) => {
                            if write_tx.send(data).is_err() {
                                break CloseReason::RemoteExited;
                            }
                        }
                        Some(SessionCommand::Resize { cols, rows }) => {
                            let _ = master.resize(pty_size(cols, rows));
                        }
                        Some(SessionCommand::HostKeyDecision(_) | SessionCommand::PromptResponse(_)) => {}
                        Some(SessionCommand::Close) | None => {
                            let _ = killer.kill();
                            break CloseReason::UserClosed;
                        }
                    },
                    _ = exit_rx.recv() => break CloseReason::RemoteExited,
                }
            };
            info!(?end, "local shell ended");
            state_tx.send_replace(SessionState::Closed { reason: end });
        }
        .instrument(span),
    );
    Ok(SessionHandle::new(id, cmd_tx, state_rx, flow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utf16_and_utf8_wsl_lists() {
        let utf16: Vec<u8> = "Ubuntu\r\nDebian\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(parse_wsl_list(&utf16), vec!["Ubuntu", "Debian"]);
        assert_eq!(parse_wsl_list(b"Ubuntu\n\n"), vec!["Ubuntu"]);
    }

    #[test]
    fn detects_at_least_one_shell() {
        assert!(!detect_shells().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn local_shell_runs_a_command_and_closes() {
        let shell = if cfg!(windows) {
            profile("cmd", "cmd", "cmd.exe", &["/Q", "/K", "echo hopr-local-ok"])
        } else {
            profile(
                "sh",
                "sh",
                "/bin/sh",
                &["-c", "echo hopr-local-ok; sleep 5"],
            )
        };
        let (tx, mut rx) = mpsc::channel(64);
        let handle = spawn_local(
            "t".into(),
            &shell,
            80,
            24,
            tx,
            Arc::new(FlowControl::default()),
        )
        .unwrap();
        let mut got = Vec::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut answered = false;
        while !String::from_utf8_lossy(&got).contains("hopr-local-ok") {
            let chunk = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out; got {:?}", String::from_utf8_lossy(&got)))
                .unwrap();
            got.extend_from_slice(&chunk);
            // ConPTY asks for the cursor position at startup and waits for the answer, which
            // xterm.js gives in the app. Play the terminal here.
            if !answered && got.windows(4).any(|w| w == b"\x1b[6n") {
                answered = true;
                handle.try_write(b"\x1b[1;1R".to_vec()).unwrap();
            }
        }
        handle.close().await.unwrap();
        let mut state = handle.state();
        let end = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            state.wait_for(SessionState::is_terminal),
        )
        .await
        .unwrap()
        .unwrap()
        .clone();
        assert!(matches!(end, SessionState::Closed { .. }));
    }
}
