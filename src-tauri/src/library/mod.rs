//! Library IPC: hosts, groups, keys, trusted host keys, OpenSSH import, local shells.
//! Every command requires an unlocked vault.

pub mod hosts;
pub mod import;
pub mod keys;
pub mod system;

use std::sync::Arc;

use hopr_core::AppError;
use hopr_core::model::{AuditEvent, Group, Host, SshKeyInfo};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::state::AppState;

type S<'a> = State<'a, Arc<AppState>>;

/// Everything the hosts screen shows.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    /// Saved hosts.
    pub hosts: Vec<Host>,
    /// Groups.
    pub groups: Vec<Group>,
    /// Stored keys.
    pub keys: Vec<SshKeyInfo>,
}

/// Outcome of an import.
#[derive(Serialize, Deserialize, Clone, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// Hosts created.
    pub hosts: u32,
    /// Keys stored.
    pub keys: u32,
    /// Trusted host keys added.
    pub known_hosts: u32,
    /// What was skipped and why.
    pub warnings: Vec<String>,
}

fn task_err(e: tokio::task::JoinError) -> AppError {
    AppError::internal("Background task failed", e)
}

fn label_ok(s: &str) -> Result<String, AppError> {
    let t = s.trim();
    if t.is_empty() || t.len() > 120 || t.chars().any(char::is_control) {
        return Err(AppError::invalid("Enter a name (up to 120 characters)"));
    }
    Ok(t.to_owned())
}

/// Hosts, groups and keys in one call.
#[tauri::command]
#[specta::specta]
pub async fn library_get(state: S<'_>) -> Result<Library, AppError> {
    state.require_unlocked()?;
    let mut keys = state.store.list_keys().await?;
    for k in &mut keys {
        k.certificate_info = k
            .certificate
            .as_deref()
            .and_then(|c| hopr_ssh::keys::parse_certificate(c, &k.public_key).ok())
            .map(|(_, info)| info);
    }
    Ok(Library {
        hosts: state.store.list_hosts().await?,
        groups: state.store.list_groups().await?,
        keys,
    })
}

/// Newest audit events first.
#[tauri::command]
#[specta::specta]
pub async fn audit_list(state: S<'_>, limit: u32) -> Result<Vec<AuditEvent>, AppError> {
    state.require_unlocked()?;
    Ok(state.store.audit_list(limit.min(10_000)).await?)
}
