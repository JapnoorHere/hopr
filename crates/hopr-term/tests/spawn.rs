//! A real local shell through ConPTY / a Unix PTY: output arrives and input runs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use hopr_core::session::FlowControl;
use hopr_term::{detect_shells, spawn_local};
use tokio::sync::mpsc;

async fn read_until(rx: &mut mpsc::Receiver<Vec<u8>>, needle: &str) -> String {
    let mut got = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !String::from_utf8_lossy(&got).contains(needle) {
        let chunk = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out; got {:?}", String::from_utf8_lossy(&got)))
            .expect("output closed");
        got.extend_from_slice(&chunk);
    }
    String::from_utf8_lossy(&got).into_owned()
}

/// ConPTY asks the terminal where the cursor is (DSR, `ESC[6n`) before it draws anything
/// else; a terminal must answer (`ESC[row;colR`), as xterm.js does.
#[tokio::test(flavor = "multi_thread")]
async fn local_shell_echoes_a_command() {
    let shell = detect_shells().into_iter().next().expect("a shell");
    let (tx, mut rx) = mpsc::channel(64);
    let handle = spawn_local(
        "t".into(),
        &shell,
        100,
        30,
        tx,
        Arc::new(FlowControl::default()),
    )
    .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(20), rx.recv())
        .await
        .expect("no output from the shell at all")
        .expect("closed");
    eprintln!("first chunk: {:?}", String::from_utf8_lossy(&first));
    if first.windows(4).any(|w| w == b"\x1b[6n") {
        handle.write(b"\x1b[1;1R".to_vec()).await.unwrap();
    }
    handle
        .write(b"echo hopr-local-42\r".to_vec())
        .await
        .unwrap();
    read_until(&mut rx, "hopr-local-42").await;
    handle.close().await.unwrap();
}
