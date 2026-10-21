//! Trusted host keys (TOFU store). Matching, including OpenSSH hashed patterns, is done
//! by hopr-ssh; this module returns candidates and records decisions.

use hopr_core::model::KnownHost;
use sqlx::FromRow;

use crate::{Result, Store, new_id, now};

#[derive(FromRow)]
struct KnownRow {
    id: String,
    host_pattern: String,
    port: i64,
    key_type: String,
    key_b64: String,
    fingerprint: String,
    first_seen_at: String,
    last_seen_at: String,
    source: String,
}

impl From<KnownRow> for KnownHost {
    fn from(r: KnownRow) -> Self {
        KnownHost {
            id: r.id,
            host_pattern: r.host_pattern,
            port: u16::try_from(r.port).unwrap_or(0),
            key_type: r.key_type,
            key_b64: r.key_b64,
            fingerprint: r.fingerprint,
            first_seen_at: r.first_seen_at,
            last_seen_at: r.last_seen_at,
            source: r.source,
        }
    }
}

/// A key to trust.
pub struct TrustKey<'a> {
    /// Host as typed (plain) or an imported pattern.
    pub host_pattern: &'a str,
    /// Port (0 = encoded in a hashed pattern).
    pub port: u16,
    /// Algorithm.
    pub key_type: &'a str,
    /// Base64 blob.
    pub key_b64: &'a str,
    /// "SHA256:…".
    pub fingerprint: &'a str,
    /// "tofu" | "imported" | "manual".
    pub source: &'a str,
}

const KNOWN_COLS: &str =
    "id, host_pattern, port, key_type, key_b64, fingerprint, first_seen_at, last_seen_at, source";

impl Store {
    /// Entries that could match `host:port`: exact plain entries plus all hashed ones.
    pub async fn known_host_candidates(&self, host: &str, port: u16) -> Result<Vec<KnownHost>> {
        let rows: Vec<KnownRow> = sqlx::query_as(&format!(
            "SELECT {KNOWN_COLS} FROM known_hosts
             WHERE (host_pattern = ? AND port = ?) OR host_pattern LIKE '|1|%'"
        ))
        .bind(host.to_ascii_lowercase())
        .bind(i64::from(port))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Trust a key (idempotent).
    pub async fn trust_key(&self, k: TrustKey<'_>) -> Result<()> {
        let ts = now();
        sqlx::query(
            "INSERT INTO known_hosts (id, host_pattern, port, key_type, key_b64, fingerprint, first_seen_at,
                                      last_seen_at, source)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(host_pattern, port, key_type, key_b64) DO UPDATE SET last_seen_at = excluded.last_seen_at",
        )
        .bind(new_id())
        .bind(k.host_pattern.to_ascii_lowercase())
        .bind(i64::from(k.port))
        .bind(k.key_type)
        .bind(k.key_b64)
        .bind(k.fingerprint)
        .bind(&ts)
        .bind(&ts)
        .bind(k.source)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Replace every plain entry for `host:port` with one new key (accepted key change).
    pub async fn replace_host_key(&self, k: TrustKey<'_>) -> Result<()> {
        sqlx::query("DELETE FROM known_hosts WHERE host_pattern = ? AND port = ?")
            .bind(k.host_pattern.to_ascii_lowercase())
            .bind(i64::from(k.port))
            .execute(&self.pool)
            .await?;
        self.trust_key(k).await
    }

    /// Everything, for the Known hosts screen.
    pub async fn list_known_hosts(&self) -> Result<Vec<KnownHost>> {
        let rows: Vec<KnownRow> = sqlx::query_as(&format!(
            "SELECT {KNOWN_COLS} FROM known_hosts ORDER BY host_pattern, port"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Forget one entry.
    pub async fn delete_known_host(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM known_hosts WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store_and_vault;

    fn tk<'a>(host: &'a str, key: &'a str) -> TrustKey<'a> {
        TrustKey {
            host_pattern: host,
            port: 22,
            key_type: "ssh-ed25519",
            key_b64: key,
            fingerprint: "SHA256:x",
            source: "tofu",
        }
    }

    #[tokio::test]
    async fn trust_lookup_replace() {
        let (store, _) = store_and_vault().await;
        store.trust_key(tk("Prod-DB", "AAA")).await.unwrap();
        store.trust_key(tk("prod-db", "AAA")).await.unwrap(); // idempotent, case-insensitive
        store.trust_key(tk("|1|salt|hash", "BBB")).await.unwrap();
        let c = store.known_host_candidates("prod-db", 22).await.unwrap();
        assert_eq!(c.len(), 2, "plain match + hashed candidate");
        assert!(
            store
                .known_host_candidates("prod-db", 2222)
                .await
                .unwrap()
                .iter()
                .all(|k| k.host_pattern.starts_with("|1|"))
        );

        store.replace_host_key(tk("prod-db", "CCC")).await.unwrap();
        let plain: Vec<_> = store
            .known_host_candidates("prod-db", 22)
            .await
            .unwrap()
            .into_iter()
            .filter(|k| !k.host_pattern.starts_with("|1|"))
            .collect();
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].key_b64, "CCC");

        let id = plain[0].id.clone();
        store.delete_known_host(&id).await.unwrap();
        assert_eq!(store.list_known_hosts().await.unwrap().len(), 1);
    }
}
