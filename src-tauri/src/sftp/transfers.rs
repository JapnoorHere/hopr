//! The transfer queue: jobs, progress events, retries, pause/resume/cancel and settings.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hopr_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use super::*;
use crate::state::AppState;

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

pub(super) struct Job {
    info: Transfer,
    cancel: Arc<AtomicBool>,
    /// Already counted in a "finished" notification.
    notified: AtomicBool,
    /// Pause (true) rather than cancel (false) when `cancel` is set.
    pausing: Arc<AtomicBool>,
}

pub(super) fn emit(app: &AppHandle, t: &Transfer) {
    let _ = t.clone().emit(app);
}

pub(super) fn update(app: &AppHandle, id: &str, f: impl FnOnce(&mut Transfer)) -> Option<Transfer> {
    let files = app.state::<Files>();
    let mut jobs = files.jobs.lock().ok()?;
    let job = jobs.iter_mut().find(|j| j.info.id == id)?;
    f(&mut job.info);
    Some(job.info.clone())
}

pub(super) fn enqueue(app: &AppHandle, info: Transfer) {
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
pub(super) fn pump(app: AppHandle) {
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

pub(super) fn retryable(e: &AppError) -> bool {
    matches!(
        e.code,
        ErrorCode::Disconnected | ErrorCode::Timeout | ErrorCode::ConnectFailed
    )
}

/// A host with transfers queued or running keeps its SFTP session.
pub(super) fn host_busy(files: &Files, host_id: &str) -> bool {
    files.jobs.lock().is_ok_and(|jobs| {
        jobs.iter().any(|j| {
            j.info.host_id == host_id
                && matches!(
                    j.info.status,
                    TransferStatus::Queued | TransferStatus::Running
                )
        })
    })
}

/// A close asked for while transfers ran happens once the host's last one ends.
async fn release_if_idle(app: &AppHandle, host_id: &str) {
    let asked = {
        let files = app.state::<Files>();
        !host_busy(&files, host_id)
            && files
                .close_when_idle
                .lock()
                .is_ok_and(|mut idle| idle.remove(host_id))
    };
    if asked {
        close_session(app, host_id).await;
    }
}

pub(super) async fn run_job(app: &AppHandle, id: &str) {
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
    release_if_idle(app, &info.host_id).await;
}

pub(super) async fn transfer_once(
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

pub(super) fn new_transfer(
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

/// When the queue has finished and the app isn't in front, say so (S4).
pub(super) fn notify_if_idle(app: &AppHandle) {
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

pub(super) fn signal(app: &AppHandle, id: &str, pause: bool) -> Result<(), AppError> {
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
