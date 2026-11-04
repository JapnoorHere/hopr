//! How a host's fields, tags and secrets are written.

use hopr_core::model::{HostInput, SecretChange};
use hopr_vault::UnlockedVault;

use crate::{Result, now};

use super::*;

pub(super) async fn write_tags(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    tags: &[String],
) -> Result<()> {
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
pub(super) async fn write_fields(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    input: &HostInput,
) -> Result<u64> {
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
pub(super) enum Secret {
    Password,
    Totp,
    ProxyPassword,
}

pub(super) async fn apply_secrets(
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

pub(super) async fn write_secret(
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
