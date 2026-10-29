//! SFTP IPC (Phase 4a): one SFTP session per saved host, reused by the file drawer and the
//! transfer queue; file operations; preview; the transfer queue (concurrency 3, resume,
//! 3 retries, pause/cancel, progress at most 10 Hz); and "edit locally".

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use data_encoding::BASE64;
use hopr_core::model::{RemoteEntry, RemoteKind};
use hopr_core::{AppError, ErrorCode};
use hopr_sftp::Sftp;
use hopr_ssh::ConnectParams;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;
use tokio::sync::Semaphore;

use crate::sessions;
use crate::state::AppState;

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
        }
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

/// Start browsing a host: its home folder.
#[tauri::command]
#[specta::specta]
pub async fn sftp_open(app: AppHandle, host_id: String) -> Result<String, AppError> {
    sftp(&app, &host_id).await?.home().await
}

/// A folder's entries.
#[tauri::command]
#[specta::specta]
pub async fn sftp_list(
    app: AppHandle,
    host_id: String,
    path: String,
) -> Result<Vec<RemoteEntry>, AppError> {
    check_path(&path)?;
    sftp(&app, &host_id).await?.list(&path).await
}

/// Resolve a typed path (`~`, `..`) to an absolute one.
#[tauri::command]
#[specta::specta]
pub async fn sftp_resolve(
    app: AppHandle,
    host_id: String,
    path: String,
) -> Result<String, AppError> {
    check_path(&path)?;
    let s = sftp(&app, &host_id).await?;
    let p = if path == "~" || path.starts_with("~/") {
        format!("{}{}", s.home().await?, &path[1..])
    } else {
        path
    };
    s.canonicalize(&p).await
}

/// New folder in `dir`.
#[tauri::command]
#[specta::specta]
pub async fn sftp_mkdir(
    app: AppHandle,
    host_id: String,
    dir: String,
    name: String,
) -> Result<(), AppError> {
    check_name(&name)?;
    sftp(&app, &host_id)
        .await?
        .mkdir(&hopr_sftp::join(&dir, name.trim()))
        .await
}

/// New empty file in `dir`.
#[tauri::command]
#[specta::specta]
pub async fn sftp_touch(
    app: AppHandle,
    host_id: String,
    dir: String,
    name: String,
) -> Result<(), AppError> {
    check_name(&name)?;
    sftp(&app, &host_id)
        .await?
        .touch(&hopr_sftp::join(&dir, name.trim()))
        .await
}

/// Rename an entry within its folder.
#[tauri::command]
#[specta::specta]
pub async fn sftp_rename(
    app: AppHandle,
    host_id: String,
    path: String,
    name: String,
) -> Result<(), AppError> {
    check_path(&path)?;
    check_name(&name)?;
    let to = hopr_sftp::join(&hopr_sftp::parent(&path), name.trim());
    sftp(&app, &host_id).await?.rename(&path, &to).await
}

/// Delete entries (folders with their contents).
#[tauri::command]
#[specta::specta]
pub async fn sftp_remove(
    app: AppHandle,
    host_id: String,
    paths: Vec<String>,
) -> Result<(), AppError> {
    let s = sftp(&app, &host_id).await?;
    for p in &paths {
        check_path(p)?;
        if p == "/" {
            return Err(AppError::invalid("Refusing to delete /"));
        }
        s.remove(p).await?;
    }
    Ok(())
}

/// Change permissions.
#[tauri::command]
#[specta::specta]
pub async fn sftp_chmod(
    app: AppHandle,
    host_id: String,
    path: String,
    mode: u32,
    recursive: bool,
) -> Result<(), AppError> {
    check_path(&path)?;
    if mode > 0o7777 {
        return Err(AppError::invalid("Invalid permissions"));
    }
    sftp(&app, &host_id)
        .await?
        .chmod(&path, mode, recursive)
        .await
}

/// What the preview pane can show.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    /// UTF-8 text (possibly cut off).
    Text {
        /// Content.
        text: String,
        /// Only the start of the file is shown.
        truncated: bool,
    },
    /// An image, as a data URL.
    #[serde(rename_all = "camelCase")]
    Image {
        /// `data:image/...;base64,...`.
        data_url: String,
    },
    /// Not previewable.
    Binary,
}

fn image_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => return None,
    })
}

/// Preview a file: text, image, or nothing (S10).
#[tauri::command]
#[specta::specta]
pub async fn sftp_preview(
    app: AppHandle,
    host_id: String,
    path: String,
) -> Result<Preview, AppError> {
    check_path(&path)?;
    let s = sftp(&app, &host_id).await?;
    let entry = s.stat(&path).await?;
    if entry.kind != RemoteKind::File {
        return Ok(Preview::Binary);
    }
    if let Some(mime) = image_type(&entry.name) {
        #[allow(clippy::cast_precision_loss)]
        if entry.size > IMAGE_MAX as f64 {
            return Ok(Preview::Binary);
        }
        let bytes = s.read_head(&path, IMAGE_MAX).await?;
        return Ok(Preview::Image {
            data_url: format!("data:{mime};base64,{}", BASE64.encode(&bytes)),
        });
    }
    let bytes = s.read_head(&path, PREVIEW_MAX).await?;
    if bytes.contains(&0) {
        return Ok(Preview::Binary);
    }
    #[allow(clippy::cast_precision_loss)]
    let truncated = entry.size > bytes.len() as f64;
    Ok(Preview::Text {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    })
}

/// Drop the SFTP session for a host.
#[tauri::command]
#[specta::specta]
pub async fn sftp_close(app: AppHandle, host_id: String) -> Result<(), AppError> {
    let s = app.state::<Files>().sessions.lock().await.remove(&host_id);
    if let Some(s) = s {
        s.close().await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Transfer queue
// ---------------------------------------------------------------------------------------

/// Upload or download.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Local → remote.
    Upload,
    /// Remote → local.
    Download,
}

/// Where a transfer is.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    /// Waiting for a slot.
    Queued,
    /// Moving bytes.
    Running,
    /// Stopped by the user; resumes from where it stopped.
    Paused,
    /// Finished.
    Done,
    /// Gave up after retries.
    Failed,
    /// Stopped for good.
    Cancelled,
}

/// One transfer as the UI sees it (also the progress event).
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    /// Id.
    pub id: String,
    /// Direction.
    pub direction: Direction,
    /// File name.
    pub name: String,
    /// Local path.
    pub local: String,
    /// Remote path.
    pub remote: String,
    /// Host id.
    pub host_id: String,
    /// Host label.
    pub host_label: String,
    /// Total bytes.
    #[specta(type = u32)]
    pub size: f64,
    /// Bytes done.
    #[specta(type = u32)]
    pub done: f64,
    /// Bytes per second during this run.
    #[specta(type = u32)]
    pub speed: f64,
    /// Status.
    pub status: TransferStatus,
    /// Why it failed.
    pub error: Option<String>,
}

struct Job {
    info: Transfer,
    cancel: Arc<AtomicBool>,
    /// Already counted in a "finished" notification.
    notified: AtomicBool,
    /// Pause (true) rather than cancel (false) when `cancel` is set.
    pausing: Arc<AtomicBool>,
}

fn emit(app: &AppHandle, t: &Transfer) {
    let _ = t.clone().emit(app);
}

fn update(app: &AppHandle, id: &str, f: impl FnOnce(&mut Transfer)) -> Option<Transfer> {
    let files = app.state::<Files>();
    let mut jobs = files.jobs.lock().ok()?;
    let job = jobs.iter_mut().find(|j| j.info.id == id)?;
    f(&mut job.info);
    Some(job.info.clone())
}

fn enqueue(app: &AppHandle, info: Transfer) {
    let files = app.state::<Files>();
    let id = info.id.clone();
    emit(app, &info);
    if let Ok(mut jobs) = files.jobs.lock() {
        jobs.push(Job {
            info,
            cancel: Arc::new(AtomicBool::new(false)),
            notified: AtomicBool::new(false),
            pausing: Arc::new(AtomicBool::new(false)),
        });
    }
    if let Ok(mut q) = files.queue.lock() {
        q.push_back(id);
    }
    pump(app.clone());
}

/// Start queued jobs while slots are free.
fn pump(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let slots = app.state::<Files>().slots.clone();
            let Ok(permit) = slots.try_acquire_owned() else {
                return;
            };
            let next = app
                .state::<Files>()
                .queue
                .lock()
                .ok()
                .and_then(|mut q| q.pop_front());
            let Some(id) = next else {
                return;
            };
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                run_job(&app2, &id).await;
                notify_if_idle(&app2);
                drop(permit);
                pump(app2);
            });
        }
    });
}

fn retryable(e: &AppError) -> bool {
    matches!(
        e.code,
        ErrorCode::Disconnected | ErrorCode::Timeout | ErrorCode::ConnectFailed
    )
}

async fn run_job(app: &AppHandle, id: &str) {
    let (info, cancel, pausing) = {
        let files = app.state::<Files>();
        let Ok(jobs) = files.jobs.lock() else { return };
        let Some(j) = jobs.iter().find(|j| j.info.id == id) else {
            return;
        };
        if j.info.status != TransferStatus::Queued {
            return;
        }
        (j.info.clone(), j.cancel.clone(), j.pausing.clone())
    };
    cancel.store(false, Ordering::Relaxed);
    pausing.store(false, Ordering::Relaxed);
    if let Some(t) = update(app, id, |t| {
        t.status = TransferStatus::Running;
        t.error = None;
    }) {
        emit(app, &t);
    }

    let mut attempt = 0;
    // Resume whenever something is already there (a retry, or a paused transfer).
    let mut resume = info.done > 0.0;
    let result = loop {
        attempt += 1;
        let r = transfer_once(app, &info, resume, &cancel).await;
        match r {
            Err(e) if retryable(&e) && attempt < ATTEMPTS && !cancel.load(Ordering::Relaxed) => {
                // Drop the session so the next attempt reconnects.
                app.state::<Files>()
                    .sessions
                    .lock()
                    .await
                    .remove(&info.host_id);
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
                resume = true;
            }
            other => break other,
        }
    };
    let stopped = cancel.load(Ordering::Relaxed);
    let paused = pausing.load(Ordering::Relaxed);
    let t = update(app, id, |t| {
        t.speed = 0.0;
        match &result {
            Ok(()) => {
                t.status = TransferStatus::Done;
                t.done = t.size;
            }
            Err(_) if stopped => {
                t.status = if paused {
                    TransferStatus::Paused
                } else {
                    TransferStatus::Cancelled
                };
            }
            Err(e) => {
                t.status = TransferStatus::Failed;
                t.error = Some(e.message.clone());
            }
        }
    });
    if let Some(t) = t {
        if t.status == TransferStatus::Cancelled && t.direction == Direction::Download {
            let _ = std::fs::remove_file(hopr_sftp::part_path(Path::new(&t.local)));
        }
        emit(app, &t);
    }
}

async fn transfer_once(
    app: &AppHandle,
    info: &Transfer,
    resume: bool,
    cancel: &AtomicBool,
) -> Result<(), AppError> {
    let s = sftp(app, &info.host_id).await?;
    let started = Instant::now();
    let last_emit = Mutex::new(None::<Instant>);
    let base = AtomicU64::new(u64::MAX);
    let progress = |done: u64, size: u64| {
        let start =
            match base.compare_exchange(u64::MAX, done, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => done,
                Err(b) => b,
            };
        let now = Instant::now();
        let due = last_emit.lock().is_ok_and(|mut l| {
            let due = l.is_none_or(|t| now.duration_since(t) >= Duration::from_millis(100));
            if due {
                *l = Some(now);
            }
            due
        });
        if due {
            let secs = started.elapsed().as_secs_f64().max(0.001);
            #[allow(clippy::cast_precision_loss)]
            let t = update(app, &info.id, |t| {
                t.done = done as f64;
                t.size = size as f64;
                t.speed = done.saturating_sub(start) as f64 / secs;
            });
            if let Some(t) = t {
                emit(app, &t);
            }
        }
    };
    let local = PathBuf::from(&info.local);
    match info.direction {
        Direction::Upload => {
            s.upload(&local, &info.remote, resume, cancel, progress)
                .await
        }
        Direction::Download => {
            s.download(&info.remote, &local, resume, cancel, progress)
                .await
        }
    }
    .map(|_| ())
}

fn new_transfer(
    direction: Direction,
    host_id: &str,
    label: &str,
    local: &Path,
    remote: &str,
    size: u64,
) -> Transfer {
    #[allow(clippy::cast_precision_loss)]
    Transfer {
        id: uuid::Uuid::new_v4().to_string(),
        direction,
        name: hopr_sftp::file_name(remote).to_owned(),
        local: local.to_string_lossy().into_owned(),
        remote: remote.to_owned(),
        host_id: host_id.to_owned(),
        host_label: label.to_owned(),
        size: size as f64,
        done: 0.0,
        speed: 0.0,
        status: TransferStatus::Queued,
        error: None,
    }
}

/// A local path not taken yet: `name`, then `name (1)`, `name (2)`…
fn free_local(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..1000 {
        let p = path.with_file_name(format!("{stem} ({n}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    path
}

/// What to do when a transfer's target already exists.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Queue nothing and report the clashes, so the user can choose.
    Ask,
    /// Replace the existing file (folders merge).
    Overwrite,
    /// Leave existing files alone.
    Skip,
    /// Keep both: `name (1).ext`.
    Rename,
}

/// Result of queueing transfers.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct QueueResult {
    /// Transfers queued.
    pub queued: u32,
    /// Names that already exist at the target (only with `ConflictPolicy::Ask`; nothing
    /// was queued then).
    pub conflicts: Vec<String>,
}

/// A remote path not taken yet: `name`, then `name (1)`, `name (2)`…
async fn free_remote(s: &Sftp, path: &str) -> String {
    if !s.exists(path).await {
        return path.to_owned();
    }
    let dir = hopr_sftp::parent(path);
    let name = hopr_sftp::file_name(path);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((st, ex)) if !st.is_empty() => (st.to_owned(), format!(".{ex}")),
        _ => (name.to_owned(), String::new()),
    };
    for n in 1..1000 {
        let p = hopr_sftp::join(&dir, &format!("{stem} ({n}){ext}"));
        if !s.exists(&p).await {
            return p;
        }
    }
    path.to_owned()
}

/// Queue uploads of local files and folders into `remote_dir` (folders recursively).
#[tauri::command]
#[specta::specta]
pub async fn transfers_upload(
    app: AppHandle,
    host_id: String,
    local_paths: Vec<String>,
    remote_dir: String,
    policy: ConflictPolicy,
) -> Result<QueueResult, AppError> {
    check_path(&remote_dir)?;
    let s = sftp(&app, &host_id).await?;
    if policy == ConflictPolicy::Ask {
        let mut conflicts = Vec::new();
        for p in &local_paths {
            let name = Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if s.exists(&hopr_sftp::join(&remote_dir, &name)).await {
                conflicts.push(name);
            }
        }
        if !conflicts.is_empty() {
            return Ok(QueueResult {
                queued: 0,
                conflicts,
            });
        }
    }
    let label = label_of(&app, &host_id).await;
    let mut queued = 0;
    let mut stack: Vec<(PathBuf, String, bool)> = local_paths
        .iter()
        .map(|p| (PathBuf::from(p), remote_dir.clone(), true))
        .collect();
    while let Some((local, dir, top)) = stack.pop() {
        let name = local
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| AppError::invalid("Invalid file"))?;
        let mut remote = hopr_sftp::join(&dir, &name);
        let meta = std::fs::metadata(&local)
            .map_err(|_| AppError::invalid(format!("Can't read {}", local.display())))?;
        if top && s.exists(&remote).await {
            match policy {
                ConflictPolicy::Skip => continue,
                ConflictPolicy::Rename => remote = free_remote(&s, &remote).await,
                ConflictPolicy::Overwrite | ConflictPolicy::Ask => {}
            }
        }
        if meta.is_dir() {
            if !s.exists(&remote).await {
                s.mkdir(&remote).await?;
            }
            let entries = std::fs::read_dir(&local)
                .map_err(|_| AppError::invalid("Can't read that folder"))?;
            for e in entries.flatten() {
                stack.push((e.path(), remote.clone(), false));
            }
        } else {
            enqueue(
                &app,
                new_transfer(
                    Direction::Upload,
                    &host_id,
                    &label,
                    &local,
                    &remote,
                    meta.len(),
                ),
            );
            queued += 1;
        }
    }
    Ok(QueueResult {
        queued,
        conflicts: Vec::new(),
    })
}

/// Queue downloads of remote files and folders into `local_dir` (default: Downloads).
#[tauri::command]
#[specta::specta]
pub async fn transfers_download(
    app: AppHandle,
    host_id: String,
    remote_paths: Vec<String>,
    local_dir: Option<String>,
    policy: ConflictPolicy,
) -> Result<QueueResult, AppError> {
    let s = sftp(&app, &host_id).await?;
    let base = match local_dir {
        Some(d) => PathBuf::from(d),
        None => app
            .path()
            .download_dir()
            .map_err(|_| AppError::new(ErrorCode::Internal, "No Downloads folder"))?,
    };
    if policy == ConflictPolicy::Ask {
        let conflicts: Vec<String> = remote_paths
            .iter()
            .map(|p| hopr_sftp::file_name(p).to_owned())
            .filter(|n| base.join(n).exists())
            .collect();
        if !conflicts.is_empty() {
            return Ok(QueueResult {
                queued: 0,
                conflicts,
            });
        }
    }
    let label = label_of(&app, &host_id).await;
    let mut queued = 0;
    let mut stack: Vec<(String, PathBuf, bool)> = remote_paths
        .into_iter()
        .map(|p| (p, base.clone(), true))
        .collect();
    while let Some((remote, dir, top)) = stack.pop() {
        check_path(&remote)?;
        let entry = s.stat(&remote).await?;
        let mut target = dir.join(&entry.name);
        if top && target.exists() {
            match policy {
                ConflictPolicy::Skip => continue,
                ConflictPolicy::Rename => target = free_local(target),
                ConflictPolicy::Overwrite | ConflictPolicy::Ask => {}
            }
        }
        if entry.kind == RemoteKind::Dir {
            std::fs::create_dir_all(&target)
                .map_err(|_| AppError::invalid(format!("Can't create {}", target.display())))?;
            for child in s.list(&remote).await? {
                stack.push((child.path, target.clone(), false));
            }
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let size = entry.size as u64;
            enqueue(
                &app,
                new_transfer(
                    Direction::Download,
                    &host_id,
                    &label,
                    &target,
                    &remote,
                    size,
                ),
            );
            queued += 1;
        }
    }
    Ok(QueueResult {
        queued,
        conflicts: Vec::new(),
    })
}

/// Transfer settings (S5).
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferSettings {
    /// Transfers at once (1–8).
    pub concurrency: u32,
    /// Total speed limit in KiB/s (0 = none).
    pub limit_kbps: u32,
}

/// Settings key: transfers at once.
pub const CONCURRENCY_KEY: &str = "transfers.concurrency";
/// Settings key: speed limit in KiB/s.
pub const LIMIT_KEY: &str = "transfers.limit_kbps";

impl Files {
    /// Apply settings: resize the slot pool and set the shared speed limit.
    pub fn apply(&self, s: &TransferSettings) {
        let want = s.concurrency.clamp(1, 8) as usize;
        let have = self.concurrency.swap(want, Ordering::Relaxed);
        if want > have {
            self.slots.add_permits(want - have);
        } else if have > want {
            // Retire permits as running transfers hand them back.
            let slots = self.slots.clone();
            let n = u32::try_from(have - want).unwrap_or(0);
            tauri::async_runtime::spawn(async move {
                if let Ok(p) = slots.acquire_many_owned(n).await {
                    p.forget();
                }
            });
        }
        self.throttle.set(u64::from(s.limit_kbps) * 1024);
    }

    fn settings(&self) -> TransferSettings {
        TransferSettings {
            concurrency: u32::try_from(self.concurrency.load(Ordering::Relaxed)).unwrap_or(3),
            limit_kbps: u32::try_from(self.throttle.get() / 1024).unwrap_or(0),
        }
    }
}

/// Current transfer settings.
#[tauri::command]
#[specta::specta]
pub fn transfers_settings(files: State<'_, Files>) -> TransferSettings {
    files.settings()
}

/// Change transfer settings (applies to running transfers too).
#[tauri::command]
#[specta::specta]
pub async fn transfers_set_settings(
    app: AppHandle,
    settings: TransferSettings,
) -> Result<(), AppError> {
    if !(1..=8).contains(&settings.concurrency) || settings.limit_kbps > 10_000_000 {
        return Err(AppError::invalid(
            "Choose 1–8 transfers and a sensible limit",
        ));
    }
    let state = app.state::<Arc<AppState>>().inner().clone();
    state
        .store
        .set_setting(CONCURRENCY_KEY, &settings.concurrency)
        .await?;
    state
        .store
        .set_setting(LIMIT_KEY, &settings.limit_kbps)
        .await?;
    app.state::<Files>().apply(&settings);
    Ok(())
}

/// A local file or folder (dual pane).
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntry {
    /// Name.
    pub name: String,
    /// Full path.
    pub path: String,
    /// A folder.
    pub dir: bool,
    /// Size in bytes.
    #[specta(type = u32)]
    pub size: f64,
    /// Modified, Unix seconds.
    pub modified: Option<u32>,
}

fn list_local(path: &str) -> Result<Vec<LocalEntry>, AppError> {
    let rd =
        std::fs::read_dir(path).map_err(|_| AppError::invalid(format!("Can't open {path}")))?;
    let mut out: Vec<LocalEntry> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            #[allow(clippy::cast_precision_loss)]
            Some(LocalEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                path: e.path().to_string_lossy().into_owned(),
                dir: m.is_dir(),
                size: m.len() as f64,
                modified: m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| u32::try_from(d.as_secs()).ok()),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// A local folder's entries, folders first.
#[tauri::command]
#[specta::specta]
pub async fn local_list(path: String) -> Result<Vec<LocalEntry>, AppError> {
    tokio::task::spawn_blocking(move || list_local(&path))
        .await
        .map_err(|e| AppError::internal("Background task failed", e))?
}

/// Where the local pane starts: the home folder.
#[tauri::command]
#[specta::specta]
pub fn local_home(app: AppHandle) -> String {
    app.path()
        .home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "C:\\".into())
}

/// When the queue has finished and the app isn't in front, say so (S4).
fn notify_if_idle(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    let (active, done, failed) = {
        let files = app.state::<Files>();
        let Ok(jobs) = files.jobs.lock() else { return };
        let active = jobs.iter().any(|j| {
            matches!(
                j.info.status,
                TransferStatus::Queued | TransferStatus::Running
            )
        });
        let unseen = |s: TransferStatus| {
            jobs.iter()
                .filter(|j| j.info.status == s && !j.notified.load(Ordering::Relaxed))
                .count()
        };
        (
            active,
            unseen(TransferStatus::Done),
            unseen(TransferStatus::Failed),
        )
    };
    if active || done + failed == 0 {
        return;
    }
    if let Ok(jobs) = app.state::<Files>().jobs.lock() {
        for j in jobs.iter() {
            j.notified.store(true, Ordering::Relaxed);
        }
    }
    let focused = app
        .webview_windows()
        .values()
        .any(|w| w.is_focused().unwrap_or(false));
    if focused {
        return;
    }
    let body = match (done, failed) {
        (d, 0) => format!("{d} transfer{} finished", if d == 1 { "" } else { "s" }),
        (0, f) => format!("{f} transfer{} failed", if f == 1 { "" } else { "s" }),
        (d, f) => format!("{d} finished, {f} failed"),
    };
    let _ = app.notification().builder().title("Hopr").body(body).show();
}

/// Every transfer in this run (oldest first).
#[tauri::command]
#[specta::specta]
pub fn transfers_list(files: State<'_, Files>) -> Vec<Transfer> {
    files
        .jobs
        .lock()
        .map(|j| j.iter().map(|j| j.info.clone()).collect())
        .unwrap_or_default()
}

fn signal(app: &AppHandle, id: &str, pause: bool) -> Result<(), AppError> {
    let files = app.state::<Files>();
    let queued = {
        let jobs = files
            .jobs
            .lock()
            .map_err(|_| AppError::new(ErrorCode::Internal, "Queue poisoned"))?;
        let j = jobs
            .iter()
            .find(|j| j.info.id == id)
            .ok_or_else(|| AppError::invalid("No such transfer"))?;
        j.pausing.store(pause, Ordering::Relaxed);
        j.cancel.store(true, Ordering::Relaxed);
        j.info.status == TransferStatus::Queued
    };
    if queued {
        // Not started: take it out of the queue right away.
        if let Ok(mut q) = files.queue.lock() {
            q.retain(|x| x != id);
        }
        let t = update(app, id, |t| {
            t.status = if pause {
                TransferStatus::Paused
            } else {
                TransferStatus::Cancelled
            };
        });
        if let Some(t) = t {
            emit(app, &t);
        }
    }
    Ok(())
}

/// Stop a transfer for good.
#[tauri::command]
#[specta::specta]
pub fn transfer_cancel(app: AppHandle, id: String) -> Result<(), AppError> {
    signal(&app, &id, false)
}

/// Pause a transfer (it resumes from the same byte).
#[tauri::command]
#[specta::specta]
pub fn transfer_pause(app: AppHandle, id: String) -> Result<(), AppError> {
    signal(&app, &id, true)
}

/// Resume a paused transfer, or retry a failed or cancelled one (from where it stopped).
#[tauri::command]
#[specta::specta]
pub fn transfer_resume(app: AppHandle, id: String) -> Result<(), AppError> {
    let t = update(&app, &id, |t| {
        if matches!(
            t.status,
            TransferStatus::Paused | TransferStatus::Failed | TransferStatus::Cancelled
        ) {
            t.status = TransferStatus::Queued;
            t.error = None;
        }
    })
    .ok_or_else(|| AppError::invalid("No such transfer"))?;
    if t.status == TransferStatus::Queued {
        emit(&app, &t);
        if let Ok(mut q) = app.state::<Files>().queue.lock() {
            q.push_back(id);
        }
        pump(app);
    }
    Ok(())
}

/// Forget finished, failed and cancelled transfers.
#[tauri::command]
#[specta::specta]
pub fn transfers_clear(files: State<'_, Files>) {
    if let Ok(mut jobs) = files.jobs.lock() {
        jobs.retain(|j| {
            matches!(
                j.info.status,
                TransferStatus::Queued | TransferStatus::Running | TransferStatus::Paused
            )
        });
    }
}

// ---------------------------------------------------------------------------------------
// Edit locally (S6)
// ---------------------------------------------------------------------------------------

struct Edit {
    host_id: String,
    remote: String,
    local: PathBuf,
    /// Remote mtime when we last downloaded or uploaded it.
    remote_mtime: Option<u32>,
    stop: Arc<AtomicBool>,
}

/// What happened to a locally edited file.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct EditEvent {
    /// Edit id.
    pub edit_id: String,
    /// Remote path.
    pub remote: String,
    /// `opened`, `saved`, `conflict` (changed on the server too) or `error`.
    pub status: String,
    /// Detail for errors.
    pub message: Option<String>,
}

fn edit_event(app: &AppHandle, edit_id: &str, remote: &str, status: &str, message: Option<String>) {
    let _ = EditEvent {
        edit_id: edit_id.to_owned(),
        remote: remote.to_owned(),
        status: status.to_owned(),
        message,
    }
    .emit(app);
}

fn local_mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

async fn upload_edit(
    app: &AppHandle,
    edit_id: &str,
    force: bool,
) -> Result<&'static str, AppError> {
    let (host_id, remote, local, known) = {
        let files = app.state::<Files>();
        let edits = files
            .edits
            .lock()
            .map_err(|_| AppError::new(ErrorCode::Internal, "Edits poisoned"))?;
        let e = edits
            .get(edit_id)
            .ok_or_else(|| AppError::invalid("That edit has ended"))?;
        (
            e.host_id.clone(),
            e.remote.clone(),
            e.local.clone(),
            e.remote_mtime,
        )
    };
    let s = sftp(app, &host_id).await?;
    let now = s.stat(&remote).await.ok().and_then(|e| e.modified);
    if !force && now != known {
        return Ok("conflict");
    }
    s.upload(&local, &remote, false, &AtomicBool::new(false), |_, _| {})
        .await?;
    let mtime = s.stat(&remote).await.ok().and_then(|e| e.modified);
    if let Ok(mut edits) = app.state::<Files>().edits.lock()
        && let Some(e) = edits.get_mut(edit_id)
    {
        e.remote_mtime = mtime;
    }
    Ok("saved")
}

/// Download a file to a temp folder, open it in the default app, and upload it back each
/// time it is saved. If the server copy changed meanwhile, an `EditEvent` with status
/// `conflict` asks first. `open` is false only in tests.
#[tauri::command]
#[specta::specta]
pub async fn sftp_edit(
    app: AppHandle,
    host_id: String,
    path: String,
    open: bool,
) -> Result<String, AppError> {
    check_path(&path)?;
    let s = sftp(&app, &host_id).await?;
    let entry = s.stat(&path).await?;
    if entry.kind != RemoteKind::File {
        return Err(AppError::invalid("Only files can be edited"));
    }
    let edit_id = uuid::Uuid::new_v4().to_string();
    let local = std::env::temp_dir()
        .join("hopr-edit")
        .join(&edit_id[..8])
        .join(&entry.name);
    s.download(&path, &local, false, &AtomicBool::new(false), |_, _| {})
        .await?;
    let stop = Arc::new(AtomicBool::new(false));
    if let Ok(mut edits) = app.state::<Files>().edits.lock() {
        edits.insert(
            edit_id.clone(),
            Edit {
                host_id: host_id.clone(),
                remote: path.clone(),
                local: local.clone(),
                remote_mtime: entry.modified,
                stop: stop.clone(),
            },
        );
    }
    if open {
        app.opener()
            .open_path(local.to_string_lossy(), None::<&str>)
            .map_err(|e| AppError::internal("Couldn't open an editor", e))?;
    }
    edit_event(&app, &edit_id, &path, "opened", None);

    // Watch the local copy; upload on every save.
    let app2 = app.clone();
    let id = edit_id.clone();
    tauri::async_runtime::spawn(async move {
        let mut seen = local_mtime(&local);
        while !stop.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(700)).await;
            let now = local_mtime(&local);
            if now.is_none() || now == seen {
                continue;
            }
            seen = now;
            match upload_edit(&app2, &id, false).await {
                Ok(status) => edit_event(&app2, &id, &path, status, None),
                Err(e) => edit_event(&app2, &id, &path, "error", Some(e.message)),
            }
        }
    });
    Ok(edit_id)
}

/// After a conflict: upload anyway (`overwrite`), or keep the server's version.
#[tauri::command]
#[specta::specta]
pub async fn sftp_edit_resolve(
    app: AppHandle,
    edit_id: String,
    overwrite: bool,
) -> Result<(), AppError> {
    if overwrite {
        let status = upload_edit(&app, &edit_id, true).await?;
        let path = app
            .state::<Files>()
            .edits
            .lock()
            .ok()
            .and_then(|e| e.get(&edit_id).map(|e| e.remote.clone()))
            .unwrap_or_default();
        edit_event(&app, &edit_id, &path, status, None);
    }
    Ok(())
}

/// Stop watching a locally edited file and delete the temp copy.
#[tauri::command]
#[specta::specta]
pub fn sftp_edit_stop(app: AppHandle, edit_id: String) {
    let edit = app
        .state::<Files>()
        .edits
        .lock()
        .ok()
        .and_then(|mut e| e.remove(&edit_id));
    if let Some(e) = edit {
        e.stop.store(true, Ordering::Relaxed);
        if let Some(dir) = e.local.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Local path of an edit (tests write to it to simulate saving in an editor).
#[tauri::command]
#[specta::specta]
pub fn sftp_edit_path(app: AppHandle, edit_id: String) -> Option<String> {
    app.state::<Files>().edits.lock().ok().and_then(|e| {
        e.get(&edit_id)
            .map(|e| e.local.to_string_lossy().into_owned())
    })
}
