//! SSH keys and known hosts: generate, import, certificates, deploy to a host, rename, delete.

use std::time::Duration;

use hopr_core::AppError;
use hopr_core::model::{CertificateInfo, KeyAlgorithm, KnownHost, SshKeyInfo};
use hopr_ssh::ConnectParams;
use hopr_ssh::keys::{DEPLOY_COMMAND, DeployResult, deploy_outcome, deploy_script};
use hopr_store::NewKey;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::sessions;

use super::*;

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

pub(super) fn new_key(label: String, k: hopr_ssh::keys::ParsedKey) -> NewKey {
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
