//! Saved hosts and groups: create, edit, delete, duplicate, bulk edit, export, reachability, favourites.

use std::time::{Duration, Instant};

use hopr_core::AppError;
use hopr_core::model::{BulkEdit, Group, Host, HostInput};
use hopr_import::export::{ExportHost, to_csv, to_ssh_config};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::clipboard;

use super::*;

/// Create a host.
#[tauri::command]
#[specta::specta]
pub async fn host_create(state: S<'_>, input: HostInput) -> Result<Host, AppError> {
    let vault = state.vault()?;
    Ok(state.store.create_host(&vault, input.normalized()?).await?)
}

/// Update a host.
#[tauri::command]
#[specta::specta]
pub async fn host_update(state: S<'_>, id: String, input: HostInput) -> Result<Host, AppError> {
    let vault = state.vault()?;
    Ok(state
        .store
        .update_host(&vault, &id, input.normalized()?)
        .await?)
}

/// Soft-delete hosts (the UI offers Undo).
#[tauri::command]
#[specta::specta]
pub async fn hosts_delete(state: S<'_>, ids: Vec<String>) -> Result<(), AppError> {
    state.require_unlocked()?;
    for id in &ids {
        let label = state.store.get_host(id).await.ok().map(|h| h.label);
        state.store.delete_host(id).await?;
        if let Some(label) = label {
            state.audit("host.delete", Some((id, &label)), "Deleted");
        }
    }
    Ok(())
}

/// Undo a delete.
#[tauri::command]
#[specta::specta]
pub async fn hosts_restore(state: S<'_>, ids: Vec<String>) -> Result<(), AppError> {
    state.require_unlocked()?;
    for id in &ids {
        state.store.restore_host(id).await?;
    }
    Ok(())
}

/// Copy a host with its settings and secrets ("label (copy)").
#[tauri::command]
#[specta::specta]
pub async fn host_duplicate(state: S<'_>, id: String) -> Result<Host, AppError> {
    let vault = state.vault()?;
    Ok(state.store.duplicate_host(&vault, &id).await?)
}

/// Apply one change to many hosts. Returns how many changed.
#[tauri::command]
#[specta::specta]
pub async fn hosts_bulk_edit(state: S<'_>, edit: BulkEdit) -> Result<u32, AppError> {
    state.require_unlocked()?;
    if edit.ids.len() > 10_000 {
        return Err(AppError::invalid("Too many hosts in one edit"));
    }
    let n = state.store.bulk_edit(&edit).await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Export formats (H8).
#[derive(Deserialize, Serialize, Clone, Copy, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// OpenSSH client config.
    SshConfig,
    /// Spreadsheet.
    Csv,
}

/// Export hosts (all when `ids` is empty) without any secrets.
#[tauri::command]
#[specta::specta]
pub async fn hosts_export(
    state: S<'_>,
    format: ExportFormat,
    ids: Vec<String>,
) -> Result<String, AppError> {
    state.require_unlocked()?;
    let hosts = state.store.list_hosts().await?;
    let groups = state.store.list_groups().await?;
    let label_of = |id: &Option<String>| {
        id.as_ref()
            .and_then(|id| hosts.iter().find(|h| &h.id == id))
            .map(|h| h.label.as_str())
    };
    let picked: Vec<&Host> = hosts
        .iter()
        .filter(|h| ids.is_empty() || ids.contains(&h.id))
        .collect();
    let rows: Vec<ExportHost<'_>> = picked
        .iter()
        .map(|h| ExportHost {
            label: &h.label,
            hostname: &h.hostname,
            port: h.port,
            username: &h.username,
            jump_label: label_of(&h.jump_host_id),
            group: h
                .group_id
                .as_ref()
                .and_then(|g| groups.iter().find(|x| &x.id == g))
                .map(|g| g.name.as_str()),
            tags: &h.tags,
            environment: h.environment.as_str(),
            proxy: h
                .proxy
                .as_ref()
                .map(|p| (p.protocol.as_str(), p.host.as_str(), p.port)),
            notes: &h.notes,
        })
        .collect();
    let (text, name) = match format {
        ExportFormat::SshConfig => (to_ssh_config(&rows), "ssh_config"),
        ExportFormat::Csv => (to_csv(&rows), "CSV"),
    };
    state.audit(
        "hosts.export",
        None,
        format!("Exported {} hosts as {name}", rows.len()),
    );
    Ok(text)
}

/// Reachability of one host.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    /// Host id.
    pub id: String,
    /// TCP connect time in milliseconds; `None` when unreachable or not probed.
    pub ms: Option<u32>,
    /// `reachable`, `unreachable`, or `skipped` (behind a jump host or proxy).
    pub status: String,
}

/// TCP-probe hosts (H9). Hosts behind a jump host or proxy are skipped: only their first
/// hop is reachable from here.
#[tauri::command]
#[specta::specta]
pub async fn hosts_ping(state: S<'_>, ids: Vec<String>) -> Result<Vec<PingResult>, AppError> {
    state.require_unlocked()?;
    let hosts = state.store.list_hosts().await?;
    let mut set = tokio::task::JoinSet::new();
    for h in hosts.into_iter().filter(|h| ids.contains(&h.id)).take(500) {
        set.spawn(async move {
            if h.jump_host_id.is_some() || h.proxy.is_some() {
                return PingResult {
                    id: h.id,
                    ms: None,
                    status: "skipped".into(),
                };
            }
            let start = Instant::now();
            let probe = tokio::time::timeout(
                Duration::from_secs(3),
                tokio::net::TcpStream::connect((h.hostname.as_str(), h.port)),
            )
            .await;
            match probe {
                Ok(Ok(_)) => PingResult {
                    id: h.id,
                    ms: Some(u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX)),
                    status: "reachable".into(),
                },
                _ => PingResult {
                    id: h.id,
                    ms: None,
                    status: "unreachable".into(),
                },
            }
        });
    }
    let mut out = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(r) = r {
            out.push(r);
        }
    }
    Ok(out)
}

/// Copy a host's saved password to the clipboard without it passing through the UI.
/// Returns the seconds until it is cleared (0 = not cleared).
#[tauri::command]
#[specta::specta]
pub async fn host_copy_password(state: S<'_>, id: String) -> Result<u32, AppError> {
    let vault = state.vault()?;
    let host = state.store.get_host(&id).await?;
    let creds = state.store.host_credentials(&vault, &id).await?;
    let pw = creds
        .password
        .ok_or_else(|| AppError::invalid(format!("{} has no saved password", host.label)))?;
    let secs = state.clipboard_clear_secs();
    clipboard::copy_secret(pw.expose_secret().to_owned(), secs).await?;
    state.audit(
        "host.copy_password",
        Some((&host.id, &host.label)),
        "Password copied to the clipboard",
    );
    Ok(secs)
}

/// Pin or unpin a host.
#[tauri::command]
#[specta::specta]
pub async fn host_set_favorite(state: S<'_>, id: String, favorite: bool) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.set_favorite(&id, favorite).await?)
}

/// Create a group.
#[tauri::command]
#[specta::specta]
pub async fn group_create(state: S<'_>, name: String) -> Result<Group, AppError> {
    state.require_unlocked()?;
    Ok(state.store.create_group(&label_ok(&name)?).await?)
}

/// Rename a group.
#[tauri::command]
#[specta::specta]
pub async fn group_rename(state: S<'_>, id: String, name: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.rename_group(&id, &label_ok(&name)?).await?)
}

/// Delete a group (its hosts become ungrouped).
#[tauri::command]
#[specta::specta]
pub async fn group_delete(state: S<'_>, id: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.delete_group(&id).await?)
}
