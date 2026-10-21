//! Signed auto-updates (U6). Every payload is checked against the public key in
//! tauri.conf.json before it is installed; nothing is ever forced.
//!
//! `HOPR_UPDATE_ENDPOINT` replaces the release feed (local testing). It can't weaken
//! anything: the signature check still applies.

use std::sync::Mutex;

use hopr_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// An update that has been found and possibly downloaded.
#[derive(Default)]
pub struct Pending {
    update: Mutex<Option<Update>>,
    bytes: Mutex<Option<Vec<u8>>>,
}

/// What the UI shows about an available update.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// New version.
    pub version: String,
    /// Running version.
    pub current: String,
    /// Release notes.
    pub notes: Option<String>,
}

fn err(msg: &str, e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::ConnectFailed, msg).with_detail(e.to_string())
}

/// Look for a newer version. `None` = up to date.
#[tauri::command]
#[specta::specta]
pub async fn update_check(
    app: AppHandle,
    pending: State<'_, Pending>,
) -> Result<Option<UpdateInfo>, AppError> {
    let mut builder = app.updater_builder();
    if let Ok(endpoint) = std::env::var("HOPR_UPDATE_ENDPOINT") {
        let url = endpoint
            .parse()
            .map_err(|e| err("Invalid update endpoint", e))?;
        builder = builder
            .endpoints(vec![url])
            .map_err(|e| err("Invalid update endpoint", e))?;
    }
    let updater = builder
        .build()
        .map_err(|e| err("Updates aren't configured", e))?;
    let found = updater
        .check()
        .await
        .map_err(|e| err("Couldn't check for updates", e))?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
    });
    if let Ok(mut p) = pending.update.lock() {
        *p = found;
    }
    if let Ok(mut b) = pending.bytes.lock() {
        *b = None;
    }
    Ok(info)
}

/// Download the update found by `update_check` and verify its signature.
#[tauri::command]
#[specta::specta]
pub async fn update_download(pending: State<'_, Pending>) -> Result<(), AppError> {
    let update = pending
        .update
        .lock()
        .ok()
        .and_then(|p| p.clone())
        .ok_or_else(|| AppError::new(ErrorCode::InvalidInput, "Check for updates first"))?;
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| err("The update failed verification or download", e))?;
    if let Ok(mut b) = pending.bytes.lock() {
        *b = Some(bytes);
    }
    Ok(())
}

/// Install the downloaded update and restart.
#[tauri::command]
#[specta::specta]
pub async fn update_install(app: AppHandle, pending: State<'_, Pending>) -> Result<(), AppError> {
    let update = pending.update.lock().ok().and_then(|p| p.clone());
    let bytes = pending.bytes.lock().ok().and_then(|mut b| b.take());
    let (Some(update), Some(bytes)) = (update, bytes) else {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Download the update first",
        ));
    };
    update.install(bytes).map_err(|e| {
        AppError::new(ErrorCode::Internal, "Installing the update failed")
            .with_detail(e.to_string())
    })?;
    app.restart();
}
