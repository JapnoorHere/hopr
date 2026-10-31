//! Import sessions from PuTTY, MobaXterm, SecureCRT and CSV files (§9.24): preview what
//! a file (or, for PuTTY on Windows, the registry) holds, then create the chosen hosts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hopr_core::AppError;
use hopr_core::model::{AuthKind, Environment, HostInput, Protocol, SecretChange};
use hopr_import::foreign::{self, ForeignHost, ForeignImport, ForeignSource};
use hopr_store::NewKey;
use tauri::State;

use crate::library::ImportSummary;
use crate::state::AppState;

fn read_file(path: &Path) -> Result<String, AppError> {
    let meta = std::fs::metadata(path).map_err(|e| {
        AppError::invalid(format!("Couldn't open {}", path.display())).with_detail(e.to_string())
    })?;
    if meta.len() > 32 * 1024 * 1024 {
        return Err(AppError::invalid(
            "That file is too large to be a session export",
        ));
    }
    std::fs::read(path)
        .map(|b| foreign::decode_text(&b))
        .map_err(|e| {
            AppError::invalid(format!("Couldn't read {}", path.display()))
                .with_detail(e.to_string())
        })
}

#[cfg(windows)]
fn putty_registry() -> Result<foreign::PuttySessions, AppError> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, RegType};
    let root = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\SimonTatham\\PuTTY\\Sessions")
        .map_err(|_| AppError::invalid("PuTTY has no saved sessions on this computer"))?;
    let mut out = Vec::new();
    for name in root.enum_keys().flatten() {
        let Ok(key) = root.open_subkey(&name) else {
            continue;
        };
        let mut values = HashMap::new();
        for (vname, v) in key.enum_values().flatten() {
            let value = match v.vtype {
                RegType::REG_DWORD => v
                    .bytes
                    .get(..4)
                    .and_then(|b| <[u8; 4]>::try_from(b).ok())
                    .map(|b| foreign::PuttyValue::Dword(u32::from_le_bytes(b))),
                RegType::REG_SZ | RegType::REG_EXPAND_SZ => {
                    let units: Vec<u16> = v
                        .bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_le_bytes(*c))
                        .take_while(|u| *u != 0)
                        .collect();
                    Some(foreign::PuttyValue::Str(String::from_utf16_lossy(&units)))
                }
                _ => None,
            };
            if let Some(value) = value {
                values.insert(vname, value);
            }
        }
        out.push((name, values));
    }
    Ok(out)
}

#[cfg(not(windows))]
fn putty_registry() -> Result<foreign::PuttySessions, AppError> {
    Err(AppError::invalid(
        "Choose a PuTTY .reg export (sessions live in the Windows registry)",
    ))
}

/// Parse a source. `path` is the export file (a folder for SecureCRT); PuTTY without a
/// path reads this computer's registry.
fn parse(source: ForeignSource, path: Option<&Path>) -> Result<ForeignImport, AppError> {
    match (source, path) {
        (ForeignSource::Putty, None) => Ok(foreign::from_putty(putty_registry()?)),
        (ForeignSource::Putty, Some(p)) => Ok(foreign::from_putty(foreign::parse_putty_reg(
            &read_file(p)?,
        ))),
        (ForeignSource::Mobaxterm, Some(p)) => Ok(foreign::parse_mobaxterm(&read_file(p)?)),
        (ForeignSource::Securecrt, Some(p)) if p.is_dir() => Ok(foreign::read_securecrt_dir(p)),
        (ForeignSource::Securecrt, Some(p)) => {
            let name = p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "session".into());
            let mut out = ForeignImport::default();
            match foreign::parse_securecrt_session(&name, None, &read_file(p)?) {
                Ok(h) => out.hosts.push(h),
                Err(why) => out.warnings.push(format!("{name}: {why}")),
            }
            Ok(out)
        }
        (ForeignSource::Csv, Some(p)) => Ok(foreign::parse_csv(&read_file(p)?)),
        (_, None) => Err(AppError::invalid("Choose the file to import")),
    }
}

async fn parse_async(
    source: ForeignSource,
    path: Option<String>,
) -> Result<ForeignImport, AppError> {
    tokio::task::spawn_blocking(move || parse(source, path.as_deref().map(Path::new)))
        .await
        .map_err(|e| AppError::internal("Background task failed", e))?
}

/// What an export (or PuTTY's registry, with no `path`) holds.
#[tauri::command]
#[specta::specta]
pub async fn import_foreign_preview(
    state: State<'_, Arc<AppState>>,
    source: ForeignSource,
    path: Option<String>,
) -> Result<ForeignImport, AppError> {
    state.require_unlocked()?;
    parse_async(source, path).await
}

fn same_host(a: &hopr_core::model::Host, b: &ForeignHost) -> bool {
    a.protocol == b.protocol
        && a.hostname.eq_ignore_ascii_case(&b.hostname)
        && a.port == b.port
        && a.username == b.username
}

/// Create the chosen sessions (by label) as hosts. Folders become groups; readable
/// OpenSSH key files are stored as keys; passwords are never imported.
#[tauri::command]
#[specta::specta]
pub async fn import_foreign_apply(
    state: State<'_, Arc<AppState>>,
    source: ForeignSource,
    path: Option<String>,
    labels: Vec<String>,
) -> Result<ImportSummary, AppError> {
    let vault = state.vault()?;
    let parsed = parse_async(source, path).await?;
    let mut summary = ImportSummary::default();
    let existing = state.store.list_hosts().await?;
    let mut groups: HashMap<String, String> = state
        .store
        .list_groups()
        .await?
        .into_iter()
        .map(|g| (g.name.to_lowercase(), g.id))
        .collect();
    let os_user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" })
        .unwrap_or_else(|_| "root".into());

    for h in parsed
        .hosts
        .into_iter()
        .filter(|h| labels.contains(&h.label))
    {
        if existing.iter().any(|e| same_host(e, &h)) {
            summary.warnings.push(format!("{}: already saved", h.label));
            continue;
        }
        let group_id = match &h.group {
            Some(name) => match groups.get(&name.to_lowercase()) {
                Some(id) => Some(id.clone()),
                None => {
                    let g = state.store.create_group(name).await?;
                    groups.insert(name.to_lowercase(), g.id.clone());
                    Some(g.id)
                }
            },
            None => None,
        };
        let mut key_id = None;
        if let Some(file) = &h.key_file {
            match std::fs::read_to_string(file).map(|t| hopr_ssh::keys::import(&t, None)) {
                Ok(Ok(k)) => {
                    let known = state.store.list_keys().await?;
                    if let Some(found) = known.iter().find(|x| x.fingerprint == k.fingerprint) {
                        key_id = Some(found.id.clone());
                    } else {
                        let label = PathBuf::from(file)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "imported key".into());
                        let info = state
                            .store
                            .add_key(
                                &vault,
                                NewKey {
                                    label,
                                    algorithm: k.algorithm,
                                    fingerprint: k.fingerprint,
                                    public_key: k.public_key,
                                    comment: k.comment,
                                    private_openssh: k.private_openssh,
                                },
                            )
                            .await?;
                        summary.keys += 1;
                        key_id = Some(info.id);
                    }
                }
                _ => summary.warnings.push(format!(
                    "{}: couldn't use the key file {file}; add it from the Keys screen",
                    h.label
                )),
            }
        }
        let username = if h.username.is_empty() && h.protocol == Protocol::Ssh {
            os_user.clone()
        } else {
            h.username.clone()
        };
        let input = HostInput {
            label: h.label.clone(),
            protocol: h.protocol,
            hostname: h.hostname.clone(),
            port: h.port,
            serial: h.serial,
            proxy_command: h.proxy_command.clone(),
            username,
            group_id,
            tags: vec!["imported".into(), source.tag().into()],
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
                state.store.create_host(&vault, i).await?;
                summary.hosts += 1;
            }
            Err(e) => summary.warnings.push(format!("{}: {}", h.label, e.message)),
        }
    }
    summary.warnings.extend(parsed.warnings);
    state.audit(
        "import",
        None,
        format!("Imported {} hosts from {}", summary.hosts, source.tag()),
    );
    Ok(summary)
}
