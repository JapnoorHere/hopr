//! Saved port forwards (§9.11). Deleting a host deletes its tunnels.

use hopr_core::model::{Tunnel, TunnelInput, TunnelKind};
use sqlx::FromRow;

use crate::{Result, Store, StoreError, new_id, now};

#[derive(FromRow)]
struct Row {
    id: String,
    host_id: String,
    label: String,
    kind: String,
    bind_addr: String,
    bind_port: i64,
    dest_host: String,
    dest_port: i64,
    auto_start: bool,
}

impl Row {
    fn into_tunnel(self) -> Option<Tunnel> {
        Some(Tunnel {
            id: self.id,
            host_id: self.host_id,
            label: self.label,
            kind: TunnelKind::parse(&self.kind)?,
            bind_addr: self.bind_addr,
            bind_port: u16::try_from(self.bind_port).unwrap_or(0),
            dest_host: self.dest_host,
            dest_port: u16::try_from(self.dest_port).unwrap_or(0),
            auto_start: self.auto_start,
        })
    }
}

const SELECT: &str = "SELECT t.id, t.host_id, t.label, t.kind, t.bind_addr, t.bind_port, t.dest_host, t.dest_port, t.auto_start
     FROM tunnels t JOIN hosts h ON h.id = t.host_id AND h.deleted_at IS NULL";

impl Store {
    /// Every tunnel of a live host, by host then port.
    pub async fn list_tunnels(&self) -> Result<Vec<Tunnel>> {
        let rows: Vec<Row> = sqlx::query_as(&format!(
            "{SELECT} ORDER BY h.label COLLATE NOCASE, t.bind_port"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().filter_map(Row::into_tunnel).collect())
    }

    /// One tunnel.
    pub async fn get_tunnel(&self, id: &str) -> Result<Tunnel> {
        let row: Option<Row> = sqlx::query_as(&format!("{SELECT} WHERE t.id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.and_then(Row::into_tunnel)
            .ok_or(StoreError::NotFound("tunnel"))
    }

    /// Create (no `id`) or update a tunnel. The input must be normalized.
    pub async fn save_tunnel(&self, id: Option<&str>, t: &TunnelInput) -> Result<Tunnel> {
        let ts = now();
        let id = match id {
            Some(id) => {
                let n = sqlx::query(
                    "UPDATE tunnels SET host_id = ?, label = ?, kind = ?, bind_addr = ?, bind_port = ?,
                            dest_host = ?, dest_port = ?, auto_start = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&t.host_id)
                .bind(&t.label)
                .bind(t.kind.as_str())
                .bind(&t.bind_addr)
                .bind(i64::from(t.bind_port))
                .bind(&t.dest_host)
                .bind(i64::from(t.dest_port))
                .bind(t.auto_start)
                .bind(&ts)
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected();
                if n == 0 {
                    return Err(StoreError::NotFound("tunnel"));
                }
                id.to_owned()
            }
            None => {
                let id = new_id();
                sqlx::query(
                    "INSERT INTO tunnels (id, host_id, label, kind, bind_addr, bind_port, dest_host, dest_port, auto_start, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&t.host_id)
                .bind(&t.label)
                .bind(t.kind.as_str())
                .bind(&t.bind_addr)
                .bind(i64::from(t.bind_port))
                .bind(&t.dest_host)
                .bind(i64::from(t.dest_port))
                .bind(t.auto_start)
                .bind(&ts)
                .bind(&ts)
                .execute(&self.pool)
                .await?;
                id
            }
        };
        self.get_tunnel(&id).await
    }

    /// Delete a tunnel.
    pub async fn delete_tunnel(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tunnels WHERE id = ?")
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
    use hopr_core::model::{AuthKind, Environment, HostInput, SecretChange};

    async fn host(store: &Store, vault: &hopr_vault::UnlockedVault) -> String {
        let input = HostInput {
            label: "db".into(),
            protocol: Default::default(),
            hostname: "db.corp".into(),
            port: 22,
            serial: None,
            proxy_command: None,
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
            password: SecretChange::Keep,
            totp_seed: SecretChange::Keep,
        };
        store.create_host(vault, input).await.unwrap().id
    }

    #[tokio::test]
    async fn tunnels_roundtrip_and_hide_with_their_host() {
        let (store, vault) = store_and_vault().await;
        let h = host(&store, &vault).await;
        let input = TunnelInput {
            host_id: h.clone(),
            label: "pg".into(),
            kind: TunnelKind::Local,
            bind_addr: "127.0.0.1".into(),
            bind_port: 15432,
            dest_host: "localhost".into(),
            dest_port: 5432,
            auto_start: true,
        };
        let t = store.save_tunnel(None, &input).await.unwrap();
        assert_eq!(t.kind, TunnelKind::Local);
        assert!(t.auto_start);
        let socks = TunnelInput {
            kind: TunnelKind::Dynamic,
            bind_port: 1080,
            ..input.clone()
        };
        let d = store.save_tunnel(None, &socks).await.unwrap();
        let updated = store
            .save_tunnel(
                Some(&t.id),
                &TunnelInput {
                    bind_port: 25432,
                    ..input.clone()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.bind_port, 25432);
        let list = store.list_tunnels().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, d.id, "sorted by port");

        store.delete_tunnel(&d.id).await.unwrap();
        assert_eq!(store.list_tunnels().await.unwrap().len(), 1);
        // A host in the trash hides its tunnels.
        store.delete_host(&h).await.unwrap();
        assert!(store.list_tunnels().await.unwrap().is_empty());
    }

    #[test]
    fn input_rules() {
        let base = TunnelInput {
            host_id: "h".into(),
            label: " web ".into(),
            kind: TunnelKind::Local,
            bind_addr: " ".into(),
            bind_port: 8080,
            dest_host: " app ".into(),
            dest_port: 80,
            auto_start: false,
        };
        let n = base.clone().normalized().unwrap();
        assert_eq!(
            (n.label.as_str(), n.bind_addr.as_str(), n.dest_host.as_str()),
            ("web", "127.0.0.1", "app")
        );
        assert!(
            TunnelInput {
                dest_port: 0,
                ..base.clone()
            }
            .normalized()
            .is_err()
        );
        assert!(
            TunnelInput {
                dest_host: String::new(),
                ..base.clone()
            }
            .normalized()
            .is_err()
        );
        let d = TunnelInput {
            kind: TunnelKind::Dynamic,
            dest_host: String::new(),
            dest_port: 0,
            ..base
        }
        .normalized()
        .unwrap();
        assert!(d.dest_host.is_empty());
    }
}
