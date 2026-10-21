//! Stored private keys. The private half is encrypted with the vault; key parsing and
//! generation live in hopr-ssh, so this module only stores what it is given.

use hopr_core::model::SshKeyInfo;
use hopr_vault::UnlockedVault;
use secrecy::{ExposeSecret, SecretString};
use sqlx::FromRow;

use crate::{Result, Store, StoreError, new_id, now};

pub(crate) fn aad_private(id: &str) -> Vec<u8> {
    format!("key.private:{id}").into_bytes()
}

/// A key to store: public metadata plus the unencrypted OpenSSH private key.
pub struct NewKey {
    /// Display name.
    pub label: String,
    /// e.g. "ssh-ed25519".
    pub algorithm: String,
    /// "SHA256:…".
    pub fingerprint: String,
    /// OpenSSH public key line.
    pub public_key: String,
    /// Comment.
    pub comment: String,
    /// OpenSSH private key (unencrypted; encrypted by the vault here).
    pub private_openssh: SecretString,
}

#[derive(FromRow)]
struct KeyRow {
    id: String,
    label: String,
    algorithm: String,
    fingerprint: String,
    public_key: String,
    comment: String,
    created_at: String,
    used_by: i64,
    certificate: Option<String>,
}

impl Store {
    /// All keys with usage counts.
    pub async fn list_keys(&self) -> Result<Vec<SshKeyInfo>> {
        let rows: Vec<KeyRow> = sqlx::query_as(
            "SELECT k.id, k.label, k.algorithm, k.fingerprint, k.public_key, k.comment, k.created_at, k.certificate,
                    (SELECT COUNT(*) FROM hosts h WHERE h.key_id = k.id AND h.deleted_at IS NULL) AS used_by
             FROM ssh_keys k ORDER BY k.label COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SshKeyInfo {
                id: r.id,
                label: r.label,
                algorithm: r.algorithm,
                fingerprint: r.fingerprint,
                public_key: r.public_key,
                comment: r.comment,
                used_by: u32::try_from(r.used_by).unwrap_or(0),
                created_at: r.created_at,
                certificate: r.certificate,
                certificate_info: None,
            })
            .collect())
    }

    /// Store a key. Rejects duplicates by fingerprint.
    pub async fn add_key(&self, vault: &UnlockedVault, key: NewKey) -> Result<SshKeyInfo> {
        let dup: Option<(String,)> =
            sqlx::query_as("SELECT label FROM ssh_keys WHERE fingerprint = ?")
                .bind(&key.fingerprint)
                .fetch_optional(&self.pool)
                .await?;
        if let Some((label,)) = dup {
            return Err(StoreError::InUse(format!(
                "This key is already saved as \"{label}\""
            )));
        }
        let id = new_id();
        let ts = now();
        let enc = vault.encrypt(
            &aad_private(&id),
            key.private_openssh.expose_secret().as_bytes(),
        )?;
        sqlx::query(
            "INSERT INTO ssh_keys (id, label, algorithm, fingerprint, public_key, comment, private_key_enc,
                                   created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&key.label)
        .bind(&key.algorithm)
        .bind(&key.fingerprint)
        .bind(&key.public_key)
        .bind(&key.comment)
        .bind(enc)
        .bind(&ts)
        .bind(&ts)
        .execute(&self.pool)
        .await?;
        Ok(SshKeyInfo {
            id,
            label: key.label,
            algorithm: key.algorithm,
            fingerprint: key.fingerprint,
            public_key: key.public_key,
            comment: key.comment,
            used_by: 0,
            created_at: ts,
            certificate: None,
            certificate_info: None,
        })
    }

    /// Attach (or with `None`, remove) an OpenSSH certificate. The caller checks it
    /// belongs to the key.
    pub async fn set_key_certificate(&self, id: &str, certificate: Option<&str>) -> Result<()> {
        let n = sqlx::query("UPDATE ssh_keys SET certificate = ?, updated_at = ? WHERE id = ?")
            .bind(certificate)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(StoreError::NotFound("key"));
        }
        Ok(())
    }

    /// The certificate attached to a key, if any.
    pub async fn key_certificate(&self, id: &str) -> Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT certificate FROM ssh_keys WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(c,)| c))
    }

    /// Rename a key.
    pub async fn rename_key(&self, id: &str, label: &str) -> Result<()> {
        sqlx::query("UPDATE ssh_keys SET label = ?, updated_at = ? WHERE id = ?")
            .bind(label.trim())
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete a key, refusing while hosts still use it.
    pub async fn delete_key(&self, id: &str) -> Result<()> {
        let (used,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM hosts WHERE key_id = ? AND deleted_at IS NULL")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        if used > 0 {
            return Err(StoreError::InUse(format!(
                "{used} host(s) still use this key. Switch them to another key first."
            )));
        }
        // Soft-deleted hosts may still reference it; they go too.
        sqlx::query("DELETE FROM hosts WHERE key_id = ? AND deleted_at IS NOT NULL")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM ssh_keys WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Decrypt a private key (OpenSSH format) for one connection.
    pub async fn key_private(&self, vault: &UnlockedVault, id: &str) -> Result<SecretString> {
        let row: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT private_key_enc FROM ssh_keys WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let (enc,) = row.ok_or(StoreError::NotFound("key"))?;
        Ok(vault.decrypt_string(&aad_private(id), &enc)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store_and_vault;
    use hopr_core::model::{AuthKind, Environment, HostInput, SecretChange};

    fn key(fp: &str) -> NewKey {
        NewKey {
            label: "work".into(),
            algorithm: "ssh-ed25519".into(),
            fingerprint: fp.into(),
            public_key: "ssh-ed25519 AAAA test".into(),
            comment: "test".into(),
            private_openssh: SecretString::from(
                "-----BEGIN OPENSSH PRIVATE KEY-----\nsecret\n".to_owned(),
            ),
        }
    }

    #[tokio::test]
    async fn add_list_decrypt_and_guard_delete() {
        let (store, vault) = store_and_vault().await;
        let k = store.add_key(&vault, key("SHA256:abc")).await.unwrap();
        assert!(
            store.add_key(&vault, key("SHA256:abc")).await.is_err(),
            "duplicate fingerprint"
        );
        assert!(
            store
                .key_private(&vault, &k.id)
                .await
                .unwrap()
                .expose_secret()
                .contains("secret")
        );

        let h = store
            .create_host(
                &vault,
                HostInput {
                    protocol: Default::default(),
                    serial: None,
                    proxy_command: None,
                    label: String::new(),
                    hostname: "h".into(),
                    port: 22,
                    username: "u".into(),
                    group_id: None,
                    tags: vec![],
                    auth: AuthKind::Key,
                    key_id: Some(k.id.clone()),
                    jump_host_id: None,
                    agent_forwarding: false,
                    proxy: None,
                    proxy_password: SecretChange::Keep,
                    favorite: false,
                    environment: Environment::None,
                    notes: String::new(),
                    password: SecretChange::Keep,
                    totp_seed: SecretChange::Keep,
                },
            )
            .await
            .unwrap();
        assert_eq!(store.list_keys().await.unwrap()[0].used_by, 1);
        assert!(matches!(
            store.delete_key(&k.id).await,
            Err(StoreError::InUse(_))
        ));
        store.delete_host(&h.id).await.unwrap();
        store.delete_key(&k.id).await.unwrap();
        assert!(store.list_keys().await.unwrap().is_empty());
    }
}
