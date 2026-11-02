//! This computer's files, for the dual pane.

use hopr_core::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

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

pub(super) fn list_local(path: &str) -> Result<Vec<LocalEntry>, AppError> {
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
