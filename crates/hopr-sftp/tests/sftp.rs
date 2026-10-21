//! SFTP end to end through the real route code (`hopr-ssh`) against an in-process russh
//! server whose `sftp` subsystem serves a temp folder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use data_encoding::BASE64;
use hopr_core::model::{KnownHost, RemoteKind};
use hopr_sftp::server::{LocalFs, Modes};
use hopr_ssh::{ConnectParams, Credential, Hop, SessionHooks};
use russh::keys::{HashAlg, PublicKeyBase64};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use secrecy::SecretString;
use tokio::net::TcpListener;

#[derive(Clone)]
struct Srv {
    root: PathBuf,
    modes: Modes,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl server::Server for Srv {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

impl server::Handler for Srv {
    type Error = russh::Error;

    async fn auth_password(&mut self, _u: &str, p: &str) -> Result<Auth, Self::Error> {
        Ok(if p == "secret" {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _s: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().unwrap().insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel = self.channels.lock().unwrap().remove(&id);
        match (name, channel) {
            ("sftp", Some(channel)) => {
                session.channel_success(id)?;
                let fs = LocalFs::new(self.root.clone(), self.modes.clone());
                tokio::spawn(russh_sftp::server::run(channel.into_stream(), fs));
            }
            _ => session.channel_failure(id)?,
        }
        Ok(())
    }
}

type ParamsFn = Box<dyn Fn() -> ConnectParams>;

struct Fixture {
    root: PathBuf,
    params: ParamsFn,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "hopr-sftp-{}-{}",
        std::process::id(),
        rand::random::<u32>()
    ));
    std::fs::create_dir_all(root.join("home")).unwrap();
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let public = key.public_key().clone();
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut srv = Srv {
        root: root.clone(),
        modes: Modes::default(),
        channels: Arc::default(),
    };
    tokio::spawn(async move {
        let _ = srv.run_on_socket(config, &listener).await;
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
    Fixture {
        root,
        params: Box::new(move || ConnectParams {
            hops: vec![Hop {
                host: "127.0.0.1".into(),
                port,
                username: "u".into(),
                credentials: vec![Credential::Password(SecretString::from(
                    "secret".to_owned(),
                ))],
                known_hosts: vec![known.clone()],
                on_save_password: None,
            }],
            agent_forwarding: false,
            proxy: None,
            proxy_command: None,
            cols: 80,
            rows: 24,
            connect_timeout: Duration::from_secs(10),
            hooks: SessionHooks::default(),
            reconnect: None,
        }),
    }
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

#[tokio::test]
async fn browse_and_file_operations() {
    let fx = fixture().await;
    let sftp = hopr_sftp::connect((fx.params)()).await.unwrap();
    assert_eq!(sftp.home().await.unwrap(), "/");

    sftp.mkdir("/home/proj").await.unwrap();
    sftp.mkdir("/home/proj/src").await.unwrap();
    sftp.touch("/home/proj/src/main.rs").await.unwrap();
    assert!(
        sftp.touch("/home/proj/src/main.rs").await.is_err(),
        "touch never overwrites"
    );
    sftp.touch("/home/proj/README.md").await.unwrap();

    let list = sftp.list("/home/proj").await.unwrap();
    let names: Vec<_> = list.iter().map(|e| (e.name.as_str(), e.kind)).collect();
    assert_eq!(
        names,
        vec![("src", RemoteKind::Dir), ("README.md", RemoteKind::File)],
        "folders first"
    );
    assert_eq!(list[1].path, "/home/proj/README.md");

    sftp.rename("/home/proj/README.md", "/home/proj/README.txt")
        .await
        .unwrap();
    assert!(sftp.exists("/home/proj/README.txt").await);

    sftp.chmod("/home/proj", 0o750, true).await.unwrap();
    assert_eq!(
        sftp.stat("/home/proj/src/main.rs").await.unwrap().mode,
        0o750
    );

    sftp.remove("/home/proj").await.unwrap();
    assert!(
        !sftp.exists("/home/proj").await,
        "folders are removed with their contents"
    );
    let missing = sftp.list("/nope").await.unwrap_err();
    assert!(
        missing.message.contains("doesn't exist"),
        "{}",
        missing.message
    );
    assert!(
        sftp.list("/../..").await.is_err(),
        "paths can't climb out of the served folder"
    );
    sftp.close().await;
}

#[tokio::test]
async fn upload_download_roundtrip_and_preview() {
    let fx = fixture().await;
    let sftp = hopr_sftp::connect((fx.params)()).await.unwrap();
    let local = fx.root.join("up.bin");
    let body = data(3 * 1024 * 1024 + 123);
    std::fs::write(&local, &body).unwrap();
    let never = AtomicBool::new(false);
    let mut ticks = 0;
    let sent = sftp
        .upload(&local, "/home/up.bin", false, &never, |_, _| ticks += 1)
        .await
        .unwrap();
    assert_eq!(sent, body.len() as u64);
    assert!(ticks > 10, "progress is reported per chunk");
    assert_eq!(std::fs::read(fx.root.join("home/up.bin")).unwrap(), body);

    let back = fx.root.join("down/back.bin");
    sftp.download("/home/up.bin", &back, false, &never, |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&back).unwrap(), body);
    assert!(
        !hopr_sftp::part_path(&back).exists(),
        "the .part file is renamed when done"
    );

    assert_eq!(
        sftp.read_head("/home/up.bin", 10).await.unwrap(),
        body[..10]
    );
}

#[tokio::test]
async fn transfers_resume_from_where_they_stopped() {
    let fx = fixture().await;
    let sftp = hopr_sftp::connect((fx.params)()).await.unwrap();
    let body = data(2 * 1024 * 1024);
    std::fs::write(fx.root.join("home/big.bin"), &body).unwrap();

    // Download: a .part with the first MiB already there is continued, not restarted.
    let local = fx.root.join("dl/big.bin");
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::fs::write(hopr_sftp::part_path(&local), &body[..1024 * 1024]).unwrap();
    let mut first = None;
    sftp.download(
        "/home/big.bin",
        &local,
        true,
        &AtomicBool::new(false),
        |done, _| {
            first.get_or_insert(done);
        },
    )
    .await
    .unwrap();
    assert_eq!(first, Some(1024 * 1024));
    assert_eq!(std::fs::read(&local).unwrap(), body);

    // Upload: a shorter remote file is extended.
    std::fs::write(fx.root.join("home/up.bin"), &body[..700_000]).unwrap();
    let src = fx.root.join("src.bin");
    std::fs::write(&src, &body).unwrap();
    let mut first = None;
    sftp.upload(
        &src,
        "/home/up.bin",
        true,
        &AtomicBool::new(false),
        |done, _| {
            first.get_or_insert(done);
        },
    )
    .await
    .unwrap();
    assert_eq!(first, Some(700_000));
    assert_eq!(std::fs::read(fx.root.join("home/up.bin")).unwrap(), body);
}

#[tokio::test]
async fn cancel_stops_between_chunks_and_keeps_the_partial() {
    let fx = fixture().await;
    let sftp = hopr_sftp::connect((fx.params)()).await.unwrap();
    std::fs::write(fx.root.join("home/big.bin"), data(4 * 1024 * 1024)).unwrap();
    let cancel = AtomicBool::new(false);
    let local = fx.root.join("c/big.bin");
    let err = sftp
        .download("/home/big.bin", &local, false, &cancel, |done, _| {
            if done > 512 * 1024 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .await
        .unwrap_err();
    assert_eq!(err.message, "Cancelled");
    let part = std::fs::metadata(hopr_sftp::part_path(&local))
        .unwrap()
        .len();
    assert!(
        part > 0 && part < 4 * 1024 * 1024,
        "partial kept for resume: {part}"
    );
    assert!(!local.exists());
}

#[tokio::test]
async fn wrong_password_fails_without_prompting() {
    let fx = fixture().await;
    let mut p = (fx.params)();
    p.hops[0].credentials = vec![Credential::Password(SecretString::from("nope".to_owned()))];
    let err = hopr_sftp::connect(p).await.err().expect("must fail");
    assert_eq!(err.code, hopr_core::ErrorCode::AuthFailed);
}

#[tokio::test]
async fn throttle_caps_the_rate_across_transfers() {
    let t = hopr_sftp::Throttle::default();
    t.set(4 * 1024 * 1024);
    let start = std::time::Instant::now();
    // Two "transfers" of 1 MiB each, interleaved, against a 4 MiB/s budget: ≈ 0.5 s.
    let a = async {
        for _ in 0..16 {
            t.take(64 * 1024).await;
        }
    };
    let b = async {
        for _ in 0..16 {
            t.take(64 * 1024).await;
        }
    };
    tokio::join!(a, b);
    let secs = start.elapsed().as_secs_f64();
    assert!((0.4..1.5).contains(&secs), "took {secs}s");
    t.set(0);
    let quick = std::time::Instant::now();
    t.take(100 * 1024 * 1024).await;
    assert!(quick.elapsed().as_millis() < 50, "0 means unlimited");
}

#[tokio::test]
async fn throttled_upload_is_limited() {
    let fx = fixture().await;
    let throttle = Arc::new(hopr_sftp::Throttle::default());
    throttle.set(2 * 1024 * 1024);
    let sftp = hopr_sftp::connect_throttled((fx.params)(), throttle)
        .await
        .unwrap();
    let local = fx.root.join("slow.bin");
    std::fs::write(&local, data(2 * 1024 * 1024)).unwrap();
    let start = std::time::Instant::now();
    sftp.upload(
        &local,
        "/home/slow.bin",
        false,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .await
    .unwrap();
    let secs = start.elapsed().as_secs_f64();
    assert!(secs >= 0.8, "2 MiB at 2 MiB/s took only {secs}s");
}
