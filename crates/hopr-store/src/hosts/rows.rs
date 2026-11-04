//! How a host is read from the database: the SELECT and the row that becomes a `Host`.

use hopr_core::model::{
    AuthKind, Environment, Host, HostProxy, Protocol, ProxyProtocol, SerialConfig,
};
use sqlx::FromRow;

/// `(password_enc, totp_seed_enc, proxy_password_enc)` ciphertext columns.
pub(super) type SecretColumns = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

#[derive(FromRow)]
pub(super) struct HostRow {
    pub(super) id: String,
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

pub(super) const HOST_SELECT: &str = "SELECT h.id, h.label, h.protocol, h.hostname, h.serial_json, h.proxy_command, h.port, h.username, h.group_id, h.auth, h.key_id,
        h.jump_host_id, h.agent_forwarding, h.proxy_protocol, h.proxy_host, h.proxy_port, h.proxy_username,
        h.favorite, h.environment, h.notes, h.last_connected_at, h.connect_count, h.created_at, h.updated_at,
        (s.password_enc IS NOT NULL) AS has_password, (s.totp_seed_enc IS NOT NULL) AS has_totp,
        (s.proxy_password_enc IS NOT NULL) AS has_proxy_password
    FROM hosts h LEFT JOIN host_secrets s ON s.host_id = h.id
    WHERE h.deleted_at IS NULL";

impl HostRow {
    pub(super) fn proxy(&self) -> Option<HostProxy> {
        let protocol = ProxyProtocol::parse(self.proxy_protocol.as_deref()?)?;
        Some(HostProxy {
            protocol,
            host: self.proxy_host.clone()?,
            port: u16::try_from(self.proxy_port?).ok()?,
            username: self.proxy_username.clone(),
        })
    }

    pub(super) fn into_host(self, tags: Vec<String>) -> Host {
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
