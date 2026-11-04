//! Tunnels (§9.11, P1–P8): saved local / remote / dynamic forwards, each on its own
//! headless SSH connection (through the host's jump chain, proxy or ProxyCommand), with a
//! 1 Hz stats event while any is running.

pub mod commands;
pub mod runtime;

pub use runtime::TunnelManager;

use hopr_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

fn poisoned() -> AppError {
    AppError::new(ErrorCode::Internal, "Tunnel registry poisoned")
}

/// Where a tunnel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    /// Not running.
    Stopped,
    /// Connecting.
    Starting,
    /// Listening and forwarding.
    Running,
    /// Couldn't start, or the connection dropped.
    Failed,
}

/// Live status of one tunnel.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    /// Tunnel id.
    pub id: String,
    /// State.
    pub state: TunnelState,
    /// Why it failed.
    pub error: Option<String>,
    /// The port actually listened on (differs from the saved one when that was 0).
    pub bound_port: Option<u16>,
    /// Bytes towards the destination.
    #[specta(type = u32)]
    pub up: f64,
    /// Bytes back.
    #[specta(type = u32)]
    pub down: f64,
    /// Bytes per second towards the destination (last second).
    #[specta(type = u32)]
    pub up_rate: f64,
    /// Bytes per second back.
    #[specta(type = u32)]
    pub down_rate: f64,
    /// Connections open now.
    pub active: u32,
    /// Connections since start.
    pub total: u32,
}

impl TunnelStatus {
    fn idle(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            state: TunnelState::Stopped,
            error: None,
            bound_port: None,
            up: 0.0,
            down: 0.0,
            up_rate: 0.0,
            down_rate: 0.0,
            active: 0,
            total: 0,
        }
    }
}

/// Every live tunnel's status, once a second while any runs.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatsEvent {
    /// Statuses (running, starting, failed, and ones that just stopped).
    pub tunnels: Vec<TunnelStatus>,
}
