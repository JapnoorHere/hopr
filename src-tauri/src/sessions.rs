//! Session IPC: open (saved host / quick connect / local shell), write, resize, ack,
//! host-key and prompt answers, close, plus state events.
//! Output streams over a per-session binary `Channel` (ADR 0006).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hopr_core::cast::CastWriter;
use hopr_core::model::{AuthKind, Host, Protocol, ProxyProtocol};
use hopr_core::session::{
    BatchConfig, FlowControl, OUTPUT_CAPACITY, PromptAnswer, SessionHandle, run_batcher,
};
use hopr_core::{AppError, CloseReason, ErrorCode, HostKeyDecision, SessionId, SessionState};
use hopr_ssh::{
    ConnectParams, Credential, Hop, Proxy, ProxyKind, Reconnect, ReconnectFuture, SavePassword,
    SessionHooks, TrustedKey,
};
use hopr_store::TrustKey;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::sync::mpsc;

use crate::state::AppState;

/// Registry of live session actors. The lock is never held across an `.await`.
#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<SessionId, SessionHandle>>,
    lives: Mutex<HashMap<SessionId, Arc<Live>>>,
}

/// Where a session's output goes. Swappable, so a tab can move to another window (T18)
/// without reconnecting: while it moves, output is held (bounded by flow control, since
/// held bytes are never acknowledged).
enum Sink {
    Window(Channel<RawBytes>),
    Held(Vec<u8>),
}

/// Who a session is for (history, recordings).
pub(crate) struct Who {
    pub host_id: Option<String>,
    pub label: String,
    /// "ssh" or "local".
    pub kind: &'static str,
}

/// An asciicast recording in progress.
pub(crate) struct Recorder {
    pub name: String,
    pub writer: CastWriter<BufWriter<File>>,
}

/// Per-session bookkeeping beside the actor: output sink, recording, counters.
pub(crate) struct Live {
    sink: Mutex<Sink>,
    pub rec: Mutex<Option<Recorder>>,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    /// Current grid size (a recording starts with it).
    pub size: Mutex<(u32, u32)>,
    pub who: Who,
    /// Connection history row, once the session was ready.
    conn_id: Mutex<Option<String>>,
}

impl Live {
    /// Stop recording, if it is. Returns the file name.
    pub fn stop_recording(&self) -> Option<String> {
        let mut rec = self.rec.lock().ok()?.take()?;
        let _ = rec.writer.flush();
        Some(rec.name)
    }
}

impl SessionManager {
    fn get(&self, id: &str) -> Result<SessionHandle, AppError> {
        self.lock()?
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::SessionNotFound, "This session has ended"))
    }

    fn insert(&self, handle: SessionHandle, live: Arc<Live>) -> Result<(), AppError> {
        if let Ok(mut lives) = self.lives.lock() {
            lives.insert(handle.id.clone(), live);
        }
        self.lock()?.insert(handle.id.clone(), handle);
        Ok(())
    }

    fn remove(&self, id: &str) {
        if let Ok(mut map) = self.lock() {
            map.remove(id);
        }
        if let Ok(mut lives) = self.lives.lock() {
            lives.remove(id);
        }
    }

    /// Send input to a session (snippets, broadcast), counted like typed input.
    pub(crate) fn write(&self, id: &str, data: Vec<u8>) -> Result<(), AppError> {
        let handle = self.get(id)?;
        if let Ok(live) = self.live(id) {
            live.bytes_out
                .fetch_add(data.len() as u64, Ordering::Relaxed);
        }
        handle.try_write(data)
    }

    pub(crate) fn live(&self, id: &str) -> Result<Arc<Live>, AppError> {
        self.lives
            .lock()
            .ok()
            .and_then(|s| s.get(id).cloned())
            .ok_or_else(|| AppError::new(ErrorCode::SessionNotFound, "This session has ended"))
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<SessionId, SessionHandle>>, AppError> {
        self.sessions
            .lock()
            .map_err(|_| AppError::new(ErrorCode::Internal, "Session registry poisoned"))
    }
}

/// Terminal bytes. Sent to JS as a raw `ArrayBuffer`, not a JSON number array.
#[derive(Type)]
pub struct RawBytes(Vec<u8>);

impl IpcResponse for RawBytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

/// What to connect to. Deliberately not `Debug`: quick connect may carry a password.
#[derive(Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectTarget {
    /// A saved host (credentials come from the vault).
    #[serde(rename_all = "camelCase")]
    Host {
        /// Host id.
        host_id: String,
    },
    /// An ad-hoc connection.
    Quick {
        /// Hostname or IP.
        host: String,
        /// Port.
        port: u16,
        /// Remote user.
        username: String,
        /// Password, if typed up front; otherwise the server will be asked how to log in.
        password: Option<String>,
    },
    /// A local shell.
    #[serde(rename_all = "camelCase")]
    Local {
        /// Shell profile id from `shells_list`.
        shell_id: String,
    },
}

/// Answer to an authentication prompt. Not `Debug`: carries secrets.
#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptReply {
    /// One value per prompt field.
    pub responses: Vec<String>,
    /// Save the typed password to the vault after a successful login.
    pub remember: bool,
}

/// Emitted on every session state transition.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SessionStateEvent {
    /// Session id.
    pub id: SessionId,
    /// New state.
    pub state: SessionState,
}

fn check_size(cols: u32, rows: u32) -> Result<(), AppError> {
    if !(1..=1000).contains(&cols) || !(1..=1000).contains(&rows) {
        return Err(AppError::invalid("Invalid terminal size"));
    }
    Ok(())
}

fn check_quick(host: &str, port: u16, username: &str) -> Result<(), AppError> {
    let host = host.trim();
    if host.is_empty()
        || host.len() > 253
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(AppError::invalid("Enter a valid hostname or IP address"));
    }
    if username.trim().is_empty() || username.len() > 64 || username.chars().any(char::is_control) {
        return Err(AppError::invalid("Enter a valid username"));
    }
    if port == 0 {
        return Err(AppError::invalid("Port must be between 1 and 65535"));
    }
    Ok(())
}

/// Methods to try for password-style auth: a known password first, then
/// keyboard-interactive (which fills password/OTP prompts it can and asks for the rest),
/// then an explicit password prompt for servers without keyboard-interactive.
fn password_credentials(
    password: Option<SecretString>,
    totp_seed: Option<SecretString>,
) -> Vec<Credential> {
    match password {
        Some(pw) => vec![
            Credential::Password(pw.clone()),
            Credential::KeyboardInteractive {
                password: Some(pw),
                totp_seed,
            },
        ],
        None => vec![
            Credential::KeyboardInteractive {
                password: None,
                totp_seed,
            },
            Credential::PromptPassword,
        ],
    }
}

/// Who a session is for, as the audit log names it: `(host id, label)`.
#[derive(Clone)]
struct AuditTarget {
    host_id: Option<String>,
    label: String,
}

impl AuditTarget {
    fn record(&self, state: &AppState, kind: &'static str, detail: impl Into<String>) {
        let host = Some((self.host_id.as_deref().unwrap_or(""), self.label.as_str()));
        state.audit(kind, host, detail);
    }
}

fn trust_hook(state: &Arc<AppState>, target: AuditTarget) -> Arc<dyn Fn(TrustedKey) + Send + Sync> {
    let app_state = state.clone();
    let store = state.store.clone();
    Arc::new(move |k: TrustedKey| {
        let store = store.clone();
        let (kind, verb) = if k.replace {
            ("host_key.replace", "Accepted a changed key for")
        } else {
            ("host_key.trust", "Trusted the key of")
        };
        target.record(
            &app_state,
            kind,
            format!(
                "{verb} {}:{} ({} {})",
                k.host, k.port, k.key_type, k.fingerprint
            ),
        );
        tauri::async_runtime::spawn(async move {
            let t = TrustKey {
                host_pattern: &k.host,
                port: k.port,
                key_type: &k.key_type,
                key_b64: &k.key_b64,
                fingerprint: &k.fingerprint,
                source: "tofu",
            };
            let res = if k.replace {
                store.replace_host_key(t).await
            } else {
                store.trust_key(t).await
            };
            if let Err(e) = res {
                eprintln!("hopr: failed to save trusted host key: {e}");
            }
        });
    })
}

/// What an SSH tab connects to, minus secrets: enough to rebuild parameters on reconnect.
#[derive(Clone)]
enum SshPlan {
    Host {
        host_id: String,
    },
    Quick {
        host: String,
        port: u16,
        username: String,
    },
}

impl SshPlan {
    async fn audit_target(&self, state: &AppState) -> AuditTarget {
        match self {
            Self::Host { host_id } => AuditTarget {
                host_id: Some(host_id.clone()),
                label: state
                    .store
                    .get_host(host_id)
                    .await
                    .map(|h| h.label)
                    .unwrap_or_else(|_| host_id.clone()),
            },
            Self::Quick {
                host,
                port,
                username,
            } => AuditTarget {
                host_id: None,
                label: format!("{username}@{host}:{port}"),
            },
        }
    }
}

/// One saved host as a hop: its credentials from the vault, trusted keys, and where a
/// typed password gets remembered. `password` (typed for this one attempt) is tried first.
async fn host_hop(
    state: &Arc<AppState>,
    vault: &Arc<hopr_vault::UnlockedVault>,
    h: &Host,
    password: Option<SecretString>,
) -> Result<Hop, AppError> {
    let mut creds = state.store.host_credentials(vault, &h.id).await?;
    if password.is_some() {
        creds.password = password;
    }
    let credentials = match h.auth {
        AuthKind::Password => password_credentials(creds.password, creds.totp_seed),
        AuthKind::Key | AuthKind::Agent => {
            let mut c = if h.auth == AuthKind::Key {
                let key_id = h.key_id.clone().ok_or_else(|| {
                    AppError::invalid(format!("{} has no key selected", h.label))
                })?;
                let private = state.store.key_private(vault, &key_id).await?;
                let key = hopr_ssh::keys::load(&private)?;
                let mut c = Vec::with_capacity(2);
                // A certificate for the key goes first; the bare key stays as a fallback.
                if let Some(text) = state.store.key_certificate(&key_id).await? {
                    let public = key.public_key().to_openssh().unwrap_or_default();
                    match hopr_ssh::keys::parse_certificate(&text, &public) {
                        Ok((cert, _)) => c.push(Credential::Certificate {
                            key: key.clone(),
                            cert: Box::new(cert),
                        }),
                        Err(e) => eprintln!(
                            "hopr: ignoring the certificate of {}: {}",
                            h.label, e.message
                        ),
                    }
                }
                c.push(Credential::Key(key));
                c
            } else {
                vec![Credential::Agent]
            };
            if creds.totp_seed.is_some() {
                // Key/agent + OTP setups answer the second factor via keyboard-interactive.
                c.push(Credential::KeyboardInteractive {
                    password: creds.password,
                    totp_seed: creds.totp_seed,
                });
            }
            c
        }
    };
    let store = state.store.clone();
    let vault = vault.clone();
    let host_id = h.id.clone();
    let on_save_password: SavePassword = Arc::new(move |pw: SecretString| {
        let (store, vault, id) = (store.clone(), vault.clone(), host_id.clone());
        tauri::async_runtime::spawn(async move {
            if let Err(e) = store
                .set_host_password(&vault, &id, pw.expose_secret())
                .await
            {
                eprintln!("hopr: failed to save password: {e}");
            }
        });
    });
    Ok(Hop {
        host: h.hostname.clone(),
        port: h.port,
        username: h.username.clone(),
        credentials,
        known_hosts: state
            .store
            .known_host_candidates(&h.hostname, h.port)
            .await?,
        on_save_password: Some(on_save_password),
    })
}

/// How to reach a saved host: its jump chain as hops, agent forwarding, and the proxy of
/// the outermost host.
pub(crate) struct HostRoute {
    pub hops: Vec<Hop>,
    pub agent_forwarding: bool,
    pub proxy: Option<Proxy>,
    /// Labels, outermost first (for audit detail).
    pub trail: Vec<String>,
    /// The outermost host's ProxyCommand.
    pub proxy_command: Option<String>,
}

/// Build the route to `host_id`. `target_password` replaces the target's saved password
/// for this attempt (deploy key asks for it when none is saved).
pub(crate) async fn host_route(
    state: &Arc<AppState>,
    host_id: &str,
    target_password: Option<SecretString>,
) -> Result<HostRoute, AppError> {
    let vault = state.vault()?;
    let chain = state.store.jump_chain(host_id).await?;
    if let Some(h) = chain.last()
        && h.protocol != Protocol::Ssh
    {
        return Err(AppError::invalid(format!(
            "{} is a {} host; files, keys and tunnels need SSH",
            h.label,
            h.protocol.as_str()
        )));
    }
    let last = chain.len().saturating_sub(1);
    let mut hops = Vec::with_capacity(chain.len());
    let mut target_password = target_password;
    for (i, h) in chain.iter().enumerate() {
        let pw = if i == last {
            target_password.take()
        } else {
            None
        };
        hops.push(host_hop(state, &vault, h, pw).await?);
    }
    let proxy = match chain.first() {
        Some(first) => match &first.proxy {
            Some(p) => {
                let creds = state.store.host_credentials(&vault, &first.id).await?;
                Some(Proxy {
                    kind: match p.protocol {
                        ProxyProtocol::Socks5 => ProxyKind::Socks5,
                        ProxyProtocol::Http => ProxyKind::Http,
                    },
                    host: p.host.clone(),
                    port: p.port,
                    username: p.username.clone(),
                    password: creds.proxy_password,
                })
            }
            None => None,
        },
        None => None,
    };
    Ok(HostRoute {
        hops,
        agent_forwarding: chain.last().is_some_and(|h| h.agent_forwarding),
        proxy,
        trail: chain.iter().map(|h| h.label.clone()).collect(),
        proxy_command: chain.first().and_then(|h| h.proxy_command.clone()),
    })
}

/// Parameters for a headless connection to a saved host (exec, tunnels, run on group):
/// its route, no PTY prompts, no reconnect.
pub(crate) async fn quiet_params(
    state: &Arc<AppState>,
    host_id: &str,
    hooks: SessionHooks,
) -> Result<ConnectParams, AppError> {
    let route = host_route(state, host_id, None).await?;
    Ok(ConnectParams {
        hops: route.hops,
        agent_forwarding: false,
        proxy: route.proxy,
        proxy_command: route.proxy_command,
        cols: 80,
        rows: 24,
        connect_timeout: Duration::from_secs(20),
        hooks,
        reconnect: None,
    })
}

/// Build connection parameters for a plan. `quick_password` is only used for the first
/// connect; reconnects re-read the vault (saved hosts) or let the server prompt (quick).
fn build_params(
    state: Arc<AppState>,
    plan: SshPlan,
    quick_password: Option<SecretString>,
    cols: u32,
    rows: u32,
) -> ReconnectFuture {
    // Boxed with an explicit `Send` type: the reconnect closure below refers back to this
    // function, and the compiler can't infer auto traits through that cycle.
    Box::pin(async move {
        let state = &state;
        let plan = &plan;
        let target = plan.audit_target(state).await;
        let mut hooks = SessionHooks {
            on_trust: Some(trust_hook(state, target.clone())),
            ..SessionHooks::default()
        };
        let (hops, agent_forwarding, proxy, proxy_command) = match plan {
            SshPlan::Host { host_id } => {
                let route = host_route(state, host_id, None).await?;
                let store = state.store.clone();
                let id = host_id.clone();
                let app_state = state.clone();
                let detail = match (&route.proxy, &route.proxy_command) {
                    (_, Some(_)) => format!("{} via ProxyCommand", route.trail.join(" › ")),
                    (Some(p), None) => format!("{} via proxy {p}", route.trail.join(" › ")),
                    (None, None) => route.trail.join(" › "),
                };
                hooks.on_ready = Some(Arc::new(move || {
                    target.record(&app_state, "session.connect", detail.clone());
                    let (store, id) = (store.clone(), id.clone());
                    tauri::async_runtime::spawn(async move {
                        let _ = store.mark_connected(&id).await;
                    });
                }));
                (
                    route.hops,
                    route.agent_forwarding,
                    route.proxy,
                    route.proxy_command,
                )
            }
            SshPlan::Quick {
                host,
                port,
                username,
            } => {
                state.require_unlocked()?;
                let hop = Hop {
                    host: host.clone(),
                    port: *port,
                    username: username.clone(),
                    credentials: password_credentials(quick_password, None),
                    known_hosts: state.store.known_host_candidates(host, *port).await?,
                    on_save_password: None,
                };
                let app_state = state.clone();
                hooks.on_ready = Some(Arc::new(move || {
                    target.record(&app_state, "session.connect", "Quick connect");
                }));
                (vec![hop], false, None, None)
            }
        };

        let reconnect: Reconnect = {
            let state = state.clone();
            let plan = plan.clone();
            Arc::new(move || {
                let (state, plan) = (state.clone(), plan.clone());
                build_params(state, plan, None, cols, rows)
            })
        };
        Ok(ConnectParams {
            hops,
            agent_forwarding,
            proxy,
            proxy_command,
            cols,
            rows,
            connect_timeout: Duration::from_secs(20),
            hooks,
            reconnect: Some(reconnect),
        })
    })
}

/// Open a session. Returns its id at once; progress arrives as `SessionStateEvent`s
/// and output on `output`.
#[tauri::command]
#[specta::specta]
pub async fn session_open(
    app: AppHandle,
    manager: State<'_, SessionManager>,
    state: State<'_, Arc<AppState>>,
    target: ConnectTarget,
    cols: u32,
    rows: u32,
    output: Channel<RawBytes>,
) -> Result<SessionId, AppError> {
    check_size(cols, rows)?;
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let flow = Arc::new(FlowControl::default());

    let mut audit_target = None;
    let mut local_name = None;
    // "telnet" / "serial" for those hosts (history shows the kind).
    let mut plain_kind: Option<&'static str> = None;
    let handle = match target {
        ConnectTarget::Local { shell_id } => {
            state.require_unlocked()?;
            let shells = tokio::task::spawn_blocking(hopr_term::detect_shells)
                .await
                .map_err(|e| AppError::internal("Shell detection failed", e))?;
            let shell = shells
                .iter()
                .find(|s| s.id == shell_id)
                .ok_or_else(|| AppError::invalid("That shell is no longer available"))?;
            local_name = Some(shell.name.clone());
            hopr_term::spawn_local(id.clone(), shell, cols, rows, tx, flow.clone())?
        }
        ConnectTarget::Host { host_id } => {
            state.require_unlocked()?;
            let host = state.store.get_host(&host_id).await?;
            match host.protocol {
                Protocol::Ssh => {
                    let plan = SshPlan::Host { host_id };
                    audit_target = Some(plan.audit_target(&state).await);
                    let params =
                        build_params(state.inner().clone(), plan, None, cols, rows).await?;
                    hopr_ssh::spawn_session(id.clone(), params, tx, flow.clone())
                }
                Protocol::Telnet | Protocol::Serial => {
                    let target = AuditTarget {
                        host_id: Some(host.id.clone()),
                        label: host.label.clone(),
                    };
                    let handle = if host.protocol == Protocol::Telnet {
                        target.record(&state, "session.connect", "Telnet (unencrypted)");
                        hopr_term::telnet::spawn_telnet(
                            id.clone(),
                            host.hostname.clone(),
                            host.port,
                            cols,
                            rows,
                            Duration::from_secs(20),
                            tx,
                            flow.clone(),
                        )
                    } else {
                        let cfg = host.serial.unwrap_or_default();
                        let h = hopr_term::serial::spawn_serial(
                            id.clone(),
                            &host.hostname,
                            &cfg,
                            tx,
                            flow.clone(),
                        )?;
                        target.record(
                            &state,
                            "session.connect",
                            format!("Serial {}", host.hostname),
                        );
                        h
                    };
                    let _ = state.store.mark_connected(&host.id).await;
                    plain_kind = Some(host.protocol.as_str());
                    audit_target = Some(target);
                    handle
                }
            }
        }
        ConnectTarget::Quick {
            host,
            port,
            username,
            password,
        } => {
            check_quick(&host, port, &username)?;
            let plan = SshPlan::Quick {
                host: host.trim().to_owned(),
                port,
                username: username.trim().to_owned(),
            };
            let password = password.filter(|p| !p.is_empty()).map(SecretString::from);
            audit_target = Some(plan.audit_target(&state).await);
            let params = build_params(state.inner().clone(), plan, password, cols, rows).await?;
            hopr_ssh::spawn_session(id.clone(), params, tx, flow.clone())
        }
    };
    let who = match (&audit_target, local_name) {
        (Some(t), _) => Who {
            host_id: t.host_id.clone(),
            label: t.label.clone(),
            kind: plain_kind.unwrap_or("ssh"),
        },
        (None, name) => Who {
            host_id: None,
            label: name.unwrap_or_else(|| "Local shell".to_owned()),
            kind: "local",
        },
    };
    let live = Arc::new(Live {
        sink: Mutex::new(Sink::Window(output)),
        rec: Mutex::new(None),
        bytes_in: AtomicU64::new(0),
        bytes_out: AtomicU64::new(0),
        size: Mutex::new((cols, rows)),
        who,
        conn_id: Mutex::new(None),
    });
    if app.state::<crate::history::HistoryState>().get().record_all
        && let Err(e) = crate::recording::start(&state, &live)
    {
        eprintln!("hopr: couldn't start recording: {}", e.message);
    }
    let batch_live = live.clone();
    tauri::async_runtime::spawn(run_batcher(
        rx,
        flow,
        BatchConfig::default(),
        move |bytes| {
            batch_live
                .bytes_in
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            if let Ok(mut rec) = batch_live.rec.lock()
                && let Some(r) = rec.as_mut()
                && r.writer.output(&bytes).is_err()
            {
                // Disk full or similar: stop instead of failing on every chunk.
                *rec = None;
            }
            match batch_live.sink.lock().as_deref_mut() {
                Ok(Sink::Window(ch)) => ch.send(RawBytes(bytes)).is_ok(),
                Ok(Sink::Held(buf)) => {
                    buf.extend_from_slice(&bytes);
                    true
                }
                Err(_) => false,
            }
        },
    ));
    manager.insert(handle.clone(), live.clone())?;
    forward_state(app, handle, state.inner().clone(), audit_target, live);
    Ok(id)
}

fn close_reason(state: &SessionState) -> &'static str {
    match state {
        SessionState::Closed { reason } => match reason {
            CloseReason::UserClosed => "user_closed",
            CloseReason::RemoteExited => "remote_exited",
            CloseReason::ConnectionLost => "connection_lost",
        },
        _ => "failed",
    }
}

fn forward_state(
    app: AppHandle,
    handle: SessionHandle,
    app_state: Arc<AppState>,
    audit: Option<AuditTarget>,
    live: Arc<Live>,
) {
    tauri::async_runtime::spawn(async move {
        let mut rx = handle.state();
        loop {
            let state = rx.borrow_and_update().clone();
            let terminal = state.is_terminal();
            let first_ready =
                state == SessionState::Ready && live.conn_id.lock().is_ok_and(|c| c.is_none());
            if first_ready {
                match app_state
                    .store
                    .connection_started(live.who.host_id.as_deref(), &live.who.label, live.who.kind)
                    .await
                {
                    Ok(cid) => {
                        if let Ok(mut c) = live.conn_id.lock() {
                            *c = Some(cid);
                        }
                    }
                    Err(e) => eprintln!("hopr: connection history: {e}"),
                }
            }
            if terminal {
                live.stop_recording();
                let cid = live.conn_id.lock().ok().and_then(|mut c| c.take());
                if let Some(cid) = cid {
                    let _ = app_state
                        .store
                        .connection_ended(
                            &cid,
                            close_reason(&state),
                            live.bytes_in.load(Ordering::Relaxed),
                            live.bytes_out.load(Ordering::Relaxed),
                        )
                        .await;
                }
            }
            if let (Some(target), SessionState::Failed { error }) = (&audit, &state) {
                match error.code {
                    ErrorCode::AuthFailed => {
                        target.record(&app_state, "session.auth_failed", error.message.clone());
                    }
                    ErrorCode::HostKeyMismatch => {
                        target.record(&app_state, "host_key.blocked", error.message.clone());
                    }
                    _ => {}
                }
            }
            let _ = SessionStateEvent {
                id: handle.id.clone(),
                state,
            }
            .emit(&app);
            if terminal {
                app.state::<SessionManager>().remove(&handle.id);
                break;
            }
            if rx.changed().await.is_err() {
                break;
            }
        }
    });
}

/// Send input. Synchronous on purpose: sync commands run in order, keystrokes must too.
#[tauri::command]
#[specta::specta]
pub fn session_write(
    manager: State<'_, SessionManager>,
    state: State<'_, Arc<AppState>>,
    id: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    state.touch();
    let handle = manager.get(&id)?;
    if let Ok(live) = manager.live(&id) {
        live.bytes_out
            .fetch_add(data.len() as u64, Ordering::Relaxed);
    }
    handle.try_write(data)
}

/// Propagate a terminal resize.
#[tauri::command]
#[specta::specta]
pub fn session_resize(
    manager: State<'_, SessionManager>,
    id: String,
    cols: u32,
    rows: u32,
) -> Result<(), AppError> {
    check_size(cols, rows)?;
    let handle = manager.get(&id)?;
    if let Ok(live) = manager.live(&id) {
        if let Ok(mut size) = live.size.lock() {
            *size = (cols, rows);
        }
        if let Ok(mut rec) = live.rec.lock()
            && let Some(r) = rec.as_mut()
        {
            let _ = r.writer.resize(cols, rows);
        }
    }
    handle.try_resize(cols, rows)
}

/// The UI rendered `bytes` of output (flow control).
#[tauri::command]
#[specta::specta]
pub fn session_ack(
    manager: State<'_, SessionManager>,
    id: String,
    bytes: u32,
) -> Result<(), AppError> {
    manager.get(&id)?.ack(u64::from(bytes));
    Ok(())
}

/// Answer a host key prompt.
#[tauri::command]
#[specta::specta]
pub fn session_host_key_decision(
    manager: State<'_, SessionManager>,
    id: String,
    decision: HostKeyDecision,
) -> Result<(), AppError> {
    manager.get(&id)?.try_host_key_decision(decision)
}

/// Answer (or cancel, with `null`) an authentication prompt.
#[tauri::command]
#[specta::specta]
pub fn session_prompt_response(
    manager: State<'_, SessionManager>,
    id: String,
    reply: Option<PromptReply>,
) -> Result<(), AppError> {
    let answer = reply.map(|r| PromptAnswer {
        responses: r.responses.into_iter().map(SecretString::from).collect(),
        remember: r.remember,
    });
    manager.get(&id)?.try_prompt_response(answer)
}

/// Hold sessions' output while their tab moves to another window.
#[tauri::command]
#[specta::specta]
pub fn session_detach(
    manager: State<'_, SessionManager>,
    ids: Vec<String>,
) -> Result<(), AppError> {
    for id in ids {
        let live = manager.live(&id)?;
        if let Ok(mut s) = live.sink.lock()
            && matches!(*s, Sink::Window(_))
        {
            *s = Sink::Held(Vec::new());
        }
    }
    Ok(())
}

/// Send a session's output to this window's channel, starting with anything held.
#[tauri::command]
#[specta::specta]
pub fn session_attach(
    manager: State<'_, SessionManager>,
    id: String,
    output: Channel<RawBytes>,
) -> Result<(), AppError> {
    let live = manager.live(&id)?;
    let mut s = live
        .sink
        .lock()
        .map_err(|_| AppError::new(ErrorCode::Internal, "Session output poisoned"))?;
    if let Sink::Held(buf) = &mut *s
        && !buf.is_empty()
    {
        let _ = output.send(RawBytes(std::mem::take(buf)));
    }
    *s = Sink::Window(output);
    Ok(())
}

/// Close a session. Closing one that already ended is not an error.
#[tauri::command]
#[specta::specta]
pub async fn session_close(manager: State<'_, SessionManager>, id: String) -> Result<(), AppError> {
    match manager.get(&id) {
        Ok(handle) => handle.close().await.or(Ok(())),
        Err(_) => Ok(()),
    }
}
