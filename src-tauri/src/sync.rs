//! Sync v1 (§10.8 Y1-lite): end-to-end encrypted sync through a folder the user already
//! syncs. The folder's key comes from a sync passphrase once; after that it is kept
//! encrypted under this device's vault, so syncing needs no passphrase.

use std::path::PathBuf;
use std::sync::Arc;

use data_encoding::BASE64;
use hopr_core::AppError;
use hopr_store::sync;
use hopr_vault::KdfParams;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::state::AppState;

const CONFIG_KEY: &str = "sync.config";
const KEY_KEY: &str = "sync.key";
const KEY_AAD: &[u8] = b"sync.key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    folder: String,
    device_id: String,
    device_name: String,
    last_sync: Option<String>,
    last_error: Option<String>,
    devices: Vec<SyncDevice>,
}

/// Another device seen in the folder.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncDevice {
    /// Its name.
    pub name: String,
    /// When it last wrote (RFC 3339).
    pub written_at: String,
}

/// Where sync stands.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Turned on for this device.
    pub enabled: bool,
    /// The folder.
    pub folder: Option<String>,
    /// This device's name.
    pub device_name: Option<String>,
    /// Last successful sync (RFC 3339).
    pub last_sync: Option<String>,
    /// Why the last sync failed.
    pub last_error: Option<String>,
    /// Other devices seen last time.
    pub devices: Vec<SyncDevice>,
}

/// What one sync did.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// Created or updated here.
    pub applied: u32,
    /// Deleted here.
    pub deleted: u32,
    /// Records published.
    pub pushed: u32,
    /// Other devices.
    pub devices: Vec<SyncDevice>,
    /// What couldn't be applied.
    pub warnings: Vec<String>,
}

/// One sync at a time.
#[derive(Default)]
pub struct SyncLock(Mutex<()>);

async fn config(state: &AppState) -> Result<Option<Config>, AppError> {
    Ok(state.store.setting::<Config>(CONFIG_KEY).await?)
}

fn status_of(c: Option<Config>) -> SyncStatus {
    match c {
        Some(c) => SyncStatus {
            enabled: true,
            folder: Some(c.folder),
            device_name: Some(c.device_name),
            last_sync: c.last_sync,
            last_error: c.last_error,
            devices: c.devices,
        },
        None => SyncStatus {
            enabled: false,
            folder: None,
            device_name: None,
            last_sync: None,
            last_error: None,
            devices: Vec::new(),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn sync_status(state: State<'_, Arc<AppState>>) -> Result<SyncStatus, AppError> {
    state.require_unlocked()?;
    Ok(status_of(config(&state).await?))
}

/// Turn sync on: open (or create) the folder's sync vault with `passphrase`, keep its key
/// under this vault, and sync once.
#[tauri::command]
#[specta::specta]
pub async fn sync_enable(
    state: State<'_, Arc<AppState>>,
    lock: State<'_, SyncLock>,
    folder: String,
    passphrase: String,
    device_name: String,
) -> Result<SyncReport, AppError> {
    let passphrase = SecretString::from(passphrase);
    let vault = state.vault()?;
    let folder = folder.trim().to_owned();
    let device_name = device_name.trim().to_owned();
    if folder.is_empty() {
        return Err(AppError::invalid("Choose the folder to sync through"));
    }
    if device_name.is_empty() || device_name.len() > 60 {
        return Err(AppError::invalid("Name this device (up to 60 characters)"));
    }
    if passphrase.expose_secret().chars().count() < 8 {
        return Err(AppError::invalid(
            "Use a sync passphrase of at least 8 characters",
        ));
    }
    let path = PathBuf::from(&folder);
    let key = tokio::task::spawn_blocking(move || {
        sync::open_folder(&path, &passphrase, KdfParams::DEFAULT)
    })
    .await
    .map_err(|e| AppError::internal("Sync setup failed", e))?
    .map_err(|e| match e {
        hopr_store::StoreError::Vault(_) => {
            AppError::invalid("That passphrase doesn't open this sync folder")
        }
        other => AppError::from(other),
    })?;
    let sealed = vault
        .encrypt(KEY_AAD, &key.export_dek())
        .map_err(|e| AppError::from(hopr_store::StoreError::from(e)))?;
    state
        .store
        .set_setting(KEY_KEY, &BASE64.encode(&sealed))
        .await?;
    let previous = config(&state).await?;
    let cfg = Config {
        folder,
        device_id: previous
            .map(|c| c.device_id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        device_name,
        last_sync: None,
        last_error: None,
        devices: Vec::new(),
    };
    state.store.set_setting(CONFIG_KEY, &cfg).await?;
    state.audit(
        "sync.enable",
        None,
        format!("Sync turned on ({})", cfg.folder),
    );
    run(&state, &lock).await
}

/// Sync now: merge the other devices' changes, then publish this device's.
#[tauri::command]
#[specta::specta]
pub async fn sync_now(
    state: State<'_, Arc<AppState>>,
    lock: State<'_, SyncLock>,
) -> Result<SyncReport, AppError> {
    run(&state, &lock).await
}

async fn run(state: &Arc<AppState>, lock: &SyncLock) -> Result<SyncReport, AppError> {
    let _one = lock.0.lock().await;
    let vault = state.vault()?;
    let mut cfg = config(state)
        .await?
        .ok_or_else(|| AppError::invalid("Sync isn't turned on"))?;
    let sealed: String = state
        .store
        .setting(KEY_KEY)
        .await?
        .ok_or_else(|| AppError::invalid("Turn sync on again: its key is missing"))?;
    let raw = BASE64
        .decode(sealed.as_bytes())
        .map_err(|_| AppError::invalid("The saved sync key is damaged; turn sync on again"))?;
    let dek: Zeroizing<Vec<u8>> = vault
        .decrypt(KEY_AAD, &raw)
        .map_err(|e| AppError::from(hopr_store::StoreError::from(e)))?;
    let folder = PathBuf::from(&cfg.folder);
    let result = async {
        let key = sync::open_folder_with_key(&folder, &dek)?;
        state
            .store
            .sync_folder(&vault, &key, &folder, &cfg.device_id, &cfg.device_name)
            .await
    }
    .await;
    match result {
        Ok(out) => {
            let devices: Vec<SyncDevice> = out
                .devices
                .iter()
                .map(|(name, at)| SyncDevice {
                    name: name.clone(),
                    written_at: at.clone(),
                })
                .collect();
            cfg.last_sync = OffsetDateTime::now_utc().format(&Rfc3339).ok();
            cfg.last_error = None;
            cfg.devices = devices.clone();
            state.store.set_setting(CONFIG_KEY, &cfg).await?;
            Ok(SyncReport {
                applied: out.applied,
                deleted: out.deleted,
                pushed: out.pushed,
                devices,
                warnings: out.warnings,
            })
        }
        Err(e) => {
            let err = AppError::from(e);
            cfg.last_error = Some(err.message.clone());
            let _ = state.store.set_setting(CONFIG_KEY, &cfg).await;
            Err(err)
        }
    }
}

/// Turn sync off on this device (the folder and other devices are left alone).
#[tauri::command]
#[specta::specta]
pub async fn sync_disable(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    state.require_unlocked()?;
    state.store.delete_setting(CONFIG_KEY).await?;
    state.store.delete_setting(KEY_KEY).await?;
    state.audit("sync.disable", None, "Sync turned off");
    Ok(())
}
