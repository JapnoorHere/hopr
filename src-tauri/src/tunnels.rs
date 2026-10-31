//! Tunnels (§9.11, P1–P8): saved local / remote / dynamic forwards, each on its own
//! headless SSH connection (through the host's jump chain, proxy or ProxyCommand), with a
//! 1 Hz stats event while any is running.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hopr_core::model::{Protocol, Tunnel, TunnelInput, TunnelKind};
use hopr_core::{AppError, ErrorCode};
use hopr_ssh::forward::{ForwardStats, RemoteForwards, serve_local, serve_socks};
use hopr_ssh::{ConnectParams, QuietConnection, SessionHooks};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::sessions;
use crate::state::AppState;

fn poisoned() -> AppError {
    AppError::new(ErrorCode::Internal, "Tunnel registry poisoned")
}

/// Where a tunnel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    /// Not running.
    Stopped,
    /// Connecting.
    Starting,
    /// Listening and forwarding.
    Running,
    /// Couldn't start, or the connection dropped.
    Failed,
}

/// Live status of one tunnel.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    /// Tunnel id.
    pub id: String,
    /// State.
    pub state: TunnelState,
    /// Why it failed.
    pub error: Option<String>,
    /// The port actually listened on (differs from the saved one when that was 0).
    pub bound_port: Option<u16>,
    /// Bytes towards the destination.
    #[specta(type = u32)]
    pub up: f64,
    /// Bytes back.
    #[specta(type = u32)]
    pub down: f64,
    /// Bytes per second towards the destination (last second).
    #[specta(type = u32)]
    pub up_rate: f64,
    /// Bytes per second back.
    #[specta(type = u32)]
    pub down_rate: f64,
    /// Connections open now.
    pub active: u32,
    /// Connections since start.
    pub total: u32,
}

impl TunnelStatus {
    fn idle(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            state: TunnelState::Stopped,
            error: None,
            bound_port: None,
            up: 0.0,
            down: 0.0,
            up_rate: 0.0,
            down_rate: 0.0,
            active: 0,
            total: 0,
        }
    }
}

/// Every live tunnel's status, once a second while any runs.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatsEvent {
    /// Statuses (running, starting, failed, and ones that just stopped).
    pub tunnels: Vec<TunnelStatus>,
}

struct Running {
    stop: watch::Sender<bool>,
    stats: Arc<ForwardStats>,
    conn: Option<Arc<QuietConnection>>,
    bound_port: Option<u16>,
    remote: Option<(String, u32)>,
    state: TunnelState,
    error: Option<String>,
    last: (u64, u64, Instant),
    rates: (f64, f64),
}

/// Live tunnels by id.
#[derive(Default)]
pub struct TunnelManager {
    running: Mutex<HashMap<String, Running>>,
    ticking: AtomicBool,
}

#[allow(clippy::cast_precision_loss)] // byte counters stay far below 2^53
fn status_of(id: &str, r: &Running) -> TunnelStatus {
    TunnelStatus {
        id: id.to_owned(),
        state: r.state,
        error: r.error.clone(),
        bound_port: r.bound_port,
        up: r.stats.up.load(Ordering::Relaxed) as f64,
        down: r.stats.down.load(Ordering::Relaxed) as f64,
        up_rate: r.rates.0,
        down_rate: r.rates.1,
        active: r.stats.active.load(Ordering::Relaxed),
        total: u32::try_from(r.stats.total.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
    }
}

impl TunnelManager {
    fn status(&self, id: &str) -> TunnelStatus {
        self.running
            .lock()
            .ok()
            .and_then(|m| m.get(id).map(|r| status_of(id, r)))
            .unwrap_or_else(|| TunnelStatus::idle(id))
    }

    fn fail(&self, id: &str, error: String) {
        if let Ok(mut m) = self.running.lock()
            && let Some(r) = m.get_mut(id)
        {
            r.state = TunnelState::Failed;
            r.error = Some(error);
            let _ = r.stop.send(true);
        }
    }

    /// Update rates; mark tunnels whose SSH link died as failed.
    #[allow(clippy::cast_precision_loss)]
    fn tick(&self) -> Vec<TunnelStatus> {
        let Ok(mut m) = self.running.lock() else {
            return Vec::new();
        };
        let now = Instant::now();
        let mut out = Vec::new();
        for (id, r) in m.iter_mut() {
            let (up, down) = (
                r.stats.up.load(Ordering::Relaxed),
                r.stats.down.load(Ordering::Relaxed),
            );
            let secs = now.duration_since(r.last.2).as_secs_f64().max(0.001);
            r.rates = (
                up.saturating_sub(r.last.0) as f64 / secs,
                down.saturating_sub(r.last.1) as f64 / secs,
            );
            r.last = (up, down, now);
            if r.state == TunnelState::Running && r.conn.as_ref().is_some_and(|c| !c.is_open()) {
                r.state = TunnelState::Failed;
                r.error = Some("The SSH connection dropped".into());
                let _ = r.stop.send(true);
            }
            out.push(status_of(id, r));
        }
        out
    }
}

/// Emit stats once a second while anything is running or starting.
fn ensure_ticker(app: &AppHandle) {
    let mgr = app.state::<TunnelManager>();
    if mgr.ticking.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mgr = app.state::<TunnelManager>();
            let tunnels = mgr.tick();
            let busy = tunnels
                .iter()
                .any(|t| matches!(t.state, TunnelState::Running | TunnelState::Starting));
            let _ = TunnelStatsEvent { tunnels }.emit(&app);
            if !busy {
                mgr.ticking.store(false, Ordering::SeqCst);
                break;
            }
        }
    });
}

async fn params_for(
    state: &Arc<AppState>,
    host_id: &str,
    remote_forwards: Option<RemoteForwards>,
) -> Result<ConnectParams, AppError> {
    let route = sessions::host_route(state, host_id, None).await?;
    Ok(ConnectParams {
        hops: route.hops,
        agent_forwarding: false,
        proxy: route.proxy,
        proxy_command: route.proxy_command,
        cols: 80,
        rows: 24,
        connect_timeout: Duration::from_secs(20),
        hooks: SessionHooks {
            remote_forwards,
            ..SessionHooks::default()
        },
        reconnect: None,
    })
}

fn bind_error(addr: &str, port: u16, e: &std::io::Error) -> AppError {
    let message = if e.kind() == std::io::ErrorKind::AddrInUse {
        format!("Port {port} on {addr} is already in use; pick another, or 0 for any free port")
    } else if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!("Not allowed to listen on {addr}:{port}")
    } else {
        format!("Couldn't listen on {addr}:{port}")
    };
    AppError::invalid(message).with_detail(e.to_string())
}

/// Connect and start forwarding. Runs with the tunnel already marked "starting".
async fn run(app: &AppHandle, t: &Tunnel) -> Result<(), AppError> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let mgr = app.state::<TunnelManager>();
    let (stats, stop_rx) = {
        let m = mgr.running.lock().map_err(|_| poisoned())?;
        let r = m
            .get(&t.id)
            .ok_or_else(|| AppError::invalid("The tunnel was stopped"))?;
        (r.stats.clone(), r.stop.subscribe())
    };
    // Listen locally first, so a busy port fails before connecting.
    let listener = match t.kind {
        TunnelKind::Local | TunnelKind::Dynamic => Some(
            TcpListener::bind((t.bind_addr.as_str(), t.bind_port))
                .await
                .map_err(|e| bind_error(&t.bind_addr, t.bind_port, &e))?,
        ),
        TunnelKind::Remote => None,
    };
    let routes = (t.kind == TunnelKind::Remote).then(RemoteForwards::default);
    let params = params_for(&state, &t.host_id, routes.clone()).await?;
    let conn = Arc::new(hopr_ssh::connect_quiet(&params).await?);
    let (bound, remote) = match listener {
        Some(l) => {
            let port = l.local_addr().map(|a| a.port()).unwrap_or(t.bind_port);
            if t.kind == TunnelKind::Dynamic {
                tauri::async_runtime::spawn(serve_socks(conn.clone(), l, stats, stop_rx));
            } else {
                tauri::async_runtime::spawn(serve_local(
                    conn.clone(),
                    l,
                    t.dest_host.clone(),
                    t.dest_port,
                    stats,
                    stop_rx,
                ));
            }
            (port, None)
        }
        None => {
            let port = conn.tcpip_forward(&t.bind_addr, t.bind_port).await?;
            if let Some(r) = &routes {
                r.insert(port, t.dest_host.clone(), t.dest_port, stats);
            }
            (
                u16::try_from(port).unwrap_or(t.bind_port),
                Some((t.bind_addr.clone(), port)),
            )
        }
    };
    let kept = if let Ok(mut m) = mgr.running.lock()
        && let Some(r) = m.get_mut(&t.id)
    {
        r.conn = Some(conn.clone());
        r.bound_port = Some(bound);
        r.remote = remote;
        r.state = TunnelState::Running;
        r.error = None;
        true
    } else {
        false // stopped while connecting
    };
    if !kept {
        conn.disconnect().await;
    }
    Ok(())
}

fn describe(t: &Tunnel, bound: Option<u16>) -> String {
    let port = bound.unwrap_or(t.bind_port);
    match t.kind {
        TunnelKind::Local => format!(
            "-L {}:{port} → {}:{}",
            t.bind_addr, t.dest_host, t.dest_port
        ),
        TunnelKind::Remote => format!(
            "-R {}:{port} → {}:{}",
            t.bind_addr, t.dest_host, t.dest_port
        ),
        TunnelKind::Dynamic => format!("-D {}:{port} (SOCKS5)", t.bind_addr),
    }
}

/// Start a tunnel (no-op if it runs already). Returns its status once it is listening
/// or has failed.
async fn start(app: &AppHandle, id: &str) -> Result<TunnelStatus, AppError> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    state.require_unlocked()?;
    let t = state.store.get_tunnel(id).await?;
    let mgr = app.state::<TunnelManager>();
    {
        let mut m = mgr.running.lock().map_err(|_| poisoned())?;
        if let Some(r) = m.get(id)
            && matches!(r.state, TunnelState::Running | TunnelState::Starting)
        {
            return Ok(status_of(id, r));
        }
        let (stop, _) = watch::channel(false);
        m.insert(
            id.to_owned(),
            Running {
                stop,
                stats: Arc::new(ForwardStats::default()),
                conn: None,
                bound_port: None,
                remote: None,
                state: TunnelState::Starting,
                error: None,
                last: (0, 0, Instant::now()),
                rates: (0.0, 0.0),
            },
        );
    }
    ensure_ticker(app);
    if let Err(e) = run(app, &t).await {
        mgr.fail(id, e.message.clone());
        return Err(e);
    }
    let host = state
        .store
        .get_host(&t.host_id)
        .await
        .map(|h| h.label)
        .unwrap_or_default();
    state.audit(
        "tunnel.start",
        Some((&t.host_id, &host)),
        describe(&t, mgr.status(id).bound_port),
    );
    Ok(mgr.status(id))
}

async fn stop(app: &AppHandle, id: &str) {
    let mgr = app.state::<TunnelManager>();
    let removed = mgr.running.lock().ok().and_then(|mut m| m.remove(id));
    if let Some(r) = removed {
        let _ = r.stop.send(true);
        if let Some(conn) = r.conn {
            if let Some((addr, port)) = &r.remote {
                conn.cancel_tcpip_forward(addr, *port).await;
            }
            conn.disconnect().await;
        }
    }
    let _ = TunnelStatsEvent {
        tunnels: vec![TunnelStatus::idle(id)],
    }
    .emit(app);
}

/// A tunnel and its live status.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    /// Saved settings.
    pub tunnel: Tunnel,
    /// Live status.
    pub status: TunnelStatus,
}

#[tauri::command]
#[specta::specta]
pub async fn tunnels_list(
    state: State<'_, Arc<AppState>>,
    mgr: State<'_, TunnelManager>,
) -> Result<Vec<TunnelView>, AppError> {
    state.require_unlocked()?;
    Ok(state
        .store
        .list_tunnels()
        .await?
        .into_iter()
        .map(|t| TunnelView {
            status: mgr.status(&t.id),
            tunnel: t,
        })
        .collect())
}

/// Create (no `id`) or update a tunnel. A running tunnel keeps its old settings until
/// it is restarted.
#[tauri::command]
#[specta::specta]
pub async fn tunnel_save(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    input: TunnelInput,
) -> Result<Tunnel, AppError> {
    state.require_unlocked()?;
    let input = input.normalized()?;
    let host = state.store.get_host(&input.host_id).await?;
    if host.protocol != Protocol::Ssh {
        return Err(AppError::invalid("Tunnels need an SSH host"));
    }
    Ok(state.store.save_tunnel(id.as_deref(), &input).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_delete(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    state.require_unlocked()?;
    stop(&app, &id).await;
    Ok(state.store.delete_tunnel(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_start(app: AppHandle, id: String) -> Result<TunnelStatus, AppError> {
    start(&app, &id).await
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_stop(app: AppHandle, id: String) -> Result<(), AppError> {
    stop(&app, &id).await;
    Ok(())
}

/// Start every tunnel marked "start on unlock" (the UI calls this after unlocking).
/// Returns how many are running.
#[tauri::command]
#[specta::specta]
pub async fn tunnels_autostart(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<u32, AppError> {
    state.require_unlocked()?;
    let mut n = 0;
    let auto: Vec<Tunnel> = state
        .store
        .list_tunnels()
        .await?
        .into_iter()
        .filter(|t| t.auto_start)
        .collect();
    for t in auto {
        match start(&app, &t.id).await {
            Ok(_) => n += 1,
            Err(e) => eprintln!("hopr: tunnel {} didn't start: {}", t.id, e.message),
        }
    }
    Ok(n)
}

/// Whether `addr:port` is free to listen on here (P8).
#[tauri::command]
#[specta::specta]
pub async fn port_free(addr: String, port: u16) -> bool {
    TcpListener::bind((addr.as_str(), port)).await.is_ok()
}

/// A free port on `addr`, chosen by the system (P8).
#[tauri::command]
#[specta::specta]
pub async fn port_pick(addr: String) -> Result<u16, AppError> {
    let l = TcpListener::bind((addr.as_str(), 0))
        .await
        .map_err(|e| bind_error(&addr, 0, &e))?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| AppError::internal("No free port", e))
}
