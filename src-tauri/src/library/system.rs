//! What this computer offers: local shells and serial ports.

use hopr_core::AppError;
use hopr_term::ShellProfile;

use super::*;

/// Local shells for "New local terminal".
#[tauri::command]
#[specta::specta]
pub async fn shells_list() -> Result<Vec<ShellProfile>, AppError> {
    tokio::task::spawn_blocking(hopr_term::detect_shells)
        .await
        .map_err(task_err)
}

/// Serial ports on this machine, for a serial host's port field.
#[tauri::command]
#[specta::specta]
pub async fn serial_ports() -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(hopr_term::serial::serial_ports)
        .await
        .map_err(task_err)
}
