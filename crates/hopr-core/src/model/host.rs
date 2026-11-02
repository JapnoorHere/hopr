//! Saved hosts: how they connect (SSH, Telnet, serial), authenticate, and the input that creates or edits them.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::AppError;

/// How a saved host authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// Password (and keyboard-interactive, with optional TOTP).
    Password,
    /// A private key from the vault.
    Key,
    /// The system SSH agent.
    Agent,
}

impl AuthKind {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::Agent => "agent",
        }
    }

    /// Parse a database value (unknown values fall back to password).
    pub fn parse(s: &str) -> Self {
        match s {
            "key" => Self::Key,
            "agent" => Self::Agent,
            _ => Self::Password,
        }
    }
}

/// Environment badge (HOPR_PLAN §9.5). `Prod` gets a red edge and extra confirmations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    /// No badge.
    None,
    /// Development.
    Dev,
    /// Staging.
    Staging,
    /// Production.
    Prod,
}

impl Environment {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }

    /// Parse a database value.
    pub fn parse(s: &str) -> Self {
        match s {
            "dev" => Self::Dev,
            "staging" => Self::Staging,
            "prod" => Self::Prod,
            _ => Self::None,
        }
    }
}

/// Proxy protocol for reaching a host's first hop (C7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProxyProtocol {
    /// SOCKS5 (names resolved by the proxy).
    Socks5,
    /// HTTP `CONNECT`.
    Http,
}

impl ProxyProtocol {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Socks5 => "socks5",
            Self::Http => "http",
        }
    }

    /// Parse a database value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "socks5" => Some(Self::Socks5),
            "http" => Some(Self::Http),
            _ => None,
        }
    }
}

/// What a saved host speaks (§9.1, reach).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// SSH (the default).
    #[default]
    Ssh,
    /// Telnet: plain text, for network gear and old systems.
    Telnet,
    /// A local serial port (console cables); `hostname` holds the port name.
    Serial,
}

impl Protocol {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Telnet => "telnet",
            Self::Serial => "serial",
        }
    }

    /// Parse a database value (unknown values fall back to SSH).
    pub fn parse(s: &str) -> Self {
        match s {
            "telnet" => Self::Telnet,
            "serial" => Self::Serial,
            _ => Self::Ssh,
        }
    }
}

/// Serial parity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    /// None.
    #[default]
    None,
    /// Odd.
    Odd,
    /// Even.
    Even,
}

/// Serial flow control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SerialFlow {
    /// None.
    #[default]
    None,
    /// XON/XOFF.
    Software,
    /// RTS/CTS.
    Hardware,
}

/// Line settings for a serial host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SerialConfig {
    /// Bits per second, e.g. 9600 or 115200.
    pub baud: u32,
    /// 5–8.
    pub data_bits: u8,
    /// Parity.
    pub parity: Parity,
    /// 1 or 2.
    pub stop_bits: u8,
    /// Flow control.
    pub flow: SerialFlow,
}

impl Default for SerialConfig {
    /// 9600 8N1, the usual console default.
    fn default() -> Self {
        Self {
            baud: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            flow: SerialFlow::None,
        }
    }
}

impl SerialConfig {
    /// Check the values a serial port accepts.
    pub fn validate(&self) -> Result<(), AppError> {
        if !(50..=4_000_000).contains(&self.baud) {
            return Err(AppError::invalid(
                "Baud rate must be between 50 and 4,000,000",
            ));
        }
        if !(5..=8).contains(&self.data_bits) {
            return Err(AppError::invalid("Data bits must be 5, 6, 7 or 8"));
        }
        if !(1..=2).contains(&self.stop_bits) {
            return Err(AppError::invalid("Stop bits must be 1 or 2"));
        }
        Ok(())
    }
}

/// A proxy in front of a host. Its password lives in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostProxy {
    /// Protocol.
    pub protocol: ProxyProtocol,
    /// Proxy host.
    pub host: String,
    /// Proxy port.
    pub port: u16,
    /// Username, if the proxy needs one.
    pub username: Option<String>,
}

/// A saved host as the UI sees it. Secrets are never included, only whether they exist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    /// Id (UUIDv7).
    pub id: String,
    /// Display name.
    pub label: String,
    /// SSH, telnet or serial.
    pub protocol: Protocol,
    /// Hostname or IP (serial: the port name, e.g. `COM3` or `/dev/ttyUSB0`).
    pub hostname: String,
    /// SSH or telnet port.
    pub port: u16,
    /// Line settings (serial only).
    pub serial: Option<SerialConfig>,
    /// Command whose stdin/stdout carry the SSH connection (OpenSSH `ProxyCommand`).
    pub proxy_command: Option<String>,
    /// Remote user.
    pub username: String,
    /// Owning group.
    pub group_id: Option<String>,
    /// Free-form tags.
    pub tags: Vec<String>,
    /// Authentication method.
    pub auth: AuthKind,
    /// Key used when `auth == Key`.
    pub key_id: Option<String>,
    /// Saved host to connect through first (jump chain).
    pub jump_host_id: Option<String>,
    /// Forward the local SSH agent to this host.
    pub agent_forwarding: bool,
    /// Proxy for the first hop.
    pub proxy: Option<HostProxy>,
    /// A proxy password is stored in the vault.
    pub has_proxy_password: bool,
    /// A password is stored in the vault.
    pub has_password: bool,
    /// A TOTP seed is stored in the vault.
    pub has_totp: bool,
    /// Pinned to Favorites.
    pub favorite: bool,
    /// Environment badge.
    pub environment: Environment,
    /// Markdown notes.
    pub notes: String,
    /// Last successful connect (RFC 3339).
    pub last_connected_at: Option<String>,
    /// Number of successful connects.
    pub connect_count: u32,
    /// Created (RFC 3339).
    pub created_at: String,
    /// Updated (RFC 3339).
    pub updated_at: String,
}

/// How an edit treats a secret field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SecretChange {
    /// Leave the stored value as is.
    Keep,
    /// Remove the stored value.
    Clear,
    /// Replace with a new value.
    Set {
        /// New plaintext (encrypted before storage).
        value: String,
    },
}

/// Create/update payload for a host. Not `Debug`: it may carry secrets.
#[derive(Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostInput {
    /// Display name (defaults to hostname when empty).
    pub label: String,
    /// SSH (default), telnet or serial.
    #[serde(default)]
    pub protocol: Protocol,
    /// Hostname or IP (serial: the port name).
    pub hostname: String,
    /// SSH or telnet port.
    pub port: u16,
    /// Line settings (serial only; defaults to 9600 8N1).
    #[serde(default)]
    pub serial: Option<SerialConfig>,
    /// OpenSSH-style `ProxyCommand` (`%h`, `%p`, `%r` are replaced).
    #[serde(default)]
    pub proxy_command: Option<String>,
    /// Remote user.
    pub username: String,
    /// Owning group.
    pub group_id: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Authentication method.
    pub auth: AuthKind,
    /// Key for `auth == Key`.
    pub key_id: Option<String>,
    /// Saved host to connect through first (jump chain).
    pub jump_host_id: Option<String>,
    /// Forward the local SSH agent to this host.
    pub agent_forwarding: bool,
    /// Proxy for the first hop.
    pub proxy: Option<HostProxy>,
    /// Proxy password change.
    pub proxy_password: SecretChange,
    /// Favorite flag.
    pub favorite: bool,
    /// Environment badge.
    pub environment: Environment,
    /// Notes.
    pub notes: String,
    /// Password change.
    pub password: SecretChange,
    /// TOTP seed change (base32).
    pub totp_seed: SecretChange,
}

impl HostInput {
    /// Validate and normalize (trim, default label, dedupe tags).
    pub fn normalized(mut self) -> Result<Self, AppError> {
        self.hostname = self.hostname.trim().to_owned();
        self.username = self.username.trim().to_owned();
        self.label = self.label.trim().to_owned();
        if self.hostname.is_empty()
            || self.hostname.len() > 253
            || self
                .hostname
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(AppError::invalid(if self.protocol == Protocol::Serial {
                "Enter the serial port, e.g. COM3 or /dev/ttyUSB0"
            } else {
                "Enter a valid hostname or IP address"
            }));
        }
        // Telnet logs in at the remote prompt and serial has no login: the user is optional.
        let needs_user = self.protocol == Protocol::Ssh;
        if (needs_user && self.username.is_empty())
            || self.username.len() > 64
            || self.username.chars().any(char::is_control)
        {
            return Err(AppError::invalid("Enter a valid username"));
        }
        match self.protocol {
            Protocol::Serial => {
                let cfg = self.serial.unwrap_or_default();
                cfg.validate()?;
                self.serial = Some(cfg);
                self.proxy_command = None;
                self.proxy = None;
                self.jump_host_id = None;
            }
            Protocol::Telnet => {
                self.serial = None;
                self.proxy_command = None;
                self.jump_host_id = None;
            }
            Protocol::Ssh => self.serial = None,
        }
        self.proxy_command = self
            .proxy_command
            .take()
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty());
        if self
            .proxy_command
            .as_ref()
            .is_some_and(|c| c.len() > 1000 || c.contains(['\n', '\r', '\0']))
        {
            return Err(AppError::invalid(
                "ProxyCommand must be one line of up to 1000 characters",
            ));
        }
        if self.proxy_command.is_some() && (self.proxy.is_some() || self.jump_host_id.is_some()) {
            return Err(AppError::invalid(
                "Use either a ProxyCommand or a jump host / proxy, not both",
            ));
        }
        if self.port == 0 {
            return Err(AppError::invalid("Port must be between 1 and 65535"));
        }
        if self.label.is_empty() {
            self.label = self.hostname.clone();
        }
        if self.label.len() > 120 {
            return Err(AppError::invalid("Label is too long"));
        }
        if self.protocol == Protocol::Ssh && self.auth == AuthKind::Key && self.key_id.is_none() {
            return Err(AppError::invalid("Choose a key for key authentication"));
        }
        if self.notes.len() > 20_000 {
            return Err(AppError::invalid("Notes are too long"));
        }
        if let Some(p) = self.proxy.as_mut() {
            p.host = p.host.trim().to_owned();
            if p.host.is_empty()
                || p.host.len() > 253
                || p.host.chars().any(|c| c.is_whitespace() || c.is_control())
            {
                return Err(AppError::invalid(
                    "Enter the proxy's hostname or IP address",
                ));
            }
            if p.port == 0 {
                return Err(AppError::invalid("Proxy port must be between 1 and 65535"));
            }
            p.username = p
                .username
                .take()
                .map(|u| u.trim().to_owned())
                .filter(|u| !u.is_empty());
            if p.username.as_ref().is_some_and(|u| u.len() > 255) {
                return Err(AppError::invalid("Proxy username is too long"));
            }
        }
        self.tags = normalize_tags(&self.tags);
        Ok(self)
    }
}

/// Trim, lowercase, drop empty or over-long tags, sort and dedupe.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty() && t.len() <= 32)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// One change applied to many hosts at once (H5). `None` fields are left alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkEdit {
    /// Hosts to change.
    pub ids: Vec<String>,
    /// Move to this group (`groupId` null = ungroup) when `moveGroup` is set.
    pub move_group: bool,
    /// Target group for `moveGroup`.
    pub group_id: Option<String>,
    /// Tags to add.
    pub add_tags: Vec<String>,
    /// Tags to remove.
    pub remove_tags: Vec<String>,
    /// New environment badge.
    pub environment: Option<Environment>,
    /// New favorite flag.
    pub favorite: Option<bool>,
}

/// A host group (folder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Sort position.
    pub sort_order: i32,
}
