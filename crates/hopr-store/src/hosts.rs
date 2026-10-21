//! Hosts, groups and host secrets.

use std::collections::HashMap;

use hopr_core::model::{
    AuthKind, BulkEdit, Environment, Group, Host, HostInput, HostProxy, Protocol, ProxyProtocol,
    SecretChange, SerialConfig, normalize_tags,
};
use hopr_vault::UnlockedVault;
use secrecy::SecretString;
use sqlx::FromRow;

use crate::{Result, Store, StoreError, new_id, now};

/// Longest jump chain, counting the target (HOPR_PLAN C6 needs 3; 5 leaves headroom).
pub const MAX_HOPS: usize = 5;

pub(crate) fn aad_password(id: &str) -> Vec<u8> {
    format!("host.password:{id}").into_bytes()
}

pub(crate) fn aad_totp(id: &str) -> Vec<u8> {
    format!("host.totp:{id}").into_bytes()
}

pub(crate) fn aad_proxy(id: &str) -> Vec<u8> {
    format!("host.proxy_password:{id}").into_bytes()
}

/// `(password_enc, totp_seed_enc, proxy_password_enc)` ciphertext columns.
type SecretColumns = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

/// Decrypted credentials for one connection attempt. Dropped (and zeroized) after use.
pub struct HostCredentials {
    /// Saved password.
    pub password: Option<SecretString>,
    /// Saved TOTP seed (base32).
    pub totp_seed: Option<SecretString>,
    /// Saved proxy password.
    pub proxy_password: Option<SecretString>,
}

#[derive(FromRow)]
struct HostRow {
    id: String,
    label: String,
    protocol: String,
    hostname: String,
    serial_json: Option<String>,
    proxy_command: Option<String>,
    port: i64,
    username: String,
    group_id: Option<String>,
    auth: String,
    key_id: Option<String>,
    jump_host_id: Option<String>,
    agent_forwarding: bool,
    proxy_protocol: Option<String>,
    proxy_host: Option<String>,
    proxy_port: Option<i64>,
    proxy_username: Option<String>,
    favorite: bool,
    environment: String,
    notes: String,
    last_connected_at: Option<String>,
    connect_count: i64,
    created_at: String,
    updated_at: String,
    has_password: bool,
    has_totp: bool,
    has_proxy_password: bool,
}

const HOST_SELECT: &str = "SELECT h.id, h.label, h.protocol, h.hostname, h.serial_json, h.proxy_command, h.port, h.username, h.group_id, h.auth, h.key_id,
        h.jump_host_id, h.agent_forwarding, h.proxy_protocol, h.proxy_host, h.proxy_port, h.proxy_username,
        h.favorite, h.environment, h.notes, h.last_connected_at, h.connect_count, h.created_at, h.updated_at,
        (s.password_enc IS NOT NULL) AS has_password, (s.totp_seed_enc IS NOT NULL) AS has_totp,
        (s.proxy_password_enc IS NOT NULL) AS has_proxy_password
    FROM hosts h LEFT JOIN host_secrets s ON s.host_id = h.id
    WHERE h.deleted_at IS NULL";

impl HostRow {
    fn proxy(&self) -> Option<HostProxy> {
        let protocol = ProxyProtocol::parse(self.proxy_protocol.as_deref()?)?;
        Some(HostProxy {
            protocol,
            host: self.proxy_host.clone()?,
            port: u16::try_from(self.proxy_port?).ok()?,
            username: self.proxy_username.clone(),
        })
    }

    fn into_host(self, tags: Vec<String>) -> Host {
        let proxy = self.proxy();
        Host {
            id: self.id,
            label: self.label,
            protocol: Protocol::parse(&self.protocol),
            hostname: self.hostname,
            port: u16::try_from(self.port).unwrap_or(22),
            serial: self
                .serial_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<SerialConfig>(j).ok()),
            proxy_command: self.proxy_command,
            username: self.username,
            group_id: self.group_id,
            tags,
            auth: AuthKind::parse(&self.auth),
            key_id: self.key_id,
            jump_host_id: self.jump_host_id,
            agent_forwarding: self.agent_forwarding,
            proxy,
            has_proxy_password: self.has_proxy_password,
            has_password: self.has_password,
            has_totp: self.has_totp,
            favorite: self.favorite,
            environment: Environment::parse(&self.environment),
            notes: self.notes,
            last_connected_at: self.last_connected_at,
            connect_count: u32::try_from(self.connect_count).unwrap_or(0),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl Store {
    /// All live hosts, ordered by label.
    pub async fn list_hosts(&self) -> Result<Vec<Host>> {
        let rows: Vec<HostRow> =
            sqlx::query_as(&format!("{HOST_SELECT} ORDER BY h.label COLLATE NOCASE"))
                .fetch_all(&self.pool)
                .await?;
        let tag_rows: Vec<(String, String)> =
            sqlx::query_as("SELECT host_id, tag FROM host_tags ORDER BY tag")
                .fetch_all(&self.pool)
                .await?;
        let mut tags: HashMap<String, Vec<String>> = HashMap::new();
        for (host_id, tag) in tag_rows {
            tags.entry(host_id).or_default().push(tag);
        }
        Ok(rows
            .into_iter()
            .map(|r| {
                let t = tags.remove(&r.id).unwrap_or_default();
                r.into_host(t)
            })
            .collect())
    }

    /// One live host.
    pub async fn get_host(&self, id: &str) -> Result<Host> {
        let row: Option<HostRow> = sqlx::query_as(&format!("{HOST_SELECT} AND h.id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let row = row.ok_or(StoreError::NotFound("host"))?;
        let tags: Vec<(String,)> =
            sqlx::query_as("SELECT tag FROM host_tags WHERE host_id = ? ORDER BY tag")
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
        Ok(row.into_host(tags.into_iter().map(|(t,)| t).collect()))
    }

    /// Hosts to dial for `id`, outermost jump first, ending with the host itself.
    pub async fn jump_chain(&self, id: &str) -> Result<Vec<Host>> {
        let mut chain: Vec<Host> = Vec::new();
        let mut next = Some(id.to_owned());
        while let Some(cur) = next {
            if chain.iter().any(|h| h.id == cur) {
                return Err(StoreError::InUse(
                    "These hosts jump through each other in a loop".into(),
                ));
            }
            if chain.len() >= MAX_HOPS {
                return Err(StoreError::InUse(format!(
                    "Jump chains are limited to {MAX_HOPS} hops"
                )));
            }
            let host = self.get_host(&cur).await?;
            next = host.jump_host_id.clone();
            chain.push(host);
        }
        chain.reverse();
        Ok(chain)
    }

    /// Reject a jump host that would make `id` reach itself or exceed the hop limit.
    async fn check_jump(&self, id: Option<&str>, jump: Option<&str>) -> Result<()> {
        let mut next = jump.map(str::to_owned);
        let mut hops = 1;
        while let Some(cur) = next {
            if Some(cur.as_str()) == id {
                return Err(StoreError::InUse("A host can't jump through itself".into()));
            }
            hops += 1;
            if hops > MAX_HOPS {
                return Err(StoreError::InUse(format!(
                    "Jump chains are limited to {MAX_HOPS} hops"
                )));
            }
            next = self.get_host(&cur).await?.jump_host_id;
        }
        Ok(())
    }

    /// Create a host; secrets are encrypted with the vault.
    pub async fn create_host(&self, vault: &UnlockedVault, input: HostInput) -> Result<Host> {
        self.check_jump(None, input.jump_host_id.as_deref()).await?;
        let id = new_id();
        let ts = now();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO hosts (id, label, hostname, username, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.label)
        .bind(&input.hostname)
        .bind(&input.username)
        .bind(&ts)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO host_secrets (host_id) VALUES (?)")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        write_fields(&mut tx, &id, &input).await?;
        write_tags(&mut tx, &id, &input.tags).await?;
        apply_secrets(&mut tx, vault, &id, &input).await?;
        tx.commit().await?;
        self.get_host(&id).await
    }

    /// Copy a host, its tags and its secrets under a new id ("label (copy)"). Connection
    /// history is not copied.
    pub async fn duplicate_host(&self, vault: &UnlockedVault, id: &str) -> Result<Host> {
        let src = self.get_host(id).await?;
        let creds = self.host_credentials(vault, id).await?;
        let keep = |s: Option<SecretString>| match s {
            Some(v) => SecretChange::Set {
                value: secrecy::ExposeSecret::expose_secret(&v).to_owned(),
            },
            None => SecretChange::Keep,
        };
        let mut label = format!("{} (copy)", src.label);
        label.truncate(120);
        let input = HostInput {
            label,
            protocol: src.protocol,
            hostname: src.hostname,
            port: src.port,
            serial: src.serial,
            proxy_command: src.proxy_command,
            username: src.username,
            group_id: src.group_id,
            tags: src.tags,
            auth: src.auth,
            key_id: src.key_id,
            jump_host_id: src.jump_host_id,
            agent_forwarding: src.agent_forwarding,
            proxy: src.proxy,
            proxy_password: keep(creds.proxy_password),
            favorite: false,
            environment: src.environment,
            notes: src.notes,
            password: keep(creds.password),
            totp_seed: keep(creds.totp_seed),
        };
        self.create_host(vault, input).await
    }

    /// Apply one change to many hosts in a single transaction (H5). Returns how many
    /// hosts changed.
    pub async fn bulk_edit(&self, edit: &BulkEdit) -> Result<u64> {
        let add = normalize_tags(&edit.add_tags);
        let remove = normalize_tags(&edit.remove_tags);
        let ts = now();
        let mut tx = self.pool.begin().await?;
        let mut changed = 0;
        for id in &edit.ids {
            let live: Option<(String,)> =
                sqlx::query_as("SELECT id FROM hosts WHERE id = ? AND deleted_at IS NULL")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if live.is_none() {
                continue;
            }
            if edit.move_group {
                sqlx::query("UPDATE hosts SET group_id = ? WHERE id = ?")
                    .bind(&edit.group_id)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            if let Some(env) = edit.environment {
                sqlx::query("UPDATE hosts SET environment = ? WHERE id = ?")
                    .bind(env.as_str())
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            if let Some(fav) = edit.favorite {
                sqlx::query("UPDATE hosts SET favorite = ? WHERE id = ?")
                    .bind(fav)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            write_tags(&mut tx, id, &add).await?;
            for tag in &remove {
                sqlx::query("DELETE FROM host_tags WHERE host_id = ? AND tag = ?")
                    .bind(id)
                    .bind(tag)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("UPDATE hosts SET updated_at = ? WHERE id = ?")
                .bind(&ts)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            changed += 1;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Update a host. `SecretChange::Keep` leaves secrets untouched.
    pub async fn update_host(
        &self,
        vault: &UnlockedVault,
        id: &str,
        input: HostInput,
    ) -> Result<Host> {
        self.check_jump(Some(id), input.jump_host_id.as_deref())
            .await?;
        let mut tx = self.pool.begin().await?;
        if write_fields(&mut tx, id, &input).await? == 0 {
            return Err(StoreError::NotFound("host"));
        }
        sqlx::query("DELETE FROM host_tags WHERE host_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        write_tags(&mut tx, id, &input.tags).await?;
        apply_secrets(&mut tx, vault, id, &input).await?;
        tx.commit().await?;
        self.get_host(id).await
    }

    /// Point a host at a jump host (or clear it), with the same loop checks as editing.
    pub async fn set_jump_host(&self, id: &str, jump: Option<&str>) -> Result<()> {
        self.check_jump(Some(id), jump).await?;
        sqlx::query("UPDATE hosts SET jump_host_id = ?, updated_at = ? WHERE id = ?")
            .bind(jump)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Switch a host to key authentication with `key_id` (after deploying that key).
    pub async fn set_auth_key(&self, id: &str, key_id: &str) -> Result<()> {
        sqlx::query("UPDATE hosts SET auth = 'key', key_id = ?, updated_at = ? WHERE id = ?")
            .bind(key_id)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Toggle the favorite flag.
    pub async fn set_favorite(&self, id: &str, favorite: bool) -> Result<()> {
        sqlx::query("UPDATE hosts SET favorite = ?, updated_at = ? WHERE id = ?")
            .bind(favorite)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Soft-delete (the UI offers Undo).
    pub async fn delete_host(&self, id: &str) -> Result<()> {
        // updated_at moves too, so the trash state syncs (newest change wins).
        let ts = now();
        sqlx::query("UPDATE hosts SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&ts)
            .bind(&ts)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Undo a soft delete.
    pub async fn restore_host(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE hosts SET deleted_at = NULL, updated_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Permanently remove soft-deleted hosts (run at startup).
    pub async fn purge_deleted_hosts(&self) -> Result<u64> {
        let res = sqlx::query("DELETE FROM hosts WHERE deleted_at IS NOT NULL")
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Record a successful connect.
    pub async fn mark_connected(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE hosts SET last_connected_at = ?, connect_count = connect_count + 1 WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Save a password typed at a login prompt ("remember" ticked).
    pub async fn set_host_password(
        &self,
        vault: &UnlockedVault,
        id: &str,
        password: &str,
    ) -> Result<()> {
        let change = SecretChange::Set {
            value: password.to_owned(),
        };
        let mut tx = self.pool.begin().await?;
        write_secret(&mut tx, vault, id, Secret::Password, &change).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Decrypt a host's stored credentials for one connection.
    pub async fn host_credentials(
        &self,
        vault: &UnlockedVault,
        id: &str,
    ) -> Result<HostCredentials> {
        let row: Option<SecretColumns> = sqlx::query_as(
            "SELECT password_enc, totp_seed_enc, proxy_password_enc FROM host_secrets WHERE host_id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let (pw, totp, proxy) = row.unwrap_or((None, None, None));
        Ok(HostCredentials {
            password: pw
                .map(|c| vault.decrypt_string(&aad_password(id), &c))
                .transpose()?,
            totp_seed: totp
                .map(|c| vault.decrypt_string(&aad_totp(id), &c))
                .transpose()?,
            proxy_password: proxy
                .map(|c| vault.decrypt_string(&aad_proxy(id), &c))
                .transpose()?,
        })
    }

    /// All groups.
    pub async fn list_groups(&self) -> Result<Vec<Group>> {
        let rows: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT id, name, sort_order FROM groups ORDER BY sort_order, name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, sort_order)| Group {
                id,
                name,
                sort_order,
            })
            .collect())
    }

    /// Create a group.
    pub async fn create_group(&self, name: &str) -> Result<Group> {
        let id = new_id();
        let ts = now();
        let next: (i32,) = sqlx::query_as("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM groups")
            .fetch_one(&self.pool)
            .await?;
        sqlx::query("INSERT INTO groups (id, name, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(name.trim())
            .bind(next.0)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await?;
        Ok(Group {
            id,
            name: name.trim().to_owned(),
            sort_order: next.0,
        })
    }

    /// Rename a group.
    pub async fn rename_group(&self, id: &str, name: &str) -> Result<()> {
        sqlx::query("UPDATE groups SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name.trim())
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete a group; its hosts become ungrouped.
    pub async fn delete_group(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM groups WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

async fn write_tags(tx: &mut sqlx::SqliteConnection, id: &str, tags: &[String]) -> Result<()> {
    for tag in tags {
        sqlx::query("INSERT OR IGNORE INTO host_tags (host_id, tag) VALUES (?, ?)")
            .bind(id)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

/// Every non-secret column of a host. Returns rows changed (0 = no such live host).
async fn write_fields(tx: &mut sqlx::SqliteConnection, id: &str, input: &HostInput) -> Result<u64> {
    let proxy = input.proxy.as_ref();
    let res = sqlx::query(
        "UPDATE hosts SET group_id = ?, label = ?, protocol = ?, hostname = ?, serial_json = ?, proxy_command = ?,
                          port = ?, username = ?, auth = ?, key_id = ?,
                          jump_host_id = ?, agent_forwarding = ?, proxy_protocol = ?, proxy_host = ?, proxy_port = ?,
                          proxy_username = ?, favorite = ?, environment = ?, notes = ?, updated_at = ?
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(&input.group_id)
    .bind(&input.label)
    .bind(input.protocol.as_str())
    .bind(&input.hostname)
    .bind(input.serial.map(|c| serde_json::to_string(&c)).transpose()?)
    .bind(&input.proxy_command)
    .bind(i64::from(input.port))
    .bind(&input.username)
    .bind(input.auth.as_str())
    .bind(&input.key_id)
    .bind(&input.jump_host_id)
    .bind(input.agent_forwarding)
    .bind(proxy.map(|p| p.protocol.as_str()))
    .bind(proxy.map(|p| p.host.clone()))
    .bind(proxy.map(|p| i64::from(p.port)))
    .bind(proxy.and_then(|p| p.username.clone()))
    .bind(input.favorite)
    .bind(input.environment.as_str())
    .bind(&input.notes)
    .bind(now())
    .bind(id)
    .execute(&mut *tx)
    .await?;
    Ok(res.rows_affected())
}

#[derive(Clone, Copy)]
enum Secret {
    Password,
    Totp,
    ProxyPassword,
}

async fn apply_secrets(
    tx: &mut sqlx::SqliteConnection,
    vault: &UnlockedVault,
    id: &str,
    input: &HostInput,
) -> Result<()> {
    write_secret(tx, vault, id, Secret::Password, &input.password).await?;
    write_secret(tx, vault, id, Secret::Totp, &input.totp_seed).await?;
    // Without a proxy there is nothing for a proxy password to belong to.
    let proxy_change = if input.proxy.is_none() {
        &SecretChange::Clear
    } else {
        &input.proxy_password
    };
    write_secret(tx, vault, id, Secret::ProxyPassword, proxy_change).await
}

async fn write_secret(
    tx: &mut sqlx::SqliteConnection,
    vault: &UnlockedVault,
    id: &str,
    which: Secret,
    change: &SecretChange,
) -> Result<()> {
    let (column, aad) = match which {
        Secret::Password => ("password_enc", aad_password(id)),
        Secret::Totp => ("totp_seed_enc", aad_totp(id)),
        Secret::ProxyPassword => ("proxy_password_enc", aad_proxy(id)),
    };
    let value: Option<Vec<u8>> = match change {
        SecretChange::Keep => return Ok(()),
        SecretChange::Clear => None,
        SecretChange::Set { value } if value.is_empty() => None,
        SecretChange::Set { value } => Some(vault.encrypt(&aad, value.as_bytes())?),
    };
    // `column` comes from the fixed match above, never from input.
    sqlx::query(&format!(
        "UPDATE host_secrets SET {column} = ? WHERE host_id = ?"
    ))
    .bind(value)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store_and_vault;
    use secrecy::ExposeSecret;

    fn input(host: &str) -> HostInput {
        HostInput {
            protocol: Default::default(),
            serial: None,
            proxy_command: None,
            label: String::new(),
            hostname: host.into(),
            port: 22,
            username: "deploy".into(),
            group_id: None,
            tags: vec!["db".into(), "eu".into()],
            auth: AuthKind::Password,
            key_id: None,
            jump_host_id: None,
            agent_forwarding: false,
            proxy: None,
            proxy_password: SecretChange::Keep,
            favorite: false,
            environment: Environment::Prod,
            notes: String::new(),
            password: SecretChange::Set {
                value: "hunter2-secret".into(),
            },
            totp_seed: SecretChange::Keep,
        }
        .normalized()
        .unwrap()
    }

    #[tokio::test]
    async fn protocol_serial_and_proxy_command_roundtrip() {
        use hopr_core::model::{Parity, SerialFlow};
        let (store, vault) = store_and_vault().await;
        let mut s = input("console");
        s.protocol = Protocol::Serial;
        s.hostname = "COM7".into();
        s.serial = Some(SerialConfig {
            baud: 115_200,
            data_bits: 7,
            parity: Parity::Even,
            stop_bits: 2,
            flow: SerialFlow::Hardware,
        });
        let h = store
            .create_host(&vault, s.normalized().unwrap())
            .await
            .unwrap();
        assert_eq!(h.protocol, Protocol::Serial);
        assert_eq!(
            h.serial.map(|c| (c.baud, c.parity)),
            Some((115_200, Parity::Even))
        );

        let mut p = input("behind-cf");
        p.proxy_command = Some("cloudflared access ssh --hostname %h".into());
        let h = store
            .create_host(&vault, p.normalized().unwrap())
            .await
            .unwrap();
        assert_eq!(h.protocol, Protocol::Ssh);
        assert_eq!(
            h.proxy_command.as_deref(),
            Some("cloudflared access ssh --hostname %h")
        );
        let copy = store.duplicate_host(&vault, &h.id).await.unwrap();
        assert_eq!(copy.proxy_command, h.proxy_command);
    }

    #[tokio::test]
    async fn proxy_settings_and_password_roundtrip() {
        let (store, vault) = store_and_vault().await;
        let mut i = input("db");
        i.proxy = Some(HostProxy {
            protocol: ProxyProtocol::Socks5,
            host: "proxy.corp".into(),
            port: 1080,
            username: Some("me".into()),
        });
        i.proxy_password = SecretChange::Set {
            value: "proxy-pw".into(),
        };
        let h = store.create_host(&vault, i).await.unwrap();
        assert_eq!(h.proxy.as_ref().map(|p| p.port), Some(1080));
        assert!(h.has_proxy_password);
        let c = store.host_credentials(&vault, &h.id).await.unwrap();
        assert_eq!(c.proxy_password.unwrap().expose_secret(), "proxy-pw");

        // Removing the proxy forgets its password too.
        let mut none = input("db");
        none.password = SecretChange::Keep;
        let h = store.update_host(&vault, &h.id, none).await.unwrap();
        assert!(h.proxy.is_none() && !h.has_proxy_password);
        assert!(h.has_password);
    }

    #[tokio::test]
    async fn duplicate_copies_settings_and_secrets_but_not_history() {
        let (store, vault) = store_and_vault().await;
        let src = store.create_host(&vault, input("db")).await.unwrap();
        store.mark_connected(&src.id).await.unwrap();
        let copy = store.duplicate_host(&vault, &src.id).await.unwrap();
        assert_ne!(copy.id, src.id);
        assert_eq!(copy.label, "db (copy)");
        assert_eq!(copy.tags, src.tags);
        assert_eq!(copy.connect_count, 0);
        let c = store.host_credentials(&vault, &copy.id).await.unwrap();
        assert_eq!(c.password.unwrap().expose_secret(), "hunter2-secret");
    }

    #[tokio::test]
    async fn bulk_edit_moves_tags_and_badges_many_hosts() {
        let (store, vault) = store_and_vault().await;
        let g = store.create_group("EU").await.unwrap();
        let a = store.create_host(&vault, input("a")).await.unwrap();
        let b = store.create_host(&vault, input("b")).await.unwrap();
        let untouched = store.create_host(&vault, input("c")).await.unwrap();
        let n = store
            .bulk_edit(&BulkEdit {
                ids: vec![a.id.clone(), b.id.clone(), "missing".into()],
                move_group: true,
                group_id: Some(g.id.clone()),
                add_tags: vec![" Web ".into()],
                remove_tags: vec!["db".into()],
                environment: Some(Environment::Staging),
                favorite: Some(true),
            })
            .await
            .unwrap();
        assert_eq!(n, 2);
        for id in [&a.id, &b.id] {
            let h = store.get_host(id).await.unwrap();
            assert_eq!(h.group_id.as_deref(), Some(g.id.as_str()));
            assert_eq!(h.tags, vec!["eu", "web"]);
            assert_eq!(h.environment, Environment::Staging);
            assert!(h.favorite);
        }
        let c = store.get_host(&untouched.id).await.unwrap();
        assert_eq!(c.tags, vec!["db", "eu"]);
        assert_eq!(c.group_id, None);
    }

    #[tokio::test]
    async fn create_list_update_delete_restore() {
        let (store, vault) = store_and_vault().await;
        let g = store.create_group("Prod").await.unwrap();
        let mut i = input("prod-db");
        i.group_id = Some(g.id.clone());
        let h = store.create_host(&vault, i).await.unwrap();
        assert_eq!(h.label, "prod-db");
        assert!(h.has_password && !h.has_totp);
        assert_eq!(h.tags, vec!["db", "eu"]);

        let mut upd = input("prod-db.internal");
        upd.password = SecretChange::Keep;
        upd.tags = vec!["db".into()];
        let h2 = store.update_host(&vault, &h.id, upd).await.unwrap();
        assert_eq!(h2.hostname, "prod-db.internal");
        assert!(h2.has_password, "Keep must not clear the password");
        assert_eq!(h2.tags, vec!["db"]);

        store.delete_host(&h.id).await.unwrap();
        assert!(store.list_hosts().await.unwrap().is_empty());
        store.restore_host(&h.id).await.unwrap();
        assert_eq!(store.list_hosts().await.unwrap().len(), 1);

        store.delete_group(&g.id).await.unwrap();
        assert_eq!(store.get_host(&h.id).await.unwrap().group_id, None);
    }

    #[tokio::test]
    async fn credentials_decrypt_and_clear() {
        let (store, vault) = store_and_vault().await;
        let mut i = input("a");
        i.totp_seed = SecretChange::Set {
            value: "JBSWY3DPEHPK3PXP".into(),
        };
        let h = store.create_host(&vault, i).await.unwrap();
        let c = store.host_credentials(&vault, &h.id).await.unwrap();
        assert_eq!(c.password.unwrap().expose_secret(), "hunter2-secret");
        assert_eq!(c.totp_seed.unwrap().expose_secret(), "JBSWY3DPEHPK3PXP");

        let mut clear = input("a");
        clear.password = SecretChange::Clear;
        let h = store.update_host(&vault, &h.id, clear).await.unwrap();
        assert!(!h.has_password && h.has_totp);
    }

    #[tokio::test]
    async fn secrets_are_never_stored_in_plaintext() {
        let (store, vault) = store_and_vault().await;
        store.create_host(&vault, input("a")).await.unwrap();
        let blobs: Vec<(Option<Vec<u8>>,)> =
            sqlx::query_as("SELECT password_enc FROM host_secrets")
                .fetch_all(&store.pool)
                .await
                .unwrap();
        let blob = blobs[0].0.clone().unwrap();
        assert!(!blob.windows(7).any(|w| w == b"hunter2"));
    }

    #[tokio::test]
    async fn jump_chains_resolve_outermost_first_and_reject_loops() {
        let (store, vault) = store_and_vault().await;
        let a = store.create_host(&vault, input("bastion")).await.unwrap();
        let mut ib = input("app");
        ib.jump_host_id = Some(a.id.clone());
        let b = store.create_host(&vault, ib).await.unwrap();
        let mut ic = input("db");
        ic.jump_host_id = Some(b.id.clone());
        let c = store.create_host(&vault, ic).await.unwrap();

        let chain: Vec<_> = store
            .jump_chain(&c.id)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.hostname)
            .collect();
        assert_eq!(chain, vec!["bastion", "app", "db"]);

        let mut loop_back = input("bastion");
        loop_back.jump_host_id = Some(c.id.clone());
        assert!(matches!(
            store.update_host(&vault, &a.id, loop_back).await,
            Err(StoreError::InUse(_))
        ));

        let mut self_jump = input("db");
        self_jump.jump_host_id = Some(c.id.clone());
        assert!(store.update_host(&vault, &c.id, self_jump).await.is_err());

        // Deleting a jump host ungroups the chain instead of breaking it.
        sqlx::query("DELETE FROM hosts WHERE id = ?")
            .bind(&b.id)
            .execute(&store.pool)
            .await
            .unwrap();
        assert_eq!(store.jump_chain(&c.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn mark_connected_counts() {
        let (store, vault) = store_and_vault().await;
        let h = store.create_host(&vault, input("a")).await.unwrap();
        store.mark_connected(&h.id).await.unwrap();
        store.mark_connected(&h.id).await.unwrap();
        let h = store.get_host(&h.id).await.unwrap();
        assert_eq!(h.connect_count, 2);
        assert!(h.last_connected_at.is_some());
    }
}
