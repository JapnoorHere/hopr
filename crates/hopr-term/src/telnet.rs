//! Telnet client (RFC 854): option negotiation for echo, suppress-go-ahead, terminal
//! type and window size (NAWS), and a session actor with the same `SessionHandle`
//! protocol as SSH and local shells. Telnet is plain text; the UI says so.

use std::sync::Arc;
use std::time::Duration;

use hopr_core::session::{COMMAND_CAPACITY, FlowControl, SessionCommand, SessionHandle};
use hopr_core::{AppError, CloseReason, ErrorCode, SessionId, SessionState};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info};

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;

const ECHO: u8 = 1;
const SGA: u8 = 3;
const TTYPE: u8 = 24;
const NAWS: u8 = 31;

const TTYPE_IS: u8 = 0;
const TTYPE_SEND: u8 = 1;
const TERMINAL_TYPE: &[u8] = b"XTERM-256COLOR";

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    Iac,
    Command(u8),
    Sub,
    SubIac,
}

/// Telnet protocol state: turns server bytes into screen bytes plus replies.
pub struct Telnet {
    state: State,
    sub: Vec<u8>,
    /// Options we perform (we said WILL and the server didn't refuse).
    local: [bool; 256],
    /// Options the server performs (it said WILL, or we asked with DO).
    remote: [bool; 256],
    cols: u16,
    rows: u16,
}

impl Telnet {
    /// New state for a terminal of `cols` × `rows`.
    pub fn new(cols: u32, rows: u32) -> Self {
        Self {
            state: State::Data,
            sub: Vec::new(),
            local: [false; 256],
            remote: [false; 256],
            cols: clamp16(cols),
            rows: clamp16(rows),
        }
    }

    /// What a client says first: it will report its window size and terminal type, and
    /// asks the server to suppress go-ahead.
    pub fn greeting(&mut self) -> Vec<u8> {
        self.local[usize::from(NAWS)] = true;
        self.local[usize::from(TTYPE)] = true;
        self.remote[usize::from(SGA)] = true;
        vec![IAC, WILL, NAWS, IAC, WILL, TTYPE, IAC, DO, SGA]
    }

    /// Bytes from the server → (bytes for the screen, replies to send back). Sequences
    /// split across reads are carried over.
    pub fn feed(&mut self, input: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut screen = Vec::with_capacity(input.len());
        let mut reply = Vec::new();
        for &b in input {
            self.state = match self.state {
                State::Data if b == IAC => State::Iac,
                State::Data => {
                    screen.push(b);
                    State::Data
                }
                State::Iac => match b {
                    IAC => {
                        screen.push(IAC); // escaped 255
                        State::Data
                    }
                    DO | DONT | WILL | WONT => State::Command(b),
                    SB => {
                        self.sub.clear();
                        State::Sub
                    }
                    _ => State::Data, // NOP, GA, AYT…: nothing to show
                },
                State::Command(cmd) => {
                    self.negotiate(cmd, b, &mut reply);
                    State::Data
                }
                State::Sub if b == IAC => State::SubIac,
                State::Sub => {
                    if self.sub.len() < 1024 {
                        self.sub.push(b);
                    }
                    State::Sub
                }
                State::SubIac => match b {
                    SE => {
                        self.subnegotiation(&mut reply);
                        State::Data
                    }
                    IAC => {
                        self.sub.push(IAC);
                        State::Sub
                    }
                    _ => State::Data,
                },
            };
        }
        (screen, reply)
    }

    fn negotiate(&mut self, cmd: u8, opt: u8, reply: &mut Vec<u8>) {
        let o = usize::from(opt);
        match cmd {
            WILL => {
                let wanted = matches!(opt, ECHO | SGA);
                if wanted && !self.remote[o] {
                    self.remote[o] = true;
                    reply.extend([IAC, DO, opt]);
                } else if !wanted {
                    reply.extend([IAC, DONT, opt]);
                }
            }
            WONT if self.remote[o] => {
                self.remote[o] = false;
                reply.extend([IAC, DONT, opt]);
            }
            DO => match opt {
                NAWS | TTYPE => {
                    if !self.local[o] {
                        self.local[o] = true;
                        reply.extend([IAC, WILL, opt]);
                    }
                    if opt == NAWS {
                        reply.extend(self.naws());
                    }
                }
                _ => reply.extend([IAC, WONT, opt]),
            },
            DONT if self.local[o] => {
                self.local[o] = false;
                reply.extend([IAC, WONT, opt]);
            }
            _ => {}
        }
    }

    fn subnegotiation(&mut self, reply: &mut Vec<u8>) {
        if self.sub.first() == Some(&TTYPE) && self.sub.get(1) == Some(&TTYPE_SEND) {
            reply.extend([IAC, SB, TTYPE, TTYPE_IS]);
            reply.extend_from_slice(TERMINAL_TYPE);
            reply.extend([IAC, SE]);
        }
    }

    fn naws(&self) -> Vec<u8> {
        let mut out = vec![IAC, SB, NAWS];
        for byte in self
            .cols
            .to_be_bytes()
            .into_iter()
            .chain(self.rows.to_be_bytes())
        {
            out.push(byte);
            if byte == IAC {
                out.push(IAC);
            }
        }
        out.extend([IAC, SE]);
        out
    }

    /// The window changed: the new size, if the server accepted NAWS.
    pub fn resize(&mut self, cols: u32, rows: u32) -> Vec<u8> {
        self.cols = clamp16(cols);
        self.rows = clamp16(rows);
        if self.local[usize::from(NAWS)] {
            self.naws()
        } else {
            Vec::new()
        }
    }

    /// User input for the wire: 255 doubled, a bare CR sent as CR NUL (RFC 854).
    pub fn encode(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len() + 4);
        for (i, &b) in input.iter().enumerate() {
            match b {
                IAC => out.extend([IAC, IAC]),
                b'\r' if input.get(i + 1) != Some(&b'\n') => out.extend([b'\r', 0]),
                _ => out.push(b),
            }
        }
        out
    }
}

fn clamp16(n: u32) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn connect_error(host: &str, port: u16, e: &std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ConnectFailed,
        format!("Couldn't reach {host}:{port}"),
    )
    .with_detail(e.to_string())
}

/// Open a telnet session. Returns at once; the connection is made by the actor, which
/// reports `Connecting`, then `Ready` or `Failed`.
#[allow(clippy::too_many_arguments)] // mirrors the other spawn_* entry points
pub fn spawn_telnet(
    id: SessionId,
    host: String,
    port: u16,
    cols: u32,
    rows: u32,
    connect_timeout: Duration,
    output: mpsc::Sender<Vec<u8>>,
    flow: Arc<FlowControl>,
) -> SessionHandle {
    let (cmd_tx, mut cmd_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (state_tx, state_rx) = watch::channel(SessionState::Connecting { hop: 1, total: 1 });
    let span = tracing::info_span!("telnet", session_id = %id, %host, port);
    tokio::spawn(
        async move {
            let connect = TcpStream::connect((host.as_str(), port));
            let stream = match tokio::time::timeout(connect_timeout, connect).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    state_tx.send_replace(SessionState::Failed {
                        error: connect_error(&host, port, &e),
                    });
                    return;
                }
                Err(_) => {
                    state_tx.send_replace(SessionState::Failed {
                        error: AppError::new(
                            ErrorCode::Timeout,
                            format!("{host}:{port} didn't answer in time"),
                        ),
                    });
                    return;
                }
            };
            let _ = stream.set_nodelay(true);
            let (mut rd, mut wr) = stream.into_split();
            let mut tn = Telnet::new(cols, rows);
            if wr.write_all(&tn.greeting()).await.is_err() {
                state_tx.send_replace(SessionState::Closed {
                    reason: CloseReason::ConnectionLost,
                });
                return;
            }
            state_tx.send_replace(SessionState::Ready);
            let mut buf = vec![0u8; 32 * 1024];
            let end = loop {
                tokio::select! {
                    n = rd.read(&mut buf) => match n {
                        Ok(0) => break CloseReason::RemoteExited,
                        Ok(n) => {
                            let (screen, reply) = tn.feed(&buf[..n]);
                            if !reply.is_empty() && wr.write_all(&reply).await.is_err() {
                                break CloseReason::ConnectionLost;
                            }
                            if !screen.is_empty() && output.send(screen).await.is_err() {
                                break CloseReason::UserClosed;
                            }
                        }
                        Err(_) => break CloseReason::ConnectionLost,
                    },
                    cmd = cmd_rx.recv() => match cmd {
                        Some(SessionCommand::Write(data)) => {
                            if wr.write_all(&Telnet::encode(&data)).await.is_err() {
                                break CloseReason::ConnectionLost;
                            }
                        }
                        Some(SessionCommand::Resize { cols, rows }) => {
                            let naws = tn.resize(cols, rows);
                            if !naws.is_empty() && wr.write_all(&naws).await.is_err() {
                                break CloseReason::ConnectionLost;
                            }
                        }
                        Some(SessionCommand::HostKeyDecision(_) | SessionCommand::PromptResponse(_)) => {}
                        Some(SessionCommand::Close) | None => {
                            let _ = wr.shutdown().await;
                            break CloseReason::UserClosed;
                        }
                    },
                }
            };
            info!(?end, "telnet session ended");
            state_tx.send_replace(SessionState::Closed { reason: end });
        }
        .instrument(span),
    );
    SessionHandle::new(id, cmd_tx, state_rx, flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_echo_naws_and_terminal_type() {
        let mut t = Telnet::new(80, 24);
        let (screen, reply) = t.feed(&[IAC, WILL, ECHO, IAC, DO, NAWS, IAC, DO, 39]);
        assert!(screen.is_empty());
        assert_eq!(
            reply,
            [
                IAC, DO, ECHO, // accept server echo
                IAC, WILL, NAWS, IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE, // size
                IAC, WONT, 39, // refuse anything else
            ]
        );
        // Asked again: no loop.
        let (_, again) = t.feed(&[IAC, WILL, ECHO]);
        assert!(again.is_empty());
        let (_, tt) = t.feed(&[IAC, SB, TTYPE, TTYPE_SEND, IAC, SE]);
        let mut want = vec![IAC, SB, TTYPE, TTYPE_IS];
        want.extend_from_slice(TERMINAL_TYPE);
        want.extend([IAC, SE]);
        assert_eq!(tt, want);
    }

    #[test]
    fn data_with_escapes_and_split_sequences() {
        let mut t = Telnet::new(80, 24);
        let (a, r1) = t.feed(&[b'h', b'i', IAC]);
        let (b, r2) = t.feed(&[IAC, b'x', IAC]);
        let (c, r3) = t.feed(&[WILL]);
        let (d, r4) = t.feed(&[SGA, b'!']);
        assert_eq!([a, b, c, d].concat(), b"hi\xffx!");
        assert!(r1.is_empty() && r2.is_empty() && r3.is_empty());
        assert_eq!(r4, [IAC, DO, SGA]);
    }

    #[test]
    fn encodes_input_and_resizes_once_naws_is_on() {
        assert_eq!(Telnet::encode(b"ls\r"), b"ls\r\0");
        assert_eq!(Telnet::encode(b"a\r\nb"), b"a\r\nb");
        assert_eq!(Telnet::encode(&[1, IAC, 2]), [1, IAC, IAC, 2]);
        let mut t = Telnet::new(80, 24);
        assert!(
            t.resize(100, 30).is_empty(),
            "no NAWS before the server agrees"
        );
        t.feed(&[IAC, DO, NAWS]);
        assert_eq!(
            t.resize(255, 30),
            [IAC, SB, NAWS, 0, IAC, IAC, 0, 30, IAC, SE]
        );
        t.feed(&[IAC, DONT, NAWS]);
        assert!(t.resize(90, 30).is_empty());
    }
}
