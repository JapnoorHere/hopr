//! SFTP IPC (Phase 4a): one SFTP session per saved host, reused by the file drawer and the
//! transfer queue; file operations; preview; the transfer queue (concurrency 3, resume,
//! 3 retries, pause/cancel, progress at most 10 Hz); and "edit locally".

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hopr_core::AppError;
use hopr_sftp::Sftp;
use hopr_ssh::ConnectParams;
use tauri::{AppHandle, Manager};
use tokio::sync::Semaphore;

use crate::sessions;
use crate::state::AppState;

pub mod browse;
pub mod edit;
pub mod enqueue;
pub mod local;
pub mod transfers;

use edit::Edit;
use transfers::Job;
pub use transfers::{CONCURRENCY_KEY, LIMIT_KEY, Transfer, TransferSettings};

/// Transfers running at once (HOPR_PLAN §4).
const CONCURRENCY: usize = 3;
/// Attempts per transfer before it is marked failed.
const ATTEMPTS: u32 = 3;
/// Largest text shown in the preview.
const PREVIEW_MAX: usize = 256 * 1024;
/// Largest image shown in the preview.
const IMAGE_MAX: usize = 4 * 1024 * 1024;

/// Open SFTP sessions by host id, plus the transfer queue and local edits.
pub struct Files {
    sessions: tokio::sync::Mutex<HashMap<String, Arc<Sftp>>>,
    jobs: Mutex<Vec<Job>>,
    queue: Mutex<VecDeque<String>>,
    slots: Arc<Semaphore>,
    edits: Mutex<HashMap<String, Edit>>,
    concurrency: std::sync::atomic::AtomicUsize,
    throttle: Arc<hopr_sftp::Throttle>,
    /// Hosts nothing shows any more, whose session closes when their last transfer ends.
    close_when_idle: Mutex<HashSet<String>>,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            sessions: tokio::sync::Mutex::new(HashMap::new()),
            jobs: Mutex::new(Vec::new()),
            queue: Mutex::new(VecDeque::new()),
            slots: Arc::new(Semaphore::new(CONCURRENCY)),
            edits: Mutex::new(HashMap::new()),
            concurrency: std::sync::atomic::AtomicUsize::new(CONCURRENCY),
            throttle: Arc::new(hopr_sftp::Throttle::default()),
            close_when_idle: Mutex::new(HashSet::new()),
        }
    }
}

/// Drop a host's SFTP session and close its connection.
async fn close_session(app: &AppHandle, host_id: &str) {
    let s = app.state::<Files>().sessions.lock().await.remove(host_id);
    if let Some(s) = s {
        s.close().await;
    }
}

async fn params_for(state: &Arc<AppState>, host_id: &str) -> Result<ConnectParams, AppError> {
    let route = sessions::host_route(state, host_id, None).await?;
    Ok(ConnectParams {
        hops: route.hops,
        agent_forwarding: false,
        proxy: route.proxy,
        proxy_command: route.proxy_command,
        cols: 80,
        rows: 24,
        connect_timeout: Duration::from_secs(20),
        hooks: hopr_ssh::SessionHooks::default(),
        reconnect: None,
    })
}

/// The SFTP session for a host, opening (or reopening a dropped) one as needed.
async fn sftp(app: &AppHandle, host_id: &str) -> Result<Arc<Sftp>, AppError> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    state.require_unlocked()?;
    let files = app.state::<Files>();
    let mut map = files.sessions.lock().await;
    if let Some(s) = map.get(host_id)
        && s.is_open()
    {
        return Ok(s.clone());
    }
    let params = params_for(&state, host_id).await?;
    let s = Arc::new(hopr_sftp::connect_throttled(params, files.throttle.clone()).await?);
    map.insert(host_id.to_owned(), s.clone());
    Ok(s)
}

async fn label_of(app: &AppHandle, host_id: &str) -> String {
    let state = app.state::<Arc<AppState>>().inner().clone();
    state
        .store
        .get_host(host_id)
        .await
        .map(|h| h.label)
        .unwrap_or_else(|_| host_id.to_owned())
}

fn check_path(path: &str) -> Result<(), AppError> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return Err(AppError::invalid("Invalid path"));
    }
    Ok(())
}

fn check_name(name: &str) -> Result<(), AppError> {
    let n = name.trim();
    if n.is_empty() || n.contains('/') || n == "." || n == ".." || n.len() > 255 {
        return Err(AppError::invalid("Enter a name without slashes"));
    }
    Ok(())
}
