//! SQLite persistence (ADR 0004). Repository methods that touch secrets take an
//! `&UnlockedVault` and encrypt/decrypt at this boundary, so callers never handle
//! ciphertext and the database never sees plaintext.

#![warn(missing_docs)]

mod audit;
pub mod backup;
pub mod history;
mod hosts;
mod keys;
mod known;
pub mod snippets;
pub mod sync;
mod tunnels;
mod workspaces;

use std::path::Path;
use std::str::FromStr;

use hopr_core::{AppError, ErrorCode};
use hopr_vault::{VaultError, VaultHeader};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

pub use audit::{AUDIT_KEEP, NewAudit};
pub use history::NewCommand;
pub use hosts::HostCredentials;
pub use keys::NewKey;
pub use known::TrustKey;

/// Storage failures.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite error.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    /// Migration failed.
    #[error("migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// Encryption or decryption failed.
    #[error(transparent)]
    Vault(#[from] VaultError),
    /// JSON (de)serialization failed.
    #[error("bad stored value: {0}")]
    Json(#[from] serde_json::Error),
    /// File system error (backup).
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    /// Row does not exist.
    #[error("{0} not found")]
    NotFound(&'static str),
    /// Row is referenced elsewhere.
    #[error("{0}")]
    InUse(String),
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound(what) => {
                AppError::invalid(format!("That {what} no longer exists"))
            }
            StoreError::InUse(msg) => AppError::invalid(msg),
            StoreError::Vault(VaultError::WrongPassword) => AppError::invalid("Incorrect password"),
            other => AppError::new(
                ErrorCode::Storage,
                "Couldn't read or write the Hopr database",
            )
            .with_detail(other.to_string()),
        }
    }
}

/// Result alias.
pub type Result<T, E = StoreError> = std::result::Result<T, E>;

pub(crate) fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub(crate) fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Handle to the Hopr database.
#[derive(Clone)]
pub struct Store {
    pub(crate) pool: SqlitePool,
}

impl Store {
    /// Open (or create) the database at `path`, back it up, and run migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        if path.exists() {
            // Cheap insurance before migrations or any write (HOPR_PLAN §5).
            std::fs::copy(path, path.with_extension("db.bak"))?;
        } else if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;
        Self::migrate(pool).await
    }

    /// In-memory database (tests).
    pub async fn open_memory() -> Result<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // One connection: every in-memory connection would otherwise be its own database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        Self::migrate(pool).await
    }

    async fn migrate(pool: SqlitePool) -> Result<Self> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// The vault header, if a vault has been created.
    pub async fn vault_header(&self) -> Result<Option<VaultHeader>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT header_json FROM vault_meta WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(json,)| serde_json::from_str(&json).map_err(StoreError::from))
            .transpose()
    }

    /// Create or replace the vault header (password change re-wraps the DEK).
    pub async fn save_vault_header(&self, header: &VaultHeader) -> Result<()> {
        let json = serde_json::to_string(header)?;
        let ts = now();
        sqlx::query(
            "INSERT INTO vault_meta (id, header_json, created_at, updated_at) VALUES (1, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET header_json = excluded.header_json, updated_at = excluded.updated_at",
        )
        .bind(json)
        .bind(&ts)
        .bind(&ts)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Read a non-secret setting.
    pub async fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value_json FROM app_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(json,)| serde_json::from_str(&json).map_err(StoreError::from))
            .transpose()
    }

    /// Remove a setting.
    pub async fn delete_setting(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM app_settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Write a non-secret setting.
    pub async fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_settings (key, value_json) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        )
        .bind(key)
        .bind(serde_json::to_string(value)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use hopr_vault::{KdfParams, UnlockedVault};
    use secrecy::SecretString;

    pub async fn store_and_vault() -> (super::Store, UnlockedVault) {
        let store = super::Store::open_memory().await.unwrap();
        let nv = hopr_vault::create(&SecretString::from("pw".to_owned()), KdfParams::TEST).unwrap();
        store.save_vault_header(&nv.header).await.unwrap();
        (store, nv.vault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn vault_header_and_settings_roundtrip() {
        let (store, _vault) = test_util::store_and_vault().await;
        assert!(store.vault_header().await.unwrap().is_some());
        assert_eq!(store.setting::<u32>("auto_lock").await.unwrap(), None);
        store.set_setting("auto_lock", &15u32).await.unwrap();
        store.set_setting("auto_lock", &30u32).await.unwrap();
        assert_eq!(store.setting::<u32>("auto_lock").await.unwrap(), Some(30));
    }

    #[tokio::test]
    async fn file_database_persists_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("hopr-store-test-{}", new_id()));
        let path = dir.join("hopr.db");
        {
            let s = Store::open(&path).await.unwrap();
            s.set_setting("k", &"v").await.unwrap();
            s.pool.close().await;
        }
        let s = Store::open(&path).await.unwrap();
        assert_eq!(
            s.setting::<String>("k").await.unwrap().as_deref(),
            Some("v")
        );
        assert!(path.with_extension("db.bak").exists());
        s.pool.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
