//! Headless throughput probe: connect with hopr-ssh, run a command in the PTY shell and
//! report how fast output arrives, with no UI in the path.
//!
//! cargo run --release -p hopr-ssh --example throughput -- <host> <port> <user> <password> <command> <marker>
//! The run ends when `marker` appears in the output.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use hopr_core::session::{FlowControl, OUTPUT_CAPACITY};
use hopr_core::{HostKeyDecision, SessionState};
use hopr_ssh::{ConnectParams, Credential, Hop, SessionHooks, spawn_session};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, user, password, command, marker] = a.as_slice() else {
        eprintln!("usage: throughput <host> <port> <user> <password> <command> <marker>");
        std::process::exit(2);
    };
    let (tx, mut rx) = mpsc::channel(OUTPUT_CAPACITY);
    let session = spawn_session(
        "probe".into(),
        ConnectParams {
            hops: vec![Hop {
                host: host.clone(),
                port: port.parse().unwrap(),
                username: user.clone(),
                credentials: vec![Credential::Password(password.clone().into())],
                known_hosts: Vec::new(),
                on_save_password: None,
            }],
            agent_forwarding: false,
            proxy: None,
            proxy_command: None,
            reconnect: None,
            cols: 200,
            rows: 50,
            connect_timeout: Duration::from_secs(20),
            hooks: SessionHooks::default(),
        },
        tx,
        Arc::new(FlowControl::default()),
    );
    let mut state = session.state();
    // A local probe: accept the dev server's (unknown) host key for this run only.
    loop {
        let s = state.borrow_and_update().clone();
        match s {
            SessionState::Ready => break,
            SessionState::VerifyingHostKey { .. } => session
                .try_host_key_decision(HostKeyDecision::Once)
                .unwrap(),
            s if s.is_terminal() => panic!("connect failed: {s:?}"),
            _ => {}
        }
        state.changed().await.unwrap();
    }
    tokio::time::sleep(Duration::from_secs(2)).await; // let the prompt settle
    while rx.try_recv().is_ok() {}

    session
        .write(format!("{command}\r").into_bytes())
        .await
        .unwrap();
    let start = Instant::now();
    let mut total = 0usize;
    let mut tail: Vec<u8> = Vec::new();
    let needle = marker.as_bytes();
    while let Some(chunk) = rx.recv().await {
        total += chunk.len();
        tail.extend_from_slice(&chunk);
        // Build the marker at runtime in `command` (e.g. "A" + "B") so the echo can't match.
        if tail.windows(needle.len()).any(|w| w == needle) {
            break;
        }
        if tail.len() > 4096 {
            tail.drain(..tail.len() - 4096);
        }
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "received {:.2} MB in {secs:.2} s => {:.2} MB/s",
        total as f64 / 1e6,
        total as f64 / 1e6 / secs
    );
    let _ = session.close().await;
}
