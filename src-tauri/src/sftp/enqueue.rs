//! Queueing uploads and downloads, with names that never overwrite by accident.

use std::path::{Path, PathBuf};

use hopr_core::model::RemoteKind;
use hopr_core::{AppError, ErrorCode};
use hopr_sftp::Sftp;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use super::transfers::*;
use super::*;

/// A local path not taken yet: `name`, then `name (1)`, `name (2)`…
pub(super) fn free_local(path: PathBuf) -> PathBuf {
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
pub(super) async fn free_remote(s: &Sftp, path: &str) -> String {
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
