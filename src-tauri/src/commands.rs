//! IPC commands. Keep each one thin: validate, delegate, map errors.

use hopr_core::AppInfo;

/// Version and platform of the running build.
#[tauri::command]
#[specta::specta]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    AppInfo::current(app.package_info().version.to_string())
}
