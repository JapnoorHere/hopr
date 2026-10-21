//! Local audit log (X7): unlocks, connects, auth failures, host-key decisions, secret
//! copies, deploys, exports and security setting changes. Never stores a secret.

use hopr_core::model::AuditEvent;
use sqlx::FromRow;

use crate::{Result, Store, new_id, now};

/// Events kept; older ones are pruned when new ones are written.
pub const AUDIT_KEEP: i64 = 10_000;

/// A new audit entry.
pub struct NewAudit<'a> {
    /// What happened, e.g. `vault.unlock`.
    pub kind: &'a str,
    /// Host involved.
    pub host_id: Option<&'a str>,
    /// Host label at the time.
    pub host_label: Option<&'a str>,
    /// Detail text (no secrets).
    pub detail: &'a str,
}

#[derive(FromRow)]
struct Row {
    id: String,
    at: String,
    kind: String,
    host_id: Option<String>,
    host_label: Option<String>,
    detail: String,
}

impl Store {
    /// Append an event and prune beyond [`AUDIT_KEEP`].
    pub async fn audit(&self, e: NewAudit<'_>) -> Result<()> {
        let mut detail = e.detail.to_owned();
        detail.truncate(1000);
        sqlx::query(
            "INSERT INTO audit_events (id, at, kind, host_id, host_label, detail) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(now())
        .bind(e.kind)
        .bind(e.host_id)
        .bind(e.host_label)
        .bind(detail)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "DELETE FROM audit_events WHERE id NOT IN
               (SELECT id FROM audit_events ORDER BY id DESC LIMIT ?)",
        )
        .bind(AUDIT_KEEP)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Newest first, at most `limit`.
    pub async fn audit_list(&self, limit: u32) -> Result<Vec<AuditEvent>> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, at, kind, host_id, host_label, detail FROM audit_events ORDER BY id DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AuditEvent {
                id: r.id,
                at: r.at,
                kind: r.kind,
                host_id: r.host_id,
                host_label: r.host_label,
                detail: r.detail,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::store_and_vault;

    use super::*;

    #[tokio::test]
    async fn records_newest_first() {
        let (store, _vault) = store_and_vault().await;
        for kind in ["vault.unlock", "session.connect", "host.copy_password"] {
            store
                .audit(NewAudit {
                    kind,
                    host_id: Some("h1"),
                    host_label: Some("prod-db"),
                    detail: "",
                })
                .await
                .unwrap();
        }
        let list = store.audit_list(2).await.unwrap();
        let kinds: Vec<_> = list.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, vec!["host.copy_password", "session.connect"]);
        assert_eq!(list[0].host_label.as_deref(), Some("prod-db"));
    }
}
