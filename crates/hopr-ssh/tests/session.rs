//! End-to-end tests against an in-process russh server (no Docker needed).
//!
//! The server accepts password "secret", echoes input, records PTY sizes, and understands
//! two commands: `exit` (exit status 0, then close) and `flood` (sends FLOOD_BYTES).

// Test helpers should panic loudly on setup failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use hopr_core::session::{BatchConfig, FlowControl, OUTPUT_CAPACITY, SessionHandle, run_batcher};
use hopr_core::{CloseReason, ErrorCode, SessionState};
use hopr_ssh::{ConnectParams, spawn_session};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

const FLOOD_BYTES: usize = 8 * 1024 * 1024;
const T: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
struct Recorded {
    sizes: Arc<Mutex<Vec<(u32, u32)>>>,
}

#[derive(Clone)]
struct TestServer {
    rec: Recorded,
}

impl server::Server for TestServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == "secret" {
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
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.rec.sizes.lock().unwrap().push((cols, rows));
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, b"ready\r\n".to_vec())?;
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
        self.rec.sizes.lock().unwrap().push((cols, rows));
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match data {
            b"exit\r" => {
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            b"flood\r" => {
                let handle = session.handle();
                tokio::spawn(async move {
                    let chunk: Vec<u8> = (0..32 * 1024).map(|i| (i % 251) as u8).collect();
                    for _ in 0..FLOOD_BYTES / chunk.len() {
                        if handle.data(channel, chunk.clone()).await.is_err() {
                            return;
                        }
                    }
                });
            }
            other => session.data(channel, other.to_vec())?,
        }
        Ok(())
    }
}

async fn start_server() -> (u16, Recorded) {
    let config = Arc::new(server::Config {
        keys: vec![
            russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap(),
        ],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let rec = Recorded::default();
    let mut server = TestServer { rec: rec.clone() };
    tokio::spawn(async move {
        let _ = server.run_on_socket(config, &listener).await;
    });
    (port, rec)
}

fn params(port: u16, password: &str) -> ConnectParams {
    ConnectParams {
        host: "127.0.0.1".into(),
        port,
        username: "tester".into(),
        password: password.to_owned().into(),
        cols: 80,
        rows: 24,
        connect_timeout: T,
    }
}

fn open(port: u16, password: &str) -> (SessionHandle, mpsc::Receiver<Vec<u8>>) {
    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let handle = spawn_session(
        "s1".into(),
        params(port, password),
        tx,
        Arc::new(FlowControl::default()),
    );
    (handle, rx)
}

async fn wait_for(
    state: &mut watch::Receiver<SessionState>,
    pred: impl Fn(&SessionState) -> bool,
) -> SessionState {
    tokio::time::timeout(T, state.wait_for(|s| pred(s)))
        .await
        .expect("state timeout")
        .unwrap()
        .clone()
}

async fn read_until(rx: &mut mpsc::Receiver<Vec<u8>>, needle: &[u8]) -> Vec<u8> {
    let mut got = Vec::new();
    while !got.windows(needle.len()).any(|w| w == needle) {
        let chunk = tokio::time::timeout(T, rx.recv())
            .await
            .expect("read timeout")
            .expect("closed");
        got.extend_from_slice(&chunk);
    }
    got
}

#[tokio::test]
async fn connects_opens_pty_and_echoes() {
    let (port, rec) = start_server().await;
    let (session, mut out) = open(port, "secret");
    let mut state = session.state();
    wait_for(&mut state, |s| *s == SessionState::Ready).await;
    read_until(&mut out, b"ready").await;
    assert_eq!(rec.sizes.lock().unwrap().first(), Some(&(80, 24)));

    session.write(b"hello".to_vec()).await.unwrap();
    read_until(&mut out, b"hello").await;
}

#[tokio::test]
async fn resize_reaches_the_server() {
    let (port, rec) = start_server().await;
    let (session, mut out) = open(port, "secret");
    read_until(&mut out, b"ready").await;
    session.resize(132, 43).await.unwrap();
    session.write(b"sync".to_vec()).await.unwrap();
    read_until(&mut out, b"sync").await;
    assert!(rec.sizes.lock().unwrap().contains(&(132, 43)));
}

#[tokio::test]
async fn wrong_password_fails_with_auth_failed() {
    let (port, _) = start_server().await;
    let (session, _out) = open(port, "nope");
    let mut state = session.state();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    match end {
        SessionState::Failed { error } => assert_eq!(error.code, ErrorCode::AuthFailed),
        other => panic!("expected auth failure, got {other:?}"),
    }
}

#[tokio::test]
async fn closed_port_fails_with_connect_failed() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (session, _out) = open(port, "secret");
    let mut state = session.state();
    match wait_for(&mut state, SessionState::is_terminal).await {
        SessionState::Failed { error } => {
            assert_eq!(error.code, ErrorCode::ConnectFailed);
            assert!(error.retryable);
        }
        other => panic!("expected connect failure, got {other:?}"),
    }
}

#[tokio::test]
async fn remote_exit_reports_remote_exited() {
    let (port, _) = start_server().await;
    let (session, mut out) = open(port, "secret");
    read_until(&mut out, b"ready").await;
    session.write(b"exit\r".to_vec()).await.unwrap();
    let mut state = session.state();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert_eq!(
        end,
        SessionState::Closed {
            reason: CloseReason::RemoteExited
        }
    );
}

#[tokio::test]
async fn user_close_reports_user_closed() {
    let (port, _) = start_server().await;
    let (session, mut out) = open(port, "secret");
    read_until(&mut out, b"ready").await;
    session.close().await.unwrap();
    let mut state = session.state();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert_eq!(
        end,
        SessionState::Closed {
            reason: CloseReason::UserClosed
        }
    );
}

/// 8 MiB through the real pipeline (actor → bounded queue → batcher → flow control) with
/// a deliberately slow, acking consumer: every byte arrives, in order, and the amount of
/// unacknowledged output never exceeds the flow-control window plus one batch.
#[tokio::test]
async fn flood_is_delivered_intact_under_backpressure() {
    let (port, _) = start_server().await;
    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let flow = Arc::new(FlowControl::new(256 * 1024, 64 * 1024));
    let session = spawn_session("flood".into(), params(port, "secret"), tx, flow.clone());

    let (ui_tx, mut ui_rx) = mpsc::channel::<Vec<u8>>(1024);
    tokio::spawn(run_batcher(
        rx,
        flow.clone(),
        BatchConfig::default(),
        move |b| ui_tx.try_send(b).is_ok(),
    ));

    let mut got = Vec::with_capacity(FLOOD_BYTES + 64);
    let mut peak_unacked = 0;
    // Drain the banner first.
    while !got.windows(5).any(|w| w == b"ready") {
        let b = tokio::time::timeout(T, ui_rx.recv())
            .await
            .unwrap()
            .unwrap();
        session.ack(b.len() as u64);
        got.extend_from_slice(&b);
    }
    got.clear();
    session.write(b"flood\r".to_vec()).await.unwrap();
    while got.len() < FLOOD_BYTES {
        let b = tokio::time::timeout(T, ui_rx.recv())
            .await
            .unwrap()
            .unwrap();
        peak_unacked = peak_unacked.max(flow.unacked());
        tokio::time::sleep(Duration::from_micros(200)).await; // slow renderer
        session.ack(b.len() as u64);
        got.extend_from_slice(&b);
    }
    assert_eq!(got.len(), FLOOD_BYTES);
    assert!(
        got.iter()
            .enumerate()
            .all(|(i, &b)| b == ((i % (32 * 1024)) % 251) as u8),
        "corrupted stream"
    );
    assert!(
        peak_unacked <= 256 * 1024 + BatchConfig::default().max_bytes as u64,
        "flow control exceeded: {peak_unacked}"
    );
}
