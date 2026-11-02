//! Port forwards: local, remote and SOCKS.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::AppError;

/// Direction of a port forward (§9.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TunnelKind {
    /// `-L`: a local port reaches a destination as seen from the host.
    Local,
    /// `-R`: a port on the host reaches a destination as seen from here.
    Remote,
    /// `-D`: a local SOCKS5 proxy whose connections leave from the host.
    Dynamic,
}

impl TunnelKind {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }

    /// Parse a database value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "local" => Some(Self::Local),
            "remote" => Some(Self::Remote),
            "dynamic" => Some(Self::Dynamic),
            _ => None,
        }
    }
}

/// A saved port forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    /// Id.
    pub id: String,
    /// Host it goes through.
    pub host_id: String,
    /// Optional name ("Postgres", "Grafana").
    pub label: String,
    /// Direction.
    pub kind: TunnelKind,
    /// Address to listen on (here for local/dynamic, on the host for remote).
    pub bind_addr: String,
    /// Port to listen on (0 = pick a free one when starting).
    pub bind_port: u16,
    /// Destination host (not used by dynamic).
    pub dest_host: String,
    /// Destination port (not used by dynamic).
    pub dest_port: u16,
    /// Start when the vault unlocks.
    pub auto_start: bool,
}

/// Create/update payload for a tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInput {
    /// Host it goes through.
    pub host_id: String,
    /// Optional name.
    pub label: String,
    /// Direction.
    pub kind: TunnelKind,
    /// Address to listen on.
    pub bind_addr: String,
    /// Port to listen on (0 = pick when starting).
    pub bind_port: u16,
    /// Destination host.
    pub dest_host: String,
    /// Destination port.
    pub dest_port: u16,
    /// Start when the vault unlocks.
    pub auto_start: bool,
}

impl TunnelInput {
    /// Validate and normalize (trim; dynamic tunnels have no destination).
    pub fn normalized(mut self) -> Result<Self, AppError> {
        self.label = self.label.trim().to_owned();
        self.bind_addr = self.bind_addr.trim().to_owned();
        self.dest_host = self.dest_host.trim().to_owned();
        if self.label.len() > 80 {
            return Err(AppError::invalid("Tunnel name is too long"));
        }
        if self.bind_addr.is_empty() {
            self.bind_addr = "127.0.0.1".into();
        }
        let bad_host =
            |h: &str| h.len() > 253 || h.chars().any(|c| c.is_whitespace() || c.is_control());
        if bad_host(&self.bind_addr) {
            return Err(AppError::invalid("Enter a valid address to listen on"));
        }
        match self.kind {
            TunnelKind::Dynamic => {
                self.dest_host.clear();
                self.dest_port = 0;
            }
            TunnelKind::Local | TunnelKind::Remote => {
                if self.dest_host.is_empty() || bad_host(&self.dest_host) {
                    return Err(AppError::invalid("Enter the destination host"));
                }
                if self.dest_port == 0 {
                    return Err(AppError::invalid(
                        "Destination port must be between 1 and 65535",
                    ));
                }
            }
        }
        Ok(self)
    }
}
