//! End-to-end tests against in-process russh servers (no Docker needed).
//!
//! The server accepts password "secret", public key `TestServer::allowed_key`, and
//! keyboard-interactive "Password + Verification code" (TOTP from `SEED`). It echoes input,
//! records PTY sizes, forwards `direct-tcpip` (so it can act as a jump host), and
//! understands `exit`, `flood` and `drop` (kills the connection without an exit status).

// Test helpers should panic loudly on setup failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::borrow::Cow;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use data_encoding::BASE64;
use hopr_core::model::{KeyAlgorithm, KnownHost};
use hopr_core::session::{
    BatchConfig, FlowControl, OUTPUT_CAPACITY, PromptAnswer, SessionHandle, run_batcher,
};
use hopr_core::{CloseReason, ErrorCode, HostKeyDecision, SessionState};
use hopr_ssh::{
    ConnectParams, Credential, Hop, Reconnect, SessionHooks, TrustedKey, keys, spawn_session, totp,
};
use russh::keys::{HashAlg, PublicKeyBase64};
use russh::server::{self, Auth, Msg, Response, Server as _, Session};
use russh::{Channel, ChannelId};
use secrecy::{ExposeSecret, SecretString};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

const FLOOD_BYTES: usize = 8 * 1024 * 1024;
const T: Duration = Duration::from_secs(10);
const SEED: &str = "JBSWY3DPEHPK3PXP";

#[derive(Clone, Default)]
struct Recorded {
    sizes: Arc<Mutex<Vec<(u32, u32)>>>,
    /// `host:port` targets requested through direct-tcpip.
    tunnels: Arc<Mutex<Vec<String>>>,
    /// Shells opened (counts reconnects).
    shells: Arc<AtomicU32>,
}

#[derive(Clone)]
struct TestServer {
    rec: Recorded,
    allowed_key: Option<String>, // fingerprint
    /// The shell channel; `data()` also sees tunnel traffic, which must not be echoed.
    shell: Option<ChannelId>,
    /// An exec channel: its command and the stdin received so far.
    exec: Option<(ChannelId, String, Vec<u8>)>,
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

    async fn auth_publickey(
        &mut self,
        _user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        Ok(if self.allowed_key.as_deref() == Some(fp.as_str()) {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    /// Certificates signed by the CA whose fingerprint is `allowed_key`, for this user.
    async fn auth_openssh_certificate(
        &mut self,
        user: &str,
        cert: &russh::keys::Certificate,
    ) -> Result<Auth, Self::Error> {
        let ca = cert
            .signature_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ok = self.allowed_key.as_deref() == Some(ca.as_str())
            && cert.valid_principals().iter().any(|p| p == user)
            && cert.valid_after() <= now
            && now < cert.valid_before();
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        response: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        let Some(response) = response else {
            return Ok(Auth::Partial {
                name: Cow::Borrowed("Two-factor login"),
                instructions: Cow::Borrowed(""),
                prompts: Cow::Owned(vec![
                    (Cow::Borrowed("Password: "), false),
                    (Cow::Borrowed("Verification code: "), false),
                ]),
            });
        };
        let answers: Vec<String> = response
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .collect();
        let ok = answers.len() == 2
            && answers[0] == "secret"
            && Some(answers[1].as_str()) == totp::current_code(SEED).as_deref();
        Ok(if ok { Auth::Accept } else { Auth::reject() })
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
        let target = format!("{host_to_connect}:{port_to_connect}");
        self.rec.tunnels.lock().unwrap().push(target.clone());
        match tokio::net::TcpStream::connect(&target).await {
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

    /// `-R`: listen on the requested port and send each connection back as a
    /// forwarded-tcpip channel.
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let Ok(listener) = TcpListener::bind((address, u16::try_from(*port).unwrap_or(0))).await
        else {
            return Ok(false);
        };
        let bound = u32::from(listener.local_addr().unwrap().port());
        *port = bound;
        let handle = session.handle();
        let address = address.to_owned();
        tokio::spawn(async move {
            while let Ok((mut tcp, peer)) = listener.accept().await {
                let handle = handle.clone();
                let address = address.clone();
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
        self.rec.shells.fetch_add(1, Ordering::SeqCst);
        self.shell = Some(channel);
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

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.exec = Some((
            channel,
            String::from_utf8_lossy(data).into_owned(),
            Vec::new(),
        ));
        session.channel_success(channel)?;
        Ok(())
    }

    /// Exec channels answer once stdin ends: "ran <command> with <n> bytes", exit 3.
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some((id, cmd, stdin)) = self.exec.take_if(|(id, ..)| *id == channel) {
            let reply = format!("ran {cmd} with {} bytes\n", stdin.len());
            session.data(id, reply.into_bytes())?;
            session.extended_data(id, 1, b"warn\n".to_vec())?;
            session.exit_status_request(id, 3)?;
            session.eof(id)?;
            session.close(id)?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some((id, _, stdin)) = self.exec.as_mut()
            && *id == channel
        {
            stdin.extend_from_slice(data);
            return Ok(());
        }
        if self.shell != Some(channel) {
            return Ok(());
        }
        match data {
            b"exit\r" => {
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            // Simulates a network drop: the connection dies with no exit status.
            b"drop\r" => return Err(russh::Error::Disconnect),
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

struct Server {
    port: u16,
    rec: Recorded,
    /// Trusted-hosts entry matching this server's key.
    known: KnownHost,
}

async fn start_server(allowed_key: Option<String>) -> Server {
    let host_key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let public = host_key.public_key().clone();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let rec = Recorded::default();
    let mut server = TestServer {
        rec: rec.clone(),
        allowed_key,
        shell: None,
        exec: None,
    };
    tokio::spawn(async move {
        let _ = server.run_on_socket(config, &listener).await;
    });
    let known = KnownHost {
        id: "k".into(),
        host_pattern: "127.0.0.1".into(),
        port,
        key_type: public.algorithm().as_str().into(),
        key_b64: BASE64.encode(&public.public_key_bytes()),
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        first_seen_at: String::new(),
        last_seen_at: String::new(),
        source: "test".into(),
    };
    Server { port, rec, known }
}

fn hop(s: &Server, credentials: Vec<Credential>) -> Hop {
    Hop {
        host: "127.0.0.1".into(),
        port: s.port,
        username: "tester".into(),
        credentials,
        known_hosts: vec![s.known.clone()],
        on_save_password: None,
    }
}

fn chain(hops: Vec<Hop>) -> ConnectParams {
    ConnectParams {
        hops,
        agent_forwarding: false,
        proxy: None,
        proxy_command: None,
        cols: 80,
        rows: 24,
        connect_timeout: T,
        hooks: SessionHooks::default(),
        reconnect: None,
    }
}

fn params(s: &Server, credentials: Vec<Credential>) -> ConnectParams {
    chain(vec![hop(s, credentials)])
}

fn pw(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

fn spawn(p: ConnectParams) -> (SessionHandle, mpsc::Receiver<Vec<u8>>) {
    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let handle = spawn_session("s1".into(), p, tx, Arc::new(FlowControl::default()));
    (handle, rx)
}

fn open(s: &Server, password: &str) -> (SessionHandle, mpsc::Receiver<Vec<u8>>) {
    spawn(params(s, vec![Credential::Password(pw(password))]))
}

async fn wait_for(
    state: &mut watch::Receiver<SessionState>,
    pred: impl Fn(&SessionState) -> bool,
) -> SessionState {
    let found = tokio::time::timeout(T, state.wait_for(|s| pred(s)))
        .await
        .expect("state timeout")
        .map(|s| s.clone());
    found.unwrap_or_else(|_| panic!("session ended in {:?}", *state.borrow()))
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

fn is_ready(s: &SessionState) -> bool {
    *s == SessionState::Ready
}

#[tokio::test]
async fn connects_opens_pty_and_echoes() {
    let srv = start_server(None).await;
    let (session, mut out) = open(&srv, "secret");
    let mut state = session.state();
    wait_for(&mut state, is_ready).await;
    read_until(&mut out, b"ready").await;
    assert_eq!(srv.rec.sizes.lock().unwrap().first(), Some(&(80, 24)));
    session.write(b"hello".to_vec()).await.unwrap();
    read_until(&mut out, b"hello").await;
}

#[tokio::test]
async fn resize_reaches_the_server() {
    let srv = start_server(None).await;
    let (session, mut out) = open(&srv, "secret");
    read_until(&mut out, b"ready").await;
    session.resize(132, 43).await.unwrap();
    session.write(b"sync".to_vec()).await.unwrap();
    read_until(&mut out, b"sync").await;
    assert!(srv.rec.sizes.lock().unwrap().contains(&(132, 43)));
}

#[tokio::test]
async fn wrong_password_fails_with_auth_failed() {
    let srv = start_server(None).await;
    let (session, _out) = open(&srv, "nope");
    let mut state = session.state();
    match wait_for(&mut state, SessionState::is_terminal).await {
        SessionState::Failed { error } => assert_eq!(error.code, ErrorCode::AuthFailed),
        other => panic!("expected auth failure, got {other:?}"),
    }
}

#[tokio::test]
async fn closed_port_fails_with_connect_failed() {
    let srv = start_server(None).await;
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.hops[0].port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (session, _out) = spawn(p);
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
    let srv = start_server(None).await;
    let (session, mut out) = open(&srv, "secret");
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
    let srv = start_server(None).await;
    let (session, mut out) = open(&srv, "secret");
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

#[tokio::test]
async fn unknown_host_key_asks_and_trust_is_persisted() {
    let srv = start_server(None).await;
    let trusted: Arc<Mutex<Vec<TrustedKey>>> = Arc::default();
    let sink = trusted.clone();
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.hops[0].known_hosts.clear();
    p.hooks.on_trust = Some(Arc::new(move |k| sink.lock().unwrap().push(k)));
    let (session, _out) = spawn(p);
    let mut state = session.state();
    let SessionState::VerifyingHostKey { prompt } = wait_for(&mut state, |s| {
        matches!(s, SessionState::VerifyingHostKey { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert!(!prompt.changed && prompt.previous.is_empty());
    assert_eq!(prompt.fingerprint, srv.known.fingerprint);
    session
        .try_host_key_decision(HostKeyDecision::Trust)
        .unwrap();
    wait_for(&mut state, is_ready).await;
    let saved = trusted.lock().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].key_b64, srv.known.key_b64);
    assert!(!saved[0].replace);
}

#[tokio::test]
async fn rejecting_a_new_host_key_closes_quietly() {
    let srv = start_server(None).await;
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.hops[0].known_hosts.clear();
    let (session, _out) = spawn(p);
    let mut state = session.state();
    wait_for(&mut state, |s| {
        matches!(s, SessionState::VerifyingHostKey { .. })
    })
    .await;
    session
        .try_host_key_decision(HostKeyDecision::Reject)
        .unwrap();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert_eq!(
        end,
        SessionState::Closed {
            reason: CloseReason::UserClosed
        }
    );
}

#[tokio::test]
async fn changed_host_key_is_flagged_and_rejection_blocks() {
    let srv = start_server(None).await;
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.hops[0].known_hosts[0].key_b64 = "AAAAC3NzaC1lZDI1NTE5AAAAIOldKey".into();
    p.hops[0].known_hosts[0].fingerprint = "SHA256:old".into();
    let (session, _out) = spawn(p);
    let mut state = session.state();
    let SessionState::VerifyingHostKey { prompt } = wait_for(&mut state, |s| {
        matches!(s, SessionState::VerifyingHostKey { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert!(prompt.changed);
    assert_eq!(prompt.previous, vec!["SHA256:old".to_owned()]);
    session
        .try_host_key_decision(HostKeyDecision::Reject)
        .unwrap();
    match wait_for(&mut state, SessionState::is_terminal).await {
        SessionState::Failed { error } => assert_eq!(error.code, ErrorCode::HostKeyMismatch),
        other => panic!("expected host key mismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn public_key_auth() {
    let key = keys::generate(KeyAlgorithm::Ed25519, "test").unwrap();
    let srv = start_server(Some(key.fingerprint.clone())).await;
    let loaded = keys::load(&key.private_openssh).unwrap();
    let (session, _out) = spawn(params(&srv, vec![Credential::Key(loaded)]));
    let mut state = session.state();
    wait_for(&mut state, |s| is_ready(s) || s.is_terminal()).await;
    assert_eq!(*state.borrow(), SessionState::Ready);

    // A key the server doesn't know is rejected.
    let other = keys::load(
        &keys::generate(KeyAlgorithm::Ed25519, "")
            .unwrap()
            .private_openssh,
    )
    .unwrap();
    let (session, _out) = spawn(params(&srv, vec![Credential::Key(other)]));
    let mut state = session.state();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert!(matches!(end, SessionState::Failed { error } if error.code == ErrorCode::AuthFailed));
}

#[tokio::test]
async fn keyboard_interactive_fills_password_and_totp_without_asking() {
    let srv = start_server(None).await;
    let creds = vec![Credential::KeyboardInteractive {
        password: Some(pw("secret")),
        totp_seed: Some(pw(SEED)),
    }];
    let (session, _out) = spawn(params(&srv, creds));
    let mut state = session.state();
    let end = wait_for(&mut state, |s| {
        is_ready(s) || s.is_terminal() || matches!(s, SessionState::Prompting { .. })
    })
    .await;
    assert_eq!(end, SessionState::Ready);
}

#[tokio::test]
async fn keyboard_interactive_asks_for_what_it_does_not_know() {
    let srv = start_server(None).await;
    let creds = vec![Credential::KeyboardInteractive {
        password: Some(pw("secret")),
        totp_seed: None,
    }];
    let (session, _out) = spawn(params(&srv, creds));
    let mut state = session.state();
    let SessionState::Prompting { prompt } =
        wait_for(&mut state, |s| matches!(s, SessionState::Prompting { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(prompt.title, "Two-factor login");
    assert_eq!(prompt.fields.len(), 1, "only the code is unknown");
    assert!(prompt.fields[0].label.contains("Verification"));
    let code = totp::current_code(SEED).unwrap();
    session
        .try_prompt_response(Some(PromptAnswer {
            responses: vec![pw(&code)],
            remember: false,
        }))
        .unwrap();
    wait_for(&mut state, is_ready).await;
}

#[tokio::test]
async fn password_prompt_retries_and_remembers() {
    let srv = start_server(None).await;
    let saved: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = saved.clone();
    let mut p = params(&srv, vec![Credential::PromptPassword]);
    p.hops[0].on_save_password = Some(Arc::new(move |s| {
        sink.lock().unwrap().push(s.expose_secret().to_owned())
    }));
    let (session, _out) = spawn(p);
    let mut state = session.state();

    let SessionState::Prompting { prompt } =
        wait_for(&mut state, |s| matches!(s, SessionState::Prompting { .. })).await
    else {
        unreachable!()
    };
    assert!(prompt.can_remember);
    session
        .try_prompt_response(Some(PromptAnswer {
            responses: vec![pw("nope")],
            remember: true,
        }))
        .unwrap();

    let SessionState::Prompting { prompt } = wait_for(
        &mut state,
        |s| matches!(s, SessionState::Prompting { prompt } if !prompt.instructions.is_empty()),
    )
    .await
    else {
        unreachable!()
    };
    assert!(prompt.instructions.contains("Incorrect"));
    session
        .try_prompt_response(Some(PromptAnswer {
            responses: vec![pw("secret")],
            remember: true,
        }))
        .unwrap();
    wait_for(&mut state, is_ready).await;
    assert_eq!(*saved.lock().unwrap(), vec!["secret".to_owned()]);
}

#[tokio::test]
async fn cancelling_a_prompt_closes_the_session() {
    let srv = start_server(None).await;
    let (session, _out) = spawn(params(&srv, vec![Credential::PromptPassword]));
    let mut state = session.state();
    wait_for(&mut state, |s| matches!(s, SessionState::Prompting { .. })).await;
    session.try_prompt_response(None).unwrap();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert_eq!(
        end,
        SessionState::Closed {
            reason: CloseReason::UserClosed
        }
    );
}

#[tokio::test]
async fn jump_chain_reaches_target_through_bastion() {
    let bastion = start_server(None).await;
    let target = start_server(None).await;
    let p = chain(vec![
        hop(&bastion, vec![Credential::Password(pw("secret"))]),
        hop(&target, vec![Credential::Password(pw("secret"))]),
    ]);
    let (session, mut out) = spawn(p);
    let mut state = session.state();
    // Progress is reported per hop.
    wait_for(&mut state, |s| {
        matches!(s, SessionState::Connecting { hop: 2, total: 2 })
    })
    .await;
    wait_for(&mut state, is_ready).await;
    read_until(&mut out, b"ready").await;
    session.write(b"via-bastion".to_vec()).await.unwrap();
    read_until(&mut out, b"via-bastion").await;

    // The shell ran on the target; the bastion only tunneled.
    assert_eq!(target.rec.shells.load(Ordering::SeqCst), 1);
    assert_eq!(bastion.rec.shells.load(Ordering::SeqCst), 0);
    assert_eq!(
        *bastion.rec.tunnels.lock().unwrap(),
        vec![format!("127.0.0.1:{}", target.port)]
    );
}

#[tokio::test]
async fn each_hop_verifies_its_own_host_key() {
    let bastion = start_server(None).await;
    let target = start_server(None).await;
    let mut p = chain(vec![
        hop(&bastion, vec![Credential::Password(pw("secret"))]),
        hop(&target, vec![Credential::Password(pw("secret"))]),
    ]);
    p.hops[1].known_hosts.clear(); // only the target is unknown
    let (session, _out) = spawn(p);
    let mut state = session.state();
    let SessionState::VerifyingHostKey { prompt } = wait_for(&mut state, |s| {
        matches!(s, SessionState::VerifyingHostKey { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(
        prompt.port, target.port,
        "the prompt names the hop it is about"
    );
    assert_eq!(prompt.fingerprint, target.known.fingerprint);
    session
        .try_host_key_decision(HostKeyDecision::Once)
        .unwrap();
    wait_for(&mut state, is_ready).await;
}

#[tokio::test]
async fn unreachable_target_behind_bastion_names_both() {
    let bastion = start_server(None).await;
    let dead_port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut target_hop = hop(&bastion, vec![Credential::Password(pw("secret"))]);
    target_hop.port = dead_port;
    let first = hop(&bastion, vec![Credential::Password(pw("secret"))]);
    let (session, _out) = spawn(chain(vec![first, target_hop]));
    let mut state = session.state();
    match wait_for(&mut state, SessionState::is_terminal).await {
        SessionState::Failed { error } => {
            assert_eq!(error.code, ErrorCode::ConnectFailed);
            assert!(
                error.message.contains("couldn't reach"),
                "{}",
                error.message
            );
        }
        other => panic!("expected failure, got {other:?}"),
    }
}

#[tokio::test]
async fn dropped_connection_reconnects_and_keeps_the_session() {
    let srv = Arc::new(start_server(None).await);
    let builds = Arc::new(AtomicU32::new(0));
    let factory_srv = srv.clone();
    let factory_builds = builds.clone();
    let reconnect: Reconnect = Arc::new(move || {
        let srv = factory_srv.clone();
        let builds = factory_builds.clone();
        Box::pin(async move {
            builds.fetch_add(1, Ordering::SeqCst);
            Ok(params(&srv, vec![Credential::Password(pw("secret"))]))
        })
    });
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.reconnect = Some(reconnect);
    let (session, mut out) = spawn(p);
    let mut state = session.state();
    read_until(&mut out, b"ready").await;
    session.resize(100, 30).await.unwrap();
    session.write(b"sized".to_vec()).await.unwrap();
    read_until(&mut out, b"sized").await;

    session.write(b"drop\r".to_vec()).await.unwrap();
    let SessionState::Reconnecting {
        attempt,
        max_attempts,
        retry_in_secs,
    } = wait_for(&mut state, |s| {
        matches!(s, SessionState::Reconnecting { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!((attempt, max_attempts, retry_in_secs), (1, 10, 1));
    wait_for(&mut state, is_ready).await;
    // Same output stream: the new shell's banner arrives where the old one left off.
    read_until(&mut out, b"ready").await;
    assert_eq!(
        builds.load(Ordering::SeqCst),
        1,
        "credentials are rebuilt, not kept"
    );
    assert_eq!(srv.rec.shells.load(Ordering::SeqCst), 2);
    assert_eq!(
        srv.rec.sizes.lock().unwrap().last(),
        Some(&(100, 30)),
        "new PTY uses the current size"
    );

    session.write(b"still-here".to_vec()).await.unwrap();
    read_until(&mut out, b"still-here").await;
}

#[tokio::test]
async fn closing_during_reconnect_wait_stops_cleanly() {
    let srv = start_server(None).await;
    let reconnect: Reconnect = Arc::new(|| {
        Box::pin(async { Err(hopr_core::AppError::new(ErrorCode::VaultLocked, "locked")) })
    });
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.reconnect = Some(reconnect);
    let (session, mut out) = spawn(p);
    read_until(&mut out, b"ready").await;
    session.write(b"drop\r".to_vec()).await.unwrap();
    let mut state = session.state();
    wait_for(&mut state, |s| {
        matches!(s, SessionState::Reconnecting { .. })
    })
    .await;
    session.close().await.unwrap();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert_eq!(
        end,
        SessionState::Closed {
            reason: CloseReason::UserClosed
        }
    );
}

#[tokio::test]
async fn without_reconnect_a_drop_reports_connection_lost() {
    let srv = start_server(None).await;
    let (session, mut out) = open(&srv, "secret");
    read_until(&mut out, b"ready").await;
    session.write(b"drop\r".to_vec()).await.unwrap();
    let mut state = session.state();
    let end = wait_for(&mut state, SessionState::is_terminal).await;
    assert_eq!(
        end,
        SessionState::Closed {
            reason: CloseReason::ConnectionLost
        }
    );
}

/// 8 MiB through the real pipeline (actor → bounded queue → batcher → flow control) with
/// a deliberately slow, acking consumer: every byte arrives, in order, and the amount of
/// unacknowledged output never exceeds the flow-control window plus one batch.
#[tokio::test]
async fn flood_is_delivered_intact_under_backpressure() {
    let srv = start_server(None).await;
    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let flow = Arc::new(FlowControl::new(256 * 1024, 64 * 1024));
    let p = params(&srv, vec![Credential::Password(pw("secret"))]);
    let session = spawn_session("flood".into(), p, tx, flow.clone());

    let (ui_tx, mut ui_rx) = mpsc::channel::<Vec<u8>>(1024);
    tokio::spawn(run_batcher(
        rx,
        flow.clone(),
        BatchConfig::default(),
        move |b| ui_tx.try_send(b).is_ok(),
    ));

    let mut got = Vec::with_capacity(FLOOD_BYTES + 64);
    let mut peak_unacked = 0;
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

#[tokio::test]
async fn exec_runs_a_command_with_stdin_and_collects_output() {
    let srv = start_server(None).await;
    let p = params(&srv, vec![Credential::Password(pw("secret"))]);
    let out = hopr_ssh::exec(p, "sh -s", b"hello").await.unwrap();
    assert_eq!(out.stdout, "ran sh -s with 5 bytes\n");
    assert_eq!(out.stderr, "warn\n");
    assert_eq!(out.exit_status, Some(3));
    assert_eq!(
        srv.rec.shells.load(Ordering::SeqCst),
        0,
        "no PTY shell for exec"
    );
}

#[tokio::test]
async fn exec_never_asks_and_explains_an_unverified_host_key() {
    let srv = start_server(None).await;
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.hops[0].known_hosts.clear();
    let err = hopr_ssh::exec(p, "true", b"").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostKeyMismatch);
    assert!(err.message.contains("verified"), "{}", err.message);
}

#[tokio::test]
async fn exec_fails_instead_of_prompting_for_a_password() {
    let srv = start_server(None).await;
    let p = params(&srv, vec![Credential::PromptPassword]);
    let err = hopr_ssh::exec(p, "true", b"").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthFailed);
}

/// A loopback proxy speaking SOCKS5 or HTTP CONNECT (sniffed from the first byte). It maps
/// the name `ssh.behind-proxy` to 127.0.0.1, which only resolves through the proxy, and
/// records each requested target.
async fn start_proxy() -> (u16, Arc<Mutex<Vec<String>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            let log = log.clone();
            tokio::spawn(async move {
                let first = s.read_u8().await.unwrap();
                let (host, port) = if first == 5 {
                    let n = s.read_u8().await.unwrap();
                    let mut methods = vec![0u8; usize::from(n)];
                    s.read_exact(&mut methods).await.unwrap();
                    s.write_all(&[5, 0]).await.unwrap();
                    let mut head = [0u8; 4];
                    s.read_exact(&mut head).await.unwrap();
                    assert_eq!(head[3], 3, "names are resolved by the proxy");
                    let len = s.read_u8().await.unwrap();
                    let mut name = vec![0u8; usize::from(len)];
                    s.read_exact(&mut name).await.unwrap();
                    let port = s.read_u16().await.unwrap();
                    s.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
                    (String::from_utf8(name).unwrap(), port)
                } else {
                    let mut head = vec![first];
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(s.read_u8().await.unwrap());
                    }
                    let text = String::from_utf8(head).unwrap();
                    let authority = text.split_whitespace().nth(1).unwrap().to_owned();
                    let (h, p) = authority.rsplit_once(':').unwrap();
                    s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                    (h.to_owned(), p.parse().unwrap())
                };
                log.lock().unwrap().push(format!("{host}:{port}"));
                let real = if host == "ssh.behind-proxy" {
                    "127.0.0.1"
                } else {
                    host.as_str()
                };
                let mut upstream = tokio::net::TcpStream::connect((real, port)).await.unwrap();
                let _ = tokio::io::copy_bidirectional(&mut s, &mut upstream).await;
            });
        }
    });
    (port, seen)
}

#[tokio::test]
async fn first_hop_connects_through_socks5_and_http_proxies() {
    let srv = start_server(None).await;
    let (proxy_port, seen) = start_proxy().await;
    for kind in [hopr_ssh::ProxyKind::Socks5, hopr_ssh::ProxyKind::Http] {
        let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
        p.hops[0].host = "ssh.behind-proxy".into();
        p.hops[0].known_hosts[0].host_pattern = "ssh.behind-proxy".into();
        p.proxy = Some(hopr_ssh::Proxy {
            kind,
            host: "127.0.0.1".into(),
            port: proxy_port,
            username: None,
            password: None,
        });
        let (session, mut out) = spawn(p);
        read_until(&mut out, b"ready").await;
        session.close().await.unwrap();
    }
    let want = format!("ssh.behind-proxy:{}", srv.port);
    assert_eq!(*seen.lock().unwrap(), vec![want.clone(), want]);
}

#[tokio::test]
async fn unreachable_proxy_names_the_proxy() {
    let srv = start_server(None).await;
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    p.proxy = Some(hopr_ssh::Proxy {
        kind: hopr_ssh::ProxyKind::Socks5,
        host: "127.0.0.1".into(),
        port: closed,
        username: None,
        password: None,
    });
    let (session, _out) = spawn(p);
    let mut state = session.state();
    match wait_for(&mut state, SessionState::is_terminal).await {
        SessionState::Failed { error } => {
            assert_eq!(error.code, ErrorCode::ConnectFailed);
            assert!(
                error.message.contains("through proxy socks5://127.0.0.1"),
                "{}",
                error.message
            );
        }
        other => panic!("expected connect failure, got {other:?}"),
    }
}

/// The session runs over a ProxyCommand's stdin/stdout: a PowerShell TCP relay that
/// leaves a marker file when it starts (proof the command, not a direct dial, was used).
#[cfg(windows)]
#[tokio::test]
async fn proxy_command_carries_the_session() {
    let srv = start_server(None).await;
    let dir = std::env::temp_dir().join(format!("hopr-proxycmd-{}", srv.port));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("relay.ps1");
    let marker = dir.join("started");
    std::fs::write(
        &script,
        "Set-Content -Path $args[2] -Value 1\n\
         $c = New-Object Net.Sockets.TcpClient($args[0], [int]$args[1])\n\
         $s = $c.GetStream()\n\
         $i = [Console]::OpenStandardInput()\n\
         $o = [Console]::OpenStandardOutput()\n\
         $t = $i.CopyToAsync($s)\n\
         $s.CopyTo($o)\n",
    )
    .unwrap();
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.proxy_command = Some(format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" %h %p \"{}\"",
        script.display(),
        marker.display()
    ));
    let (session, mut out) = spawn(p);
    let mut state = session.state();
    let ready = tokio::time::timeout(Duration::from_secs(30), state.wait_for(is_ready))
        .await
        .expect("ready through the ProxyCommand")
        .is_ok();
    assert!(ready, "session ended: {:?}", *state.borrow());
    read_until(&mut out, b"ready").await;
    session.write(b"via-command".to_vec()).await.unwrap();
    read_until(&mut out, b"via-command").await;
    assert!(marker.exists(), "the relay command ran");
    session.close().await.unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn proxy_command_that_cannot_start_fails_clearly() {
    let srv = start_server(None).await;
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    // The command starts (the shell runs) but exits at once: the handshake fails.
    p.proxy_command = Some("exit 3".into());
    let (session, _out) = spawn(p);
    let mut state = session.state();
    let end = tokio::time::timeout(T, state.wait_for(|s| s.is_terminal()))
        .await
        .expect("ends")
        .map(|s| s.clone())
        .unwrap_or_else(|_| state.borrow().clone());
    assert!(
        matches!(
            end,
            SessionState::Failed { .. } | SessionState::Closed { .. }
        ),
        "{end:?}"
    );
}

fn user_cert(
    ca: &russh::keys::PrivateKey,
    key: &russh::keys::PrivateKey,
    principal: &str,
    valid_secs: i64,
) -> russh::keys::Certificate {
    use russh::keys::ssh_key::certificate::{Builder, CertType};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let before = now.saturating_add_signed(valid_secs);
    let mut b =
        Builder::new_with_random_nonce(&mut rand::rng(), key.public_key(), now - 60, before)
            .unwrap();
    b.serial(7).unwrap();
    b.key_id("e2e-cert").unwrap();
    b.cert_type(CertType::User).unwrap();
    b.valid_principal(principal).unwrap();
    b.sign(ca).unwrap()
}

/// A key the server doesn't know logs in with a certificate from the CA it trusts; the
/// same key without the certificate, or with an expired one, is refused.
#[tokio::test]
async fn certificate_login_trusts_the_ca_not_the_key() {
    let ca =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let key = Arc::new(
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap(),
    );
    let ca_fp = ca.public_key().fingerprint(HashAlg::Sha256).to_string();
    let srv = start_server(Some(ca_fp)).await;

    let good = user_cert(&ca, &key, "tester", 3600);
    let (session, mut out) = spawn(params(
        &srv,
        vec![Credential::Certificate {
            key: key.clone(),
            cert: Box::new(good),
        }],
    ));
    let mut state = session.state();
    wait_for(&mut state, is_ready).await;
    read_until(&mut out, b"ready").await;
    session.close().await.unwrap();

    for creds in [
        vec![Credential::Key(key.clone())],
        vec![Credential::Certificate {
            key: key.clone(),
            cert: Box::new(user_cert(&ca, &key, "tester", -30)),
        }],
        vec![Credential::Certificate {
            key: key.clone(),
            cert: Box::new(user_cert(&ca, &key, "someone-else", 3600)),
        }],
    ] {
        let (s, _o) = spawn(params(&srv, creds));
        let mut st = s.state();
        let end = wait_for(&mut st, |x| x.is_terminal()).await;
        match end {
            SessionState::Failed { error } => assert_eq!(error.code, ErrorCode::AuthFailed),
            other => panic!("expected auth failure, got {other:?}"),
        }
    }
}

/// A TCP echo server; returns its port.
async fn echo_server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            tokio::spawn(async move {
                let (mut r, mut w) = s.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    port
}

async fn roundtrip(port: u16, msg: &[u8]) -> Vec<u8> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut c = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    c.write_all(msg).await.unwrap();
    let mut back = vec![0u8; msg.len()];
    tokio::time::timeout(T, c.read_exact(&mut back))
        .await
        .expect("echo timeout")
        .unwrap();
    back
}

#[tokio::test]
async fn local_and_socks_forwards_carry_traffic_and_count_it() {
    use hopr_ssh::forward::{ForwardStats, serve_local, serve_socks};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let srv = start_server(None).await;
    let echo = echo_server().await;
    let conn = Arc::new(
        hopr_ssh::connect_quiet(&params(&srv, vec![Credential::Password(pw("secret"))]))
            .await
            .unwrap(),
    );

    // -L
    let local = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let lport = local.local_addr().unwrap().port();
    let stats = Arc::new(ForwardStats::default());
    let (stop, stop_rx) = watch::channel(false);
    let task = tokio::spawn(serve_local(
        conn.clone(),
        local,
        "127.0.0.1".into(),
        echo,
        stats.clone(),
        stop_rx,
    ));
    assert_eq!(roundtrip(lport, b"through -L").await, b"through -L");
    assert_eq!(roundtrip(lport, b"again").await, b"again");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(stats.total.load(Ordering::Relaxed), 2);
    assert_eq!(stats.up.load(Ordering::Relaxed), 15);
    assert_eq!(stats.down.load(Ordering::Relaxed), 15);
    stop.send(true).unwrap();
    task.await.unwrap();
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", lport))
            .await
            .is_err(),
        "the listener closes when the tunnel stops"
    );

    // -D (SOCKS5 CONNECT to the echo server by name)
    let socks = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sport = socks.local_addr().unwrap().port();
    let (_stop2, stop_rx2) = watch::channel(false);
    tokio::spawn(serve_socks(
        conn.clone(),
        socks,
        Arc::new(ForwardStats::default()),
        stop_rx2,
    ));
    let mut c = tokio::net::TcpStream::connect(("127.0.0.1", sport))
        .await
        .unwrap();
    c.write_all(&[5, 1, 0]).await.unwrap();
    let mut hello = [0u8; 2];
    c.read_exact(&mut hello).await.unwrap();
    assert_eq!(hello, [5, 0]);
    let mut req = vec![5, 1, 0, 3, 9];
    req.extend_from_slice(b"localhost");
    req.extend_from_slice(&echo.to_be_bytes());
    c.write_all(&req).await.unwrap();
    let mut reply = [0u8; 10];
    c.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0, "SOCKS CONNECT succeeded");
    c.write_all(b"via socks").await.unwrap();
    let mut back = [0u8; 9];
    c.read_exact(&mut back).await.unwrap();
    assert_eq!(&back, b"via socks");
    assert!(
        srv.rec
            .tunnels
            .lock()
            .unwrap()
            .contains(&format!("localhost:{echo}"))
    );
    conn.disconnect().await;
}

#[tokio::test]
async fn remote_forward_brings_server_side_connections_home() {
    use hopr_ssh::forward::{ForwardStats, RemoteForwards};
    let srv = start_server(None).await;
    let echo = echo_server().await;
    let routes = RemoteForwards::default();
    let mut p = params(&srv, vec![Credential::Password(pw("secret"))]);
    p.hooks.remote_forwards = Some(routes.clone());
    let conn = hopr_ssh::connect_quiet(&p).await.unwrap();
    let bound = conn.tcpip_forward("127.0.0.1", 0).await.unwrap();
    assert!(bound > 0, "the server picked a port");
    let stats = Arc::new(ForwardStats::default());
    routes.insert(bound, "127.0.0.1".into(), echo, stats.clone());
    let port = u16::try_from(bound).unwrap();
    assert_eq!(
        roundtrip(port, b"from the server side").await,
        b"from the server side"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(stats.total.load(Ordering::Relaxed), 1);
    assert!(stats.down.load(Ordering::Relaxed) >= 20);
    conn.disconnect().await;
}
