//! Backup IPC: daily automatic encrypted backups (keep 7), back up now, and restore from a
//! saved backup or a `.hopr` file. A restore is staged and applied at the next start.

use std::path::Path;
use std::sync::Arc;

use hopr_core::{AppError, ErrorCode};
use hopr_store::backup::{self, BackupHeader};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::state::AppState;

type S<'a> = State<'a, Arc<AppState>>;

/// Automatic backups kept.
pub const KEEP_AUTO: usize = 7;
/// Minimum age of the newest automatic backup before another is made.
const AUTO_EVERY_SECS: i64 = 24 * 60 * 60;

/// A backup file.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    /// File name.
    pub name: String,
    /// When it was made (RFC 3339).
    pub created_at: String,
    /// Size in bytes.
    pub size: u32,
    /// Made automatically (pruned to the newest 7).
    pub automatic: bool,
}

fn stamp(t: OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

fn info(path: &Path) -> Option<BackupInfo> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let bytes = std::fs::read(path).ok()?;
    let (header, _) = backup::read_header(&bytes).ok()?;
    Some(BackupInfo {
        automatic: name.starts_with("auto-"),
        name,
        created_at: header.created_at,
        size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
    })
}

/// Write a backup now. `automatic` backups are named `auto-…` and pruned.
pub async fn write_backup(state: &AppState, automatic: bool) -> Result<BackupInfo, AppError> {
    let vault = state.vault()?;
    let header = BackupHeader {
        version: 1,
        created_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        vault: state
            .store
            .vault_header()
            .await?
            .ok_or_else(|| AppError::invalid("Create a vault first"))?,
    };
    let db = state.store.snapshot_bytes().await?;
    let sealed = backup::seal(&vault, &header, &db)?;
    let dir = state.backups_dir();
    std::fs::create_dir_all(&dir).map_err(hopr_store::StoreError::from)?;
    let prefix = if automatic { "auto" } else { "manual" };
    let path = dir.join(format!(
        "{prefix}-{}.hopr",
        stamp(OffsetDateTime::now_utc())
    ));
    std::fs::write(&path, &sealed).map_err(hopr_store::StoreError::from)?;
    if automatic {
        backup::prune(&dir, KEEP_AUTO);
    }
    info(&path)
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "The backup could not be read back"))
}

/// After an unlock: back up if the newest automatic backup is a day old (or missing).
pub fn spawn_auto_backup(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let newest = backup::list(&state.backups_dir())
            .into_iter()
            .filter_map(|p| info(&p))
            .find(|b| b.automatic);
        let due = match newest {
            None => true,
            Some(b) => OffsetDateTime::parse(&b.created_at, &Rfc3339)
                .map(|t| (OffsetDateTime::now_utc() - t).whole_seconds() >= AUTO_EVERY_SECS)
                .unwrap_or(true),
        };
        if due && let Err(e) = write_backup(&state, true).await {
            eprintln!("hopr: automatic backup failed: {}", e.message);
        }
    });
}

/// Backups on this device, newest first.
#[tauri::command]
#[specta::specta]
pub async fn backups_list(state: S<'_>) -> Result<Vec<BackupInfo>, AppError> {
    state.require_unlocked()?;
    Ok(backup::list(&state.backups_dir())
        .iter()
        .filter_map(|p| info(p))
        .collect())
}

/// Folder that holds the backups (for "Show in folder").
#[tauri::command]
#[specta::specta]
pub fn backups_dir(state: S<'_>) -> String {
    state.backups_dir().to_string_lossy().into_owned()
}

/// Back up now.
#[tauri::command]
#[specta::specta]
pub async fn backup_now(state: S<'_>) -> Result<BackupInfo, AppError> {
    let b = write_backup(&state, false).await?;
    state.audit("backup.create", None, format!("Backup {}", b.name));
    Ok(b)
}

/// Stage a restore from a saved backup (`name`) or a file's bytes, after checking the
/// master password it was made with. Takes effect when Hopr restarts.
#[tauri::command]
#[specta::specta]
pub async fn backup_restore(
    state: S<'_>,
    name: Option<String>,
    bytes: Option<Vec<u8>>,
    password: String,
) -> Result<(), AppError> {
    let (file, source) = match (name, bytes) {
        (Some(name), None) => {
            if name.contains(['/', '\\']) || !name.ends_with(".hopr") {
                return Err(AppError::invalid("Unknown backup"));
            }
            let data = std::fs::read(state.backups_dir().join(&name))
                .map_err(|_| AppError::invalid("That backup no longer exists"))?;
            (data, name)
        }
        (None, Some(bytes)) => {
            if bytes.len() > 512 * 1024 * 1024 {
                return Err(AppError::invalid("That file is too large"));
            }
            (bytes, "a .hopr file".to_owned())
        }
        _ => return Err(AppError::invalid("Choose one backup to restore")),
    };
    let pw = SecretString::from(password);
    let (header, db) = tokio::task::spawn_blocking(move || backup::open(&file, &pw))
        .await
        .map_err(|e| AppError::internal("Background task failed", e))??;
    backup::stage_restore(&state.db_path(), &db)?;
    state.audit(
        "backup.restore",
        None,
        format!("Restore of {source} ({}) staged", header.created_at),
    );
    Ok(())
}

/// A restore is waiting for a restart.
#[tauri::command]
#[specta::specta]
pub fn backup_restore_pending(state: S<'_>) -> bool {
    backup::pending_path(&state.db_path()).exists()
}
