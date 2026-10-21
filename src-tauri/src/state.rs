//! App-wide state: the database, the unlocked vault (if any) and auto-lock bookkeeping.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hopr_core::{AppError, ErrorCode};
use hopr_store::Store;
use hopr_vault::UnlockedVault;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

/// Settings key for the auto-lock timeout (minutes, 0 = never).
pub const AUTO_LOCK_KEY: &str = "vault.auto_lock_minutes";
/// Default auto-lock (HOPR_PLAN §6).
pub const AUTO_LOCK_DEFAULT: u32 = 15;

/// Emitted when the vault locks on its own (idle timeout).
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VaultLockedEvent {
    /// "idle".
    pub reason: String,
}

pub struct AppState {
    pub store: Store,
    vault: RwLock<Option<Arc<UnlockedVault>>>,
    last_activity: AtomicU64,
    auto_lock_minutes: AtomicU32,
    clipboard_clear_secs: AtomicU32,
    /// Where `hopr.db` and `backups/` live.
    pub data_dir: PathBuf,
    /// Tabs handed to a new window, by window label, until it picks them up.
    pub handoffs: Mutex<HashMap<String, String>>,
}

/// A host named in an audit entry: `(id, label)`.
pub type AuditHost<'a> = Option<(&'a str, &'a str)>;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl AppState {
    pub fn new(
        store: Store,
        data_dir: PathBuf,
        auto_lock_minutes: u32,
        clipboard_clear_secs: u32,
    ) -> Self {
        Self {
            store,
            vault: RwLock::new(None),
            last_activity: AtomicU64::new(now_secs()),
            auto_lock_minutes: AtomicU32::new(auto_lock_minutes),
            clipboard_clear_secs: AtomicU32::new(clipboard_clear_secs),
            data_dir,
            handoffs: Mutex::new(HashMap::new()),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("hopr.db")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Record an audit event in the background (X7). Failures are logged, never surfaced:
    /// auditing must not break the action it describes.
    pub fn audit(&self, kind: &'static str, host: AuditHost<'_>, detail: impl Into<String>) {
        let store = self.store.clone();
        let host = host.map(|(id, label)| (id.to_owned(), label.to_owned()));
        let detail = detail.into();
        tauri::async_runtime::spawn(async move {
            let res = store
                .audit(hopr_store::NewAudit {
                    kind,
                    host_id: host.as_ref().map(|h| h.0.as_str()),
                    host_label: host.as_ref().map(|h| h.1.as_str()),
                    detail: &detail,
                })
                .await;
            if let Err(e) = res {
                eprintln!("hopr: failed to write audit event: {e}");
            }
        });
    }

    pub fn clipboard_clear_secs(&self) -> u32 {
        self.clipboard_clear_secs.load(Ordering::Relaxed)
    }

    pub fn set_clipboard_clear_secs(&self, secs: u32) {
        self.clipboard_clear_secs.store(secs, Ordering::Relaxed);
    }

    /// The unlocked vault, or `VaultLocked`. Counts as activity.
    pub fn vault(&self) -> Result<Arc<UnlockedVault>, AppError> {
        let v = self.vault.read().ok().and_then(|g| g.clone());
        let v =
            v.ok_or_else(|| AppError::new(ErrorCode::VaultLocked, "Unlock Hopr to continue"))?;
        self.touch();
        Ok(v)
    }

    /// Fail unless unlocked (for commands that don't need the key itself).
    pub fn require_unlocked(&self) -> Result<(), AppError> {
        self.vault().map(|_| ())
    }

    pub fn is_unlocked(&self) -> bool {
        self.vault.read().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn set_vault(&self, vault: UnlockedVault) {
        if let Ok(mut g) = self.vault.write() {
            *g = Some(Arc::new(vault));
        }
        self.touch();
    }

    /// Lock: drop our reference so the DEK is zeroized once in-flight users finish.
    pub fn lock(&self) {
        if let Ok(mut g) = self.vault.write() {
            *g = None;
        }
    }

    pub fn touch(&self) {
        self.last_activity.store(now_secs(), Ordering::Relaxed);
    }

    pub fn auto_lock_minutes(&self) -> u32 {
        self.auto_lock_minutes.load(Ordering::Relaxed)
    }

    pub fn set_auto_lock_minutes(&self, minutes: u32) {
        self.auto_lock_minutes.store(minutes, Ordering::Relaxed);
    }

    fn idle_expired(&self) -> bool {
        let minutes = self.auto_lock_minutes();
        minutes > 0
            && now_secs().saturating_sub(self.last_activity.load(Ordering::Relaxed))
                >= u64::from(minutes) * 60
    }
}

/// Lock the vault after the configured idle time.
pub fn spawn_auto_lock(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        loop {
            tick.tick().await;
            if state.is_unlocked() && state.idle_expired() {
                state.lock();
                state.audit("vault.lock", None, "Locked after inactivity");
                let _ = VaultLockedEvent {
                    reason: "idle".into(),
                }
                .emit(&app);
            }
        }
    });
}

/// "Remember on this device": the DEK in the OS keychain (Credential Manager, Keychain,
/// Secret Service). Never written to disk by Hopr.
pub mod keychain {
    use data_encoding::BASE64;
    use zeroize::Zeroizing;

    const SERVICE: &str = "dev.hopr.app";
    const USER: &str = "vault-dek";

    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, USER).ok()
    }

    pub fn store(dek: &[u8]) -> bool {
        let encoded = Zeroizing::new(BASE64.encode(dek));
        entry().is_some_and(|e| e.set_password(&encoded).is_ok())
    }

    pub fn load() -> Option<Zeroizing<Vec<u8>>> {
        let text = Zeroizing::new(entry()?.get_password().ok()?);
        BASE64.decode(text.as_bytes()).ok().map(Zeroizing::new)
    }

    pub fn exists() -> bool {
        entry().is_some_and(|e| e.get_password().is_ok())
    }

    pub fn forget() {
        if let Some(e) = entry() {
            let _ = e.delete_credential();
        }
    }
}
