//! Importing hosts from ~/.ssh/config and ~/.ssh/known_hosts.

use std::path::PathBuf;

use hopr_core::model::{AuthKind, Environment, HostInput, Protocol, SecretChange};
use hopr_core::{AppError, ErrorCode};
use hopr_import::{ConfigImport, FsIncludes};
use hopr_store::TrustKey;

use super::keys::new_key;
use super::*;

pub(super) fn home_dir() -> Result<PathBuf, AppError> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find your home folder"))
}

pub(super) fn read_ssh_config() -> Result<ConfigImport, AppError> {
    let home = home_dir()?;
    let dir = home.join(".ssh");
    let path = dir.join("config");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AppError::invalid(format!("No SSH config found at {}", path.display())))?;
    Ok(hopr_import::parse_ssh_config(
        &text,
        &dir,
        &home,
        &FsIncludes,
    ))
}

/// Preview hosts from `~/.ssh/config`.
#[tauri::command]
#[specta::specta]
pub async fn import_ssh_config_preview(state: S<'_>) -> Result<ConfigImport, AppError> {
    state.require_unlocked()?;
    tokio::task::spawn_blocking(read_ssh_config)
        .await
        .map_err(task_err)?
}

/// Import the selected `~/.ssh/config` hosts, storing readable identity files as keys.
#[tauri::command]
#[specta::specta]
pub async fn import_ssh_config_apply(
    state: S<'_>,
    aliases: Vec<String>,
) -> Result<ImportSummary, AppError> {
    let vault = state.vault()?;
    let parsed = tokio::task::spawn_blocking(read_ssh_config)
        .await
        .map_err(task_err)??;
    let mut summary = ImportSummary {
        warnings: parsed.warnings,
        ..ImportSummary::default()
    };
    let existing = state.store.list_hosts().await?;
    let os_user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" })
        .unwrap_or_else(|_| "root".into());

    // (created host id, alias, ProxyJump value), applied once every host exists.
    let mut jumps: Vec<(String, String, String)> = Vec::new();
    for h in parsed
        .hosts
        .into_iter()
        .filter(|h| aliases.contains(&h.alias))
    {
        let username = h.user.clone().unwrap_or_else(|| os_user.clone());
        if existing
            .iter()
            .any(|e| e.hostname == h.hostname && e.port == h.port && e.username == username)
        {
            summary.warnings.push(format!("{}: already saved", h.alias));
            continue;
        }
        // First identity file that exists and isn't passphrase-protected.
        let mut key_id = None;
        for path in &h.identity_files {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            match hopr_ssh::keys::import(&text, None) {
                Ok(parsed_key) => {
                    let known = state.store.list_keys().await?;
                    if let Some(k) = known
                        .iter()
                        .find(|k| k.fingerprint == parsed_key.fingerprint)
                    {
                        key_id = Some(k.id.clone());
                    } else {
                        let label = PathBuf::from(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "imported key".into());
                        let info = state
                            .store
                            .add_key(&vault, new_key(label, parsed_key))
                            .await?;
                        summary.keys += 1;
                        key_id = Some(info.id);
                    }
                    break;
                }
                Err(e) if e.code == ErrorCode::PassphraseRequired => {
                    summary.warnings.push(format!(
                        "{}: {path} is passphrase-protected; import it from the Keys screen",
                        h.alias
                    ))
                }
                Err(_) => {}
            }
        }
        let proxy_jump = h.proxy_jump.clone();
        let input = HostInput {
            label: h.alias.clone(),
            protocol: Protocol::Ssh,
            hostname: h.hostname,
            port: h.port,
            serial: None,
            proxy_command: h.proxy_command.clone(),
            username,
            group_id: None,
            tags: vec!["imported".into()],
            auth: if key_id.is_some() {
                AuthKind::Key
            } else {
                AuthKind::Password
            },
            key_id,
            jump_host_id: None,
            agent_forwarding: false,
            proxy: None,
            proxy_password: SecretChange::Keep,
            favorite: false,
            environment: Environment::None,
            notes: String::new(),
            password: SecretChange::Keep,
            totp_seed: SecretChange::Keep,
        };
        match input.normalized() {
            Ok(i) => {
                let created = state.store.create_host(&vault, i).await?;
                summary.hosts += 1;
                if let Some(jump) = proxy_jump {
                    jumps.push((created.id, h.alias.clone(), jump));
                }
            }
            Err(e) => summary.warnings.push(format!("{}: {}", h.alias, e.message)),
        }
    }

    // Second pass: ProxyJump names a Host alias, which is now a saved host's label.
    let hosts = state.store.list_hosts().await?;
    for (id, alias, jump) in jumps {
        if jump.contains(',') {
            summary.warnings.push(format!(
                "{alias}: multi-hop ProxyJump {jump} was not applied"
            ));
            continue;
        }
        // `user@host:port` forms name a host directly; match on its name.
        let name = jump
            .rsplit('@')
            .next()
            .unwrap_or(&jump)
            .split(':')
            .next()
            .unwrap_or(&jump);
        match hosts.iter().find(|x| x.label == name || x.hostname == name) {
            Some(j) => {
                if let Err(e) = state.store.set_jump_host(&id, Some(&j.id)).await {
                    summary
                        .warnings
                        .push(format!("{alias}: ProxyJump {jump}: {e}"));
                }
            }
            None => summary.warnings.push(format!(
                "{alias}: ProxyJump {jump} isn't a saved host; set it in the host editor"
            )),
        }
    }
    Ok(summary)
}

/// Trust every key in `~/.ssh/known_hosts`.
#[tauri::command]
#[specta::specta]
pub async fn import_known_hosts(state: S<'_>) -> Result<ImportSummary, AppError> {
    state.require_unlocked()?;
    let path = home_dir()?.join(".ssh").join("known_hosts");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AppError::invalid(format!("No known_hosts file at {}", path.display())))?;
    let parsed = hopr_import::parse_known_hosts(&text);
    let mut summary = ImportSummary {
        warnings: parsed.warnings,
        ..ImportSummary::default()
    };
    for e in &parsed.entries {
        state
            .store
            .trust_key(TrustKey {
                host_pattern: &e.host_pattern,
                port: e.port,
                key_type: &e.key_type,
                key_b64: &e.key_b64,
                fingerprint: &e.fingerprint,
                source: "imported",
            })
            .await?;
        summary.known_hosts += 1;
    }
    Ok(summary)
}
