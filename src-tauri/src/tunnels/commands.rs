//! The tunnel commands: list, save, delete, start, stop, autostart and port checks.

use std::sync::Arc;

use hopr_core::AppError;
use hopr_core::model::{Protocol, Tunnel, TunnelInput};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tokio::net::TcpListener;

use crate::state::AppState;

use super::runtime::{bind_error, start, stop};
use super::*;

/// A tunnel and its live status.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    /// Saved settings.
    pub tunnel: Tunnel,
    /// Live status.
    pub status: TunnelStatus,
}

#[tauri::command]
#[specta::specta]
pub async fn tunnels_list(
    state: State<'_, Arc<AppState>>,
    mgr: State<'_, TunnelManager>,
) -> Result<Vec<TunnelView>, AppError> {
    state.require_unlocked()?;
    Ok(state
        .store
        .list_tunnels()
        .await?
        .into_iter()
        .map(|t| TunnelView {
            status: mgr.status(&t.id),
            tunnel: t,
        })
        .collect())
}

/// Create (no `id`) or update a tunnel. A running tunnel keeps its old settings until
/// it is restarted.
#[tauri::command]
#[specta::specta]
pub async fn tunnel_save(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    input: TunnelInput,
) -> Result<Tunnel, AppError> {
    state.require_unlocked()?;
    let input = input.normalized()?;
    let host = state.store.get_host(&input.host_id).await?;
    if host.protocol != Protocol::Ssh {
        return Err(AppError::invalid("Tunnels need an SSH host"));
    }
    Ok(state.store.save_tunnel(id.as_deref(), &input).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_delete(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    state.require_unlocked()?;
    stop(&app, &id).await;
    Ok(state.store.delete_tunnel(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_start(app: AppHandle, id: String) -> Result<TunnelStatus, AppError> {
    start(&app, &id).await
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_stop(app: AppHandle, id: String) -> Result<(), AppError> {
    stop(&app, &id).await;
    Ok(())
}

/// Start every tunnel marked "start on unlock" (the UI calls this after unlocking).
/// Returns how many are running.
#[tauri::command]
#[specta::specta]
pub async fn tunnels_autostart(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<u32, AppError> {
    state.require_unlocked()?;
    let mut n = 0;
    let auto: Vec<Tunnel> = state
        .store
        .list_tunnels()
        .await?
        .into_iter()
        .filter(|t| t.auto_start)
        .collect();
    for t in auto {
        match start(&app, &t.id).await {
            Ok(_) => n += 1,
            Err(e) => eprintln!("hopr: tunnel {} didn't start: {}", t.id, e.message),
        }
    }
    Ok(n)
}

/// Whether `addr:port` is free to listen on here (P8).
#[tauri::command]
#[specta::specta]
pub async fn port_free(addr: String, port: u16) -> bool {
    TcpListener::bind((addr.as_str(), port)).await.is_ok()
}

/// A free port on `addr`, chosen by the system (P8).
#[tauri::command]
#[specta::specta]
pub async fn port_pick(addr: String) -> Result<u16, AppError> {
    let l = TcpListener::bind((addr.as_str(), 0))
        .await
        .map_err(|e| bind_error(&addr, 0, &e))?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| AppError::internal("No free port", e))
}
