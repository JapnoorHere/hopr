//! Encrypted backups (`.hopr`). A backup is the whole database (a `VACUUM INTO` copy)
//! sealed with the vault key, plus the vault header, so it opens on any machine with the
//! master password it was made under, and is unreadable without it.
//!
//! Layout: `HOPRBAK1\n`, one line of JSON ([`BackupHeader`]), `\n`, then the ciphertext.

use std::path::{Path, PathBuf};

use hopr_vault::{UnlockedVault, VaultHeader};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Result, Store, StoreError, new_id};

const MAGIC: &[u8] = b"HOPRBAK1\n";
const AAD: &[u8] = b"hopr.backup.v1";
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// Plaintext part of a backup file. Holds no secret (the vault header only wraps the key).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupHeader {
    /// Format version.
    pub version: u32,
    /// When it was made (RFC 3339).
    pub created_at: String,
    /// Hopr version that made it.
    pub app_version: String,
    /// The vault header at backup time (wraps the key that seals the body).
    pub vault: VaultHeader,
}

fn bad(msg: &str) -> StoreError {
    StoreError::InUse(msg.to_owned())
}

impl Store {
    /// A consistent copy of the whole database, as bytes.
    pub async fn snapshot_bytes(&self) -> Result<Vec<u8>> {
        let tmp = std::env::temp_dir().join(format!("hopr-snapshot-{}.db", new_id()));
        let path = tmp.to_string_lossy().replace('\\', "/").replace('\'', "''");
        sqlx::query(&format!("VACUUM INTO '{path}'"))
            .execute(&self.pool)
            .await?;
        let bytes = std::fs::read(&tmp);
        let _ = std::fs::remove_file(&tmp);
        Ok(bytes?)
    }
}

/// Seal database bytes into a backup file.
pub fn seal(vault: &UnlockedVault, header: &BackupHeader, db: &[u8]) -> Result<Vec<u8>> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(serde_json::to_string(header)?.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(&vault.encrypt(AAD, db)?);
    Ok(out)
}

/// Read a backup's header without decrypting it.
pub fn read_header(file: &[u8]) -> Result<(BackupHeader, &[u8])> {
    let rest = file
        .strip_prefix(MAGIC)
        .ok_or_else(|| bad("That isn't a Hopr backup (.hopr) file"))?;
    let nl = rest
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| bad("The backup file is damaged"))?;
    let header: BackupHeader =
        serde_json::from_slice(&rest[..nl]).map_err(|_| bad("The backup file is damaged"))?;
    if header.version != 1 {
        return Err(bad("This backup was made by a newer Hopr"));
    }
    Ok((header, &rest[nl + 1..]))
}

/// Decrypt a backup with the master password it was made under.
pub fn open(file: &[u8], password: &SecretString) -> Result<(BackupHeader, Zeroizing<Vec<u8>>)> {
    let (header, body) = read_header(file)?;
    let vault = hopr_vault::unlock(&header.vault, password)?;
    let db = vault
        .decrypt(AAD, body)
        .map_err(|_| bad("The backup file is damaged"))?;
    if !db.starts_with(SQLITE_MAGIC) {
        return Err(bad("The backup file is damaged"));
    }
    Ok((header, db))
}

/// Where a restore waits for the next start.
pub fn pending_path(db_path: &Path) -> PathBuf {
    db_path.with_extension("db.restore")
}

/// Stage database bytes to replace the current database at the next start.
pub fn stage_restore(db_path: &Path, db: &[u8]) -> Result<()> {
    let pending = pending_path(db_path);
    let tmp = pending.with_extension("restore.part");
    std::fs::write(&tmp, db)?;
    std::fs::rename(&tmp, &pending)?;
    Ok(())
}

/// Before the database opens: swap in a staged restore, keeping the replaced database as
/// `hopr.db.pre-restore`. Returns whether a restore was applied.
pub fn apply_pending_restore(db_path: &Path) -> Result<bool> {
    let pending = pending_path(db_path);
    if !pending.exists() {
        return Ok(false);
    }
    if db_path.exists() {
        std::fs::copy(db_path, db_path.with_extension("db.pre-restore"))?;
    }
    // The old WAL/SHM belong to the old database and must not be replayed onto the new one.
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(db_path.with_extension(ext));
    }
    std::fs::rename(&pending, db_path)?;
    Ok(true)
}

/// Backup files in `dir`, newest first.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "hopr"))
                .collect()
        })
        .unwrap_or_default();
    // Names end in a sortable timestamp; sort on that, not the auto-/manual- prefix.
    files.sort_by_key(|p| {
        p.file_stem()
            .map(|s| {
                let s = s.to_string_lossy();
                s.split_once('-')
                    .map(|(_, t)| t.to_owned())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    });
    files.reverse();
    files
}

/// Delete all but the newest `keep` automatic backups (`auto-*.hopr`).
pub fn prune(dir: &Path, keep: usize) {
    let autos: Vec<PathBuf> = list(dir)
        .into_iter()
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("auto-"))
        })
        .collect();
    for old in autos.into_iter().skip(keep) {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use hopr_core::model::{AuthKind, Environment, HostInput, SecretChange};

    use super::*;
    use crate::test_util::store_and_vault;

    fn host(name: &str) -> HostInput {
        HostInput {
            protocol: Default::default(),
            serial: None,
            proxy_command: None,
            label: name.into(),
            hostname: name.into(),
            port: 22,
            username: "u".into(),
            group_id: None,
            tags: vec![],
            auth: AuthKind::Password,
            key_id: None,
            jump_host_id: None,
            agent_forwarding: false,
            proxy: None,
            proxy_password: SecretChange::Keep,
            favorite: false,
            environment: Environment::None,
            notes: String::new(),
            password: SecretChange::Set {
                value: "s3cret-pw".into(),
            },
            totp_seed: SecretChange::Keep,
        }
    }

    #[tokio::test]
    async fn backup_roundtrips_and_needs_the_master_password() {
        // File-backed: an in-memory database would VACUUM INTO memory as well.
        let (mem, vault) = store_and_vault().await;
        let src_dir = std::env::temp_dir().join(format!("hopr-backup-src-{}", new_id()));
        let store = Store::open(&src_dir.join("hopr.db")).await.unwrap();
        let nv_header = mem.vault_header().await.unwrap().unwrap();
        store.save_vault_header(&nv_header).await.unwrap();
        store.create_host(&vault, host("db-1")).await.unwrap();
        let header = BackupHeader {
            version: 1,
            created_at: "2026-10-17T00:00:00Z".into(),
            app_version: "0.1.0".into(),
            vault: store.vault_header().await.unwrap().unwrap(),
        };
        let file = seal(&vault, &header, &store.snapshot_bytes().await.unwrap()).unwrap();
        assert!(
            !file.windows(4).any(|w| w == b"db-1"),
            "host names are encrypted too"
        );

        assert!(open(&file, &SecretString::from("wrong".to_owned())).is_err());
        let (_, db) = open(&file, &SecretString::from("pw".to_owned())).unwrap();

        // The decrypted bytes are a working database with the host in it.
        let dir = std::env::temp_dir().join(format!("hopr-restore-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hopr.db");
        stage_restore(&path, &db).unwrap();
        assert!(apply_pending_restore(&path).unwrap());
        assert!(!apply_pending_restore(&path).unwrap(), "applied once");
        let restored = Store::open(&path).await.unwrap();
        let hosts = restored.list_hosts().await.unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].label, "db-1");
        let c = restored
            .host_credentials(&vault, &hosts[0].id)
            .await
            .unwrap();
        assert!(c.password.is_some());
        restored.pool.close().await;
        store.pool.close().await;
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(src_dir);
    }

    #[test]
    fn rejects_foreign_and_tampered_files() {
        assert!(read_header(b"PK\x03\x04zip").is_err());
        let mut file = MAGIC.to_vec();
        file.extend_from_slice(b"{not json}\n");
        assert!(read_header(&file).is_err());
    }

    #[test]
    fn prune_keeps_the_newest_automatic_backups() {
        let dir = std::env::temp_dir().join(format!("hopr-prune-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        for day in 1..=9 {
            std::fs::write(dir.join(format!("auto-2026101{day}-000000.hopr")), b"x").unwrap();
        }
        std::fs::write(dir.join("manual-20260101-000000.hopr"), b"x").unwrap();
        prune(&dir, 7);
        let left: Vec<String> = list(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 8, "{left:?}");
        assert_eq!(left[0], "auto-20261019-000000.hopr", "newest first");
        assert!(left.contains(&"manual-20260101-000000.hopr".to_owned()));
        assert!(!left.contains(&"auto-20261011-000000.hopr".to_owned()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
