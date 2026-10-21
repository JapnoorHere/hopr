//! Telnet session against a scripted server on loopback: negotiation, output with
//! escaped bytes, typed input (CR NUL), window resizes (NAWS) and remote close.

use std::sync::Arc;
use std::time::Duration;

use hopr_core::session::FlowControl;
use hopr_core::{CloseReason, SessionState};
use hopr_term::telnet::spawn_telnet;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const IAC: u8 = 255;
const WILL: u8 = 251;
const DO: u8 = 253;
const SB: u8 = 250;
const SE: u8 = 240;
const NAWS: u8 = 31;
const ECHO: u8 = 1;

async fn read_until(s: &mut tokio::net::TcpStream, seen: &mut Vec<u8>, want: &[u8]) {
    let mut buf = [0u8; 1024];
    tokio::time::timeout(Duration::from_secs(5), async {
        while !seen.windows(want.len()).any(|w| w == want) {
            let Ok(n) = s.read(&mut buf).await else {
                panic!("server read failed");
            };
            assert!(n > 0, "client hung up early; got {seen:?}");
            seen.extend_from_slice(&buf[..n]);
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {want:?}; got {seen:?}"));
}

#[tokio::test]
async fn telnet_session_end_to_end() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut seen = Vec::new();
        // The client offers NAWS up front; accept it and take over echo.
        read_until(&mut s, &mut seen, &[IAC, WILL, NAWS]).await;
        s.write_all(&[IAC, DO, NAWS, IAC, WILL, ECHO])
            .await
            .unwrap();
        read_until(&mut s, &mut seen, &[IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]).await;
        read_until(&mut s, &mut seen, &[IAC, DO, ECHO]).await;
        s.write_all(b"login: \xff\xffok").await.unwrap();
        read_until(&mut s, &mut seen, b"admin\r\0").await;
        read_until(&mut s, &mut seen, &[IAC, SB, NAWS, 0, 120, 0, 40, IAC, SE]).await;
        s.write_all(b"\r\nbye\r\n").await.unwrap();
        // Hang up: the session must end as "remote exited".
    });

    let (tx, mut rx) = mpsc::channel(64);
    let handle = spawn_telnet(
        "t1".into(),
        "127.0.0.1".into(),
        port,
        80,
        24,
        Duration::from_secs(5),
        tx,
        Arc::new(FlowControl::default()),
    );
    let mut state = handle.state();
    tokio::time::timeout(
        Duration::from_secs(5),
        state.wait_for(|s| *s == SessionState::Ready),
    )
    .await
    .unwrap()
    .unwrap();

    let mut screen = Vec::new();
    while !screen.ends_with(b"ok") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        screen.extend(chunk);
    }
    assert_eq!(screen, b"login: \xffok", "IAC IAC shows as one 0xFF byte");
    handle.write(b"admin\r".to_vec()).await.unwrap();
    handle.resize(120, 40).await.unwrap();

    let closed = tokio::time::timeout(
        Duration::from_secs(5),
        state.wait_for(|s| matches!(s, SessionState::Closed { .. })),
    )
    .await
    .unwrap()
    .unwrap()
    .clone();
    assert_eq!(
        closed,
        SessionState::Closed {
            reason: CloseReason::RemoteExited
        }
    );
    server.await.unwrap();
}

#[tokio::test]
async fn telnet_refused_port_fails_with_connect_error() {
    // Bind then drop: nothing listens on this port any more.
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let (tx, _rx) = mpsc::channel(4);
    let handle = spawn_telnet(
        "t2".into(),
        "127.0.0.1".into(),
        port,
        80,
        24,
        Duration::from_secs(5),
        tx,
        Arc::new(FlowControl::default()),
    );
    let mut state = handle.state();
    let failed = tokio::time::timeout(
        Duration::from_secs(10),
        state.wait_for(|s| matches!(s, SessionState::Failed { .. })),
    )
    .await
    .unwrap()
    .unwrap()
    .clone();
    let SessionState::Failed { error } = failed else {
        unreachable!()
    };
    assert_eq!(error.code, hopr_core::ErrorCode::ConnectFailed);
    assert!(error.message.contains(&port.to_string()));
}
