//! Library IPC: hosts, groups, keys, trusted host keys, OpenSSH import, local shells.
//! Every command requires an unlocked vault.

use std::path::PathBuf;
use std::sync::Arc;

use std::time::{Duration, Instant};

use hopr_core::model::{
    AuditEvent, AuthKind, BulkEdit, CertificateInfo, Environment, Group, Host, HostInput,
    KeyAlgorithm, KnownHost, Protocol, SecretChange, SshKeyInfo,
};
use hopr_core::{AppError, ErrorCode};
use hopr_import::export::{ExportHost, to_csv, to_ssh_config};
use hopr_import::{ConfigImport, FsIncludes};
use hopr_ssh::ConnectParams;
use hopr_ssh::keys::{DEPLOY_COMMAND, DeployResult, deploy_outcome, deploy_script};
use hopr_store::{NewKey, TrustKey};
use hopr_term::ShellProfile;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::state::AppState;
use crate::{clipboard, sessions};

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

/// Attach an OpenSSH certificate to a key (or remove it with `null`). Hosts using the key
/// then log in with the certificate first, falling back to the bare key.
#[tauri::command]
#[specta::specta]
pub async fn key_set_certificate(
    state: S<'_>,
    id: String,
    certificate: Option<String>,
) -> Result<Option<CertificateInfo>, AppError> {
    state.require_unlocked()?;
    let key = state
        .store
        .list_keys()
        .await?
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| AppError::invalid("That key no longer exists"))?;
    let text = certificate
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty());
    let info = match &text {
        Some(c) if c.len() > 64 * 1024 => {
            return Err(AppError::invalid("That certificate is too large"));
        }
        Some(c) => Some(hopr_ssh::keys::parse_certificate(c, &key.public_key)?.1),
        None => None,
    };
    state
        .store
        .set_key_certificate(&id, text.as_deref())
        .await?;
    let detail = match &info {
        Some(i) => format!(
            "Attached certificate {} to {} (CA {})",
            i.key_id, key.label, i.ca_fingerprint
        ),
        None => format!("Removed the certificate from {}", key.label),
    };
    state.audit("key.certificate", None, detail);
    Ok(info)
}

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

/// Outcome of a deploy.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployReport {
    /// The key was added (false: it was already there).
    pub added: bool,
    /// The host now uses the key.
    pub switched: bool,
}

/// Authorize a stored key on a saved host (C17), logging in the way the host normally
/// does. `password` is used for this attempt only (never saved). With `use_key`, the
/// host switches to the key afterwards.
#[tauri::command]
#[specta::specta]
pub async fn key_deploy(
    state: S<'_>,
    host_id: String,
    key_id: String,
    password: Option<String>,
    use_key: bool,
) -> Result<DeployReport, AppError> {
    state.require_unlocked()?;
    let host = state.store.get_host(&host_id).await?;
    let key = state
        .store
        .list_keys()
        .await?
        .into_iter()
        .find(|k| k.id == key_id)
        .ok_or_else(|| AppError::invalid("That key no longer exists"))?;
    let script = deploy_script(&key.public_key)?;
    let password = password.filter(|p| !p.is_empty()).map(SecretString::from);
    let route = sessions::host_route(state.inner(), &host_id, password).await?;
    let params = ConnectParams {
        hops: route.hops,
        agent_forwarding: false,
        proxy: route.proxy,
        proxy_command: route.proxy_command,
        cols: 80,
        rows: 24,
        connect_timeout: Duration::from_secs(20),
        hooks: hopr_ssh::SessionHooks::default(),
        reconnect: None,
    };
    let who = Some((host.id.as_str(), host.label.as_str()));
    let result = match hopr_ssh::exec(params, DEPLOY_COMMAND, script.as_bytes()).await {
        Ok(out) => deploy_outcome(out.exit_status, &out.stdout, &out.stderr),
        Err(e) => Err(e),
    };
    let outcome = match result {
        Ok(o) => o,
        Err(e) => {
            state.audit(
                "key.deploy_failed",
                who,
                format!("{} ({}): {}", key.label, key.fingerprint, e.message),
            );
            return Err(e);
        }
    };
    let added = outcome == DeployResult::Added;
    state.audit(
        "key.deploy",
        who,
        format!(
            "{} {} ({})",
            key.label,
            if added {
                "added"
            } else {
                "was already authorized"
            },
            key.fingerprint
        ),
    );
    if use_key {
        state.store.set_auth_key(&host_id, &key_id).await?;
    }
    Ok(DeployReport {
        added,
        switched: use_key,
    })
}

/// Newest audit events first.
#[tauri::command]
#[specta::specta]
pub async fn audit_list(state: S<'_>, limit: u32) -> Result<Vec<AuditEvent>, AppError> {
    state.require_unlocked()?;
    Ok(state.store.audit_list(limit.min(10_000)).await?)
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

fn new_key(label: String, k: hopr_ssh::keys::ParsedKey) -> NewKey {
    NewKey {
        label,
        algorithm: k.algorithm,
        fingerprint: k.fingerprint,
        public_key: k.public_key,
        comment: k.comment,
        private_openssh: k.private_openssh,
    }
}

/// Generate a key pair and store it.
#[tauri::command]
#[specta::specta]
pub async fn key_generate(
    state: S<'_>,
    label: String,
    algorithm: KeyAlgorithm,
    comment: String,
) -> Result<SshKeyInfo, AppError> {
    let vault = state.vault()?;
    let label = label_ok(&label)?;
    let parsed = tokio::task::spawn_blocking(move || hopr_ssh::keys::generate(algorithm, &comment))
        .await
        .map_err(task_err)??;
    Ok(state.store.add_key(&vault, new_key(label, parsed)).await?)
}

/// Import a private key from text (OpenSSH, PEM, PKCS#8 or PuTTY).
#[tauri::command]
#[specta::specta]
pub async fn key_import(
    state: S<'_>,
    label: String,
    text: String,
    passphrase: Option<String>,
) -> Result<SshKeyInfo, AppError> {
    let vault = state.vault()?;
    let label = label_ok(&label)?;
    if text.len() > 64 * 1024 {
        return Err(AppError::invalid(
            "That file is too large to be a private key",
        ));
    }
    let parsed = hopr_ssh::keys::import(&text, passphrase.as_deref())?;
    Ok(state.store.add_key(&vault, new_key(label, parsed)).await?)
}

/// Rename a key.
#[tauri::command]
#[specta::specta]
pub async fn key_rename(state: S<'_>, id: String, label: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.rename_key(&id, &label_ok(&label)?).await?)
}

/// Delete a key (refused while hosts use it).
#[tauri::command]
#[specta::specta]
pub async fn key_delete(state: S<'_>, id: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    let label = state
        .store
        .list_keys()
        .await?
        .into_iter()
        .find(|k| k.id == id)
        .map(|k| format!("{} ({})", k.label, k.fingerprint));
    state.store.delete_key(&id).await?;
    if let Some(label) = label {
        state.audit("key.delete", None, label);
    }
    Ok(())
}

/// Trusted host keys.
#[tauri::command]
#[specta::specta]
pub async fn known_hosts_list(state: S<'_>) -> Result<Vec<KnownHost>, AppError> {
    state.require_unlocked()?;
    Ok(state.store.list_known_hosts().await?)
}

/// Forget a trusted host key.
#[tauri::command]
#[specta::specta]
pub async fn known_host_delete(state: S<'_>, id: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    let what = state
        .store
        .list_known_hosts()
        .await?
        .into_iter()
        .find(|k| k.id == id)
        .map(|k| format!("{}:{} ({})", k.host_pattern, k.port, k.fingerprint));
    state.store.delete_known_host(&id).await?;
    if let Some(what) = what {
        state.audit("host_key.forget", None, what);
    }
    Ok(())
}

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

fn home_dir() -> Result<PathBuf, AppError> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find your home folder"))
}

fn read_ssh_config() -> Result<ConfigImport, AppError> {
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
