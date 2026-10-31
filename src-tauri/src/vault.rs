//! Vault IPC: onboarding, unlock (password / remembered / recovery), lock, password change,
//! auto-lock setting. Argon2 runs on a blocking thread so the UI stays responsive.

use std::sync::Arc;

use hopr_core::{AppError, ErrorCode};
use hopr_vault::{KdfParams, VaultError, VaultHeader};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::state::{AUTO_LOCK_KEY, AppState, keychain};
use crate::{backups, biometric, clipboard};

type S<'a> = State<'a, Arc<AppState>>;

/// What the UI needs to decide between onboarding, unlock and the app.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// A vault has been created.
    pub exists: bool,
    /// The vault is unlocked.
    pub unlocked: bool,
    /// A remembered key is in the OS keychain.
    pub remembered: bool,
    /// Auto-lock timeout in minutes (0 = never).
    pub auto_lock_minutes: u32,
    /// Seconds before a copied secret is cleared from the clipboard (0 = never).
    pub clipboard_clear_secs: u32,
    /// Unlock with Windows Hello is on.
    pub biometric: bool,
    /// Windows Hello is set up on this device.
    pub biometric_available: bool,
}

fn vault_err(e: VaultError) -> AppError {
    match e {
        VaultError::WrongPassword => AppError::invalid("Incorrect password"),
        VaultError::MalformedRecoveryKey => {
            AppError::invalid("That doesn't look like a recovery key")
        }
        other => AppError::internal("Vault error", other),
    }
}

fn check_new_password(p: &str) -> Result<(), AppError> {
    if p.chars().count() < 8 {
        return Err(AppError::invalid("Use at least 8 characters"));
    }
    if p.len() > 1024 {
        return Err(AppError::invalid("That password is too long"));
    }
    Ok(())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, AppError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::internal("Background task failed", e))
}

async fn header(state: &AppState) -> Result<VaultHeader, AppError> {
    state
        .store
        .vault_header()
        .await?
        .ok_or_else(|| AppError::invalid("Create a vault first"))
}

/// Current vault status.
#[tauri::command]
#[specta::specta]
pub async fn vault_status(state: S<'_>) -> Result<VaultStatus, AppError> {
    Ok(VaultStatus {
        exists: state.store.vault_header().await?.is_some(),
        unlocked: state.is_unlocked(),
        remembered: keychain::exists(),
        auto_lock_minutes: state.auto_lock_minutes(),
        clipboard_clear_secs: state.clipboard_clear_secs(),
        biometric: biometric_on(&state).await,
        biometric_available: biometric::available().await,
    })
}

async fn biometric_on(state: &AppState) -> bool {
    state
        .store
        .setting::<bool>(biometric::BIOMETRIC_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

/// Shared tail of every successful unlock.
fn unlocked(state: &S<'_>, vault: hopr_vault::UnlockedVault, how: &str) {
    state.set_vault(vault);
    state.audit("vault.unlock", None, how.to_owned());
    backups::spawn_auto_backup(state.inner().clone());
}

/// Require Windows Hello before the remembered key may unlock (or stop requiring it).
#[tauri::command]
#[specta::specta]
pub async fn vault_set_biometric(state: S<'_>, enabled: bool) -> Result<(), AppError> {
    let vault = state.vault()?;
    if enabled {
        if !biometric::available().await {
            return Err(AppError::invalid(
                "Windows Hello isn't set up on this device. Set it up in Windows Settings first.",
            ));
        }
        if !keychain::store(&vault.export_dek()) {
            return Err(AppError::new(
                ErrorCode::Internal,
                "Couldn't store the vault key in Windows Credential Manager",
            ));
        }
    } else {
        keychain::forget();
    }
    state
        .store
        .set_setting(biometric::BIOMETRIC_KEY, &enabled)
        .await?;
    state.audit(
        "settings.biometric",
        None,
        if enabled {
            "Unlock with Windows Hello turned on"
        } else {
            "Unlock with Windows Hello turned off"
        },
    );
    Ok(())
}

/// Unlock after a Windows Hello check, using the key kept in Credential Manager.
#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_biometric(state: S<'_>) -> Result<(), AppError> {
    if !biometric_on(&state).await {
        return Err(AppError::invalid("Unlock with Windows Hello is off"));
    }
    let header = header(&state).await?;
    let ok = biometric::verify("Unlock Hopr".into())
        .await
        .map_err(|e| AppError::new(ErrorCode::Internal, "Windows Hello failed").with_detail(e))?;
    if !ok {
        state.audit("vault.unlock_failed", None, "Windows Hello not confirmed");
        return Err(AppError::invalid("Windows Hello didn't confirm it's you"));
    }
    let dek = keychain::load().ok_or_else(|| {
        AppError::invalid("This device no longer has the vault key; enter your password")
    })?;
    let vault = hopr_vault::unlock_with_dek(&header, &dek)
        .map_err(|_| AppError::invalid("The stored key no longer matches; enter your password"))?;
    unlocked(&state, vault, "Windows Hello");
    Ok(())
}

/// Create the vault. Returns the recovery key, shown to the user exactly once.
#[tauri::command]
#[specta::specta]
pub async fn vault_create(state: S<'_>, password: String) -> Result<String, AppError> {
    if state.store.vault_header().await?.is_some() {
        return Err(AppError::invalid("A vault already exists"));
    }
    check_new_password(&password)?;
    let password = SecretString::from(password);
    let nv = blocking(move || hopr_vault::create(&password, KdfParams::DEFAULT))
        .await?
        .map_err(vault_err)?;
    state.store.save_vault_header(&nv.header).await?;
    let recovery = nv.recovery_key.expose_secret().to_owned();
    state.set_vault(nv.vault);
    Ok(recovery)
}

/// Unlock with the master password, optionally remembering this device.
#[tauri::command]
#[specta::specta]
pub async fn vault_unlock(state: S<'_>, password: String, remember: bool) -> Result<(), AppError> {
    let header = header(&state).await?;
    let password = SecretString::from(password);
    let vault = blocking(move || hopr_vault::unlock(&header, &password))
        .await?
        .map_err(|e| {
            if matches!(e, VaultError::WrongPassword) {
                state.audit("vault.unlock_failed", None, "Wrong master password");
            }
            vault_err(e)
        })?;
    if remember {
        if !keychain::store(&vault.export_dek()) {
            eprintln!("hopr: could not store the vault key in the OS keychain");
        }
    } else if !biometric_on(&state).await {
        // Windows Hello unlock keeps using the stored key.
        keychain::forget();
    }
    unlocked(
        &state,
        vault,
        if remember {
            "Master password (remember this device)"
        } else {
            "Master password"
        },
    );
    Ok(())
}

/// Unlock with the key remembered in the OS keychain.
#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_remembered(state: S<'_>) -> Result<(), AppError> {
    if biometric_on(&state).await {
        return Err(AppError::invalid(
            "Unlock with Windows Hello or your password",
        ));
    }
    let header = header(&state).await?;
    let dek = keychain::load()
        .ok_or_else(|| AppError::invalid("This device doesn't remember the vault key"))?;
    match hopr_vault::unlock_with_dek(&header, &dek) {
        Ok(v) => {
            unlocked(&state, v, "Remembered device key");
            Ok(())
        }
        Err(_) => {
            keychain::forget(); // stale (e.g. vault re-created)
            Err(AppError::invalid(
                "The remembered key no longer matches; enter your password",
            ))
        }
    }
}

/// Reset the master password with the recovery key.
#[tauri::command]
#[specta::specta]
pub async fn vault_recover(
    state: S<'_>,
    recovery_key: String,
    new_password: String,
) -> Result<(), AppError> {
    check_new_password(&new_password)?;
    let header = header(&state).await?;
    let rk = SecretString::from(recovery_key);
    let np = SecretString::from(new_password);
    let (vault, new_header) = blocking(move || {
        let v = hopr_vault::unlock_with_recovery(&header, &rk)?;
        let h = v.rewrap(&header, &np)?;
        Ok::<_, VaultError>((v, h))
    })
    .await?
    .map_err(vault_err)?;
    state.store.save_vault_header(&new_header).await?;
    keychain::forget();
    state
        .store
        .set_setting(biometric::BIOMETRIC_KEY, &false)
        .await?;
    state.set_vault(vault);
    state.audit(
        "vault.recover",
        None,
        "Master password reset with the recovery key",
    );
    Ok(())
}

/// Change the master password (data is not re-encrypted).
#[tauri::command]
#[specta::specta]
pub async fn vault_change_password(
    state: S<'_>,
    current: String,
    new_password: String,
) -> Result<(), AppError> {
    check_new_password(&new_password)?;
    let header = header(&state).await?;
    let cur = SecretString::from(current);
    let np = SecretString::from(new_password);
    let new_header = blocking(move || {
        let v = hopr_vault::unlock(&header, &cur)?;
        v.rewrap(&header, &np)
    })
    .await?
    .map_err(vault_err)?;
    state.store.save_vault_header(&new_header).await?;
    state.audit("vault.password_changed", None, "Master password changed");
    Ok(())
}

/// Lock now (Ctrl+Shift+L). Sessions keep running behind the lock screen.
#[tauri::command]
#[specta::specta]
pub fn vault_lock(state: S<'_>) {
    if state.is_unlocked() {
        state.audit("vault.lock", None, "Locked manually");
    }
    state.lock();
}

/// Forget the remembered key on this device.
#[tauri::command]
#[specta::specta]
pub async fn vault_forget_device(state: S<'_>) -> Result<(), AppError> {
    keychain::forget();
    state
        .store
        .set_setting(biometric::BIOMETRIC_KEY, &false)
        .await?;
    state.audit(
        "vault.forget_device",
        None,
        "This device no longer remembers the vault key",
    );
    Ok(())
}

/// The user is active (throttled pings from the UI): postpones auto-lock.
#[tauri::command]
#[specta::specta]
pub fn vault_activity(state: S<'_>) {
    if state.is_unlocked() {
        state.touch();
    }
}

/// Set the auto-lock timeout (0 = never).
#[tauri::command]
#[specta::specta]
pub async fn vault_set_auto_lock(state: S<'_>, minutes: u32) -> Result<(), AppError> {
    state.require_unlocked()?;
    if minutes > 24 * 60 {
        return Err(AppError::invalid("Choose at most 24 hours"));
    }
    state.store.set_setting(AUTO_LOCK_KEY, &minutes).await?;
    state.set_auto_lock_minutes(minutes);
    state.audit(
        "settings.auto_lock",
        None,
        if minutes == 0 {
            "Auto-lock turned off".to_owned()
        } else {
            format!("Auto-lock after {minutes} min")
        },
    );
    Ok(())
}

/// Set how long a copied secret stays on the clipboard (0 = until replaced).
#[tauri::command]
#[specta::specta]
pub async fn vault_set_clipboard_clear(state: S<'_>, seconds: u32) -> Result<(), AppError> {
    state.require_unlocked()?;
    if seconds > 600 {
        return Err(AppError::invalid("Choose at most 10 minutes"));
    }
    state
        .store
        .set_setting(clipboard::CLEAR_KEY, &seconds)
        .await?;
    state.set_clipboard_clear_secs(seconds);
    state.audit(
        "settings.clipboard_clear",
        None,
        if seconds == 0 {
            "Copied secrets are no longer cleared".to_owned()
        } else {
            format!("Copied secrets clear after {seconds} s")
        },
    );
    Ok(())
}
