//! Edit locally: download to a temp file, upload again on every save.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hopr_core::model::RemoteKind;
use hopr_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;

use super::*;

pub(super) struct Edit {
    host_id: String,
    remote: String,
    local: PathBuf,
    /// Remote mtime when we last downloaded or uploaded it.
    remote_mtime: Option<u32>,
    pub(super) stop: Arc<AtomicBool>,
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

/// Where local copies of remote files live while they are edited.
fn temp_root() -> PathBuf {
    std::env::temp_dir().join("hopr-edit")
}

/// Delete every local copy: at startup (left from a crash or a previous run) and on exit.
pub fn clear_temp() {
    let _ = std::fs::remove_dir_all(temp_root());
}

pub(super) fn edit_event(
    app: &AppHandle,
    edit_id: &str,
    remote: &str,
    status: &str,
    message: Option<String>,
) {
    let _ = EditEvent {
        edit_id: edit_id.to_owned(),
        remote: remote.to_owned(),
        status: status.to_owned(),
        message,
    }
    .emit(app);
}

pub(super) fn local_mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub(super) async fn upload_edit(
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
    let local = temp_root().join(&edit_id[..8]).join(&entry.name);
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
