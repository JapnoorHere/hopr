//! Workspace IPC (R6): named layouts, the autosaved last session, the startup workspace.

use std::sync::Arc;

use hopr_core::AppError;
use hopr_core::model::Workspace;
use tauri::State;

use crate::state::AppState;

type S<'a> = State<'a, Arc<AppState>>;

fn check_layout(layout_json: &str) -> Result<(), AppError> {
    if layout_json.len() > 512 * 1024
        || serde_json::from_str::<serde_json::Value>(layout_json).is_err()
    {
        return Err(AppError::invalid("Invalid workspace layout"));
    }
    Ok(())
}

/// Every workspace (the autosave first).
#[tauri::command]
#[specta::specta]
pub async fn workspaces_list(state: S<'_>) -> Result<Vec<Workspace>, AppError> {
    state.require_unlocked()?;
    Ok(state.store.list_workspaces().await?)
}

/// Save the current layout under a name (same name replaces).
#[tauri::command]
#[specta::specta]
pub async fn workspace_save(
    state: S<'_>,
    name: String,
    layout_json: String,
) -> Result<Workspace, AppError> {
    state.require_unlocked()?;
    let name = name.trim();
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        return Err(AppError::invalid("Enter a name (up to 80 characters)"));
    }
    check_layout(&layout_json)?;
    Ok(state.store.save_workspace(name, &layout_json).await?)
}

/// Overwrite the autosaved last session.
#[tauri::command]
#[specta::specta]
pub async fn workspace_autosave(state: S<'_>, layout_json: String) -> Result<(), AppError> {
    // Autosave isn't user activity, so it must not postpone auto-lock.
    if !state.is_unlocked() {
        return Ok(());
    }
    check_layout(&layout_json)?;
    Ok(state.store.autosave_workspace(&layout_json).await?)
}

/// Delete a named workspace.
#[tauri::command]
#[specta::specta]
pub async fn workspace_delete(state: S<'_>, id: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.delete_workspace(&id).await?)
}

/// Open this workspace at startup (`null` = none).
#[tauri::command]
#[specta::specta]
pub async fn workspace_set_startup(state: S<'_>, id: Option<String>) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.set_startup_workspace(id.as_deref()).await?)
}
