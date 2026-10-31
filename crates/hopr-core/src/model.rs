//! Persistent domain model shared by the store, the IPC layer and the UI.

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

/// What a remote directory entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKind {
    /// Regular file.
    File,
    /// Directory.
    Dir,
    /// Symbolic link.
    Link,
    /// Anything else (device, socket, fifo).
    Other,
}

/// One entry in a remote directory listing (SFTP, S1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    /// File name.
    pub name: String,
    /// Full remote path.
    pub path: String,
    /// Kind.
    pub kind: RemoteKind,
    /// Size in bytes (f64: exact to 2^53, and TypeScript has no u64).
    #[specta(type = u32)] // a plain TS `number`; f64 would be `number | null`
    pub size: f64,
    /// Last modified, Unix seconds.
    pub modified: Option<u32>,
    /// Permission bits (e.g. 0o644), without the file-type bits.
    pub mode: u32,
    /// Owner name or uid.
    pub owner: Option<String>,
    /// Group name or gid.
    pub group: Option<String>,
}

/// What an OpenSSH certificate attached to a key says (CA-based login).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CertificateInfo {
    /// The certificate's key id (often the user or ticket).
    pub key_id: String,
    /// Users it may log in as (empty = any, as the server decides).
    pub principals: Vec<String>,
    /// Valid from, Unix seconds.
    #[specta(type = u32)]
    pub valid_after: f64,
    /// Valid until, Unix seconds (`None` = forever).
    #[specta(type = Option<u32>)]
    pub valid_before: Option<f64>,
    /// Fingerprint of the CA that signed it.
    pub ca_fingerprint: String,
    /// Past its end date.
    pub expired: bool,
}

/// What clicking a snippet does (§9.12).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// Type it at the prompt, without Enter.
    #[default]
    Paste,
    /// Type it and press Enter.
    PasteEnter,
    /// Show it, run on confirmation.
    Confirm,
}

impl RunMode {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paste => "paste",
            Self::PasteEnter => "paste_enter",
            Self::Confirm => "confirm",
        }
    }

    /// Parse a database value.
    pub fn parse(s: &str) -> Self {
        match s {
            "paste_enter" => Self::PasteEnter,
            "confirm" => Self::Confirm,
            _ => Self::Paste,
        }
    }
}

/// A saved command or script with optional `{{variables}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Folder path ("Ops/Postgres"); "" for the top level.
    pub folder: String,
    /// Tags.
    pub tags: Vec<String>,
    /// What it does.
    pub description: String,
    /// The text, with `{{name}}`, `{{name:default}}`, `{{name:a|b|c}}`, `{{secret:x}}`
    /// and built-ins `{{host}} {{user}} {{date}} {{clipboard}}`.
    pub body: String,
    /// Paste, paste + Enter, or confirm first.
    pub run_mode: RunMode,
    /// Updated (RFC 3339).
    pub updated_at: String,
}

/// Create/update payload for a snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    /// Name.
    pub name: String,
    /// Folder path.
    pub folder: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Description.
    pub description: String,
    /// Body.
    pub body: String,
    /// Run mode.
    pub run_mode: RunMode,
}

impl SnippetInput {
    /// Validate and normalize.
    pub fn normalized(mut self) -> Result<Self, AppError> {
        self.name = self.name.trim().to_owned();
        self.folder = self
            .folder
            .split('/')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        if self.name.is_empty() || self.name.len() > 120 {
            return Err(AppError::invalid(
                "Give the snippet a name (up to 120 characters)",
            ));
        }
        if self.body.trim().is_empty() {
            return Err(AppError::invalid("The snippet is empty"));
        }
        if self.body.len() > 64 * 1024 || self.description.len() > 2000 || self.folder.len() > 200 {
            return Err(AppError::invalid("The snippet is too long"));
        }
        self.tags = normalize_tags(&self.tags);
        Ok(self)
    }
}

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

/// A saved layout of tabs and splits (R6). `layout_json` is owned by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// Id.
    pub id: String,
    /// Name ("Last session" for the autosave).
    pub name: String,
    /// Tabs, splits and pane targets as JSON. Never secrets.
    pub layout_json: String,
    /// The autosaved last session.
    pub autosave: bool,
    /// Opened at startup.
    pub startup: bool,
    /// Last saved (RFC 3339).
    pub updated_at: String,
}

/// An entry in the local audit log (X7). Never contains a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    /// Id (UUIDv7, so ids sort by time).
    pub id: String,
    /// When (RFC 3339).
    pub at: String,
    /// What happened, e.g. `vault.unlock`, `session.connect`, `host_key.replace`.
    pub kind: String,
    /// Host involved, if any.
    pub host_id: Option<String>,
    /// Host label at the time (kept when the host is deleted later).
    pub host_label: Option<String>,
    /// Human-readable detail.
    pub detail: String,
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

/// A stored private key's public metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyInfo {
    /// Id.
    pub id: String,
    /// Display name.
    pub label: String,
    /// e.g. "ssh-ed25519".
    pub algorithm: String,
    /// "SHA256:…".
    pub fingerprint: String,
    /// OpenSSH public key line.
    pub public_key: String,
    /// Key comment.
    pub comment: String,
    /// Number of hosts using it.
    pub used_by: u32,
    /// Created (RFC 3339).
    pub created_at: String,
    /// OpenSSH certificate signed for this key, used with it when logging in.
    pub certificate: Option<String>,
    /// What the certificate says (filled in for the UI).
    pub certificate_info: Option<CertificateInfo>,
}

/// Algorithms offered by "Generate key" (HOPR_PLAN §9.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyAlgorithm {
    /// Ed25519 (default).
    Ed25519,
    /// ECDSA on NIST P-256.
    EcdsaP256,
    /// RSA 3072-bit.
    Rsa3072,
    /// RSA 4096-bit.
    Rsa4096,
}

/// A trusted host key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    /// Id.
    pub id: String,
    /// Hostname, `[host]:port` or OpenSSH hashed pattern.
    pub host_pattern: String,
    /// Port.
    pub port: u16,
    /// Key algorithm.
    pub key_type: String,
    /// Base64 public key blob.
    pub key_b64: String,
    /// "SHA256:…".
    pub fingerprint: String,
    /// First trusted (RFC 3339).
    pub first_seen_at: String,
    /// Last seen (RFC 3339).
    pub last_seen_at: String,
    /// "tofu" | "imported" | "manual".
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> HostInput {
        HostInput {
            protocol: Default::default(),
            serial: None,
            proxy_command: None,
            label: " ".into(),
            hostname: " prod-db ".into(),
            port: 22,
            username: "deploy".into(),
            group_id: None,
            tags: vec!["DB".into(), "db".into(), " ".into(), "eu".into()],
            auth: AuthKind::Password,
            key_id: None,
            jump_host_id: None,
            agent_forwarding: false,
            proxy: None,
            proxy_password: SecretChange::Keep,
            favorite: false,
            environment: Environment::Prod,
            notes: String::new(),
            password: SecretChange::Keep,
            totp_seed: SecretChange::Keep,
        }
    }

    #[test]
    fn telnet_and_serial_rules() {
        // Telnet: no user needed; SSH-only settings are dropped.
        let mut t = input();
        t.protocol = Protocol::Telnet;
        t.username = String::new();
        t.port = 23;
        t.proxy_command = Some("nc x 1".into());
        let n = t.normalized().unwrap();
        assert!(n.username.is_empty() && n.proxy_command.is_none() && n.serial.is_none());

        // Serial: defaults to 9600 8N1, bad settings refused.
        let mut s = input();
        s.protocol = Protocol::Serial;
        s.hostname = "COM3".into();
        s.username = String::new();
        let n = s.clone().normalized().unwrap();
        assert_eq!(n.serial, Some(SerialConfig::default()));
        s.serial = Some(SerialConfig {
            stop_bits: 3,
            ..SerialConfig::default()
        });
        assert!(s.normalized().is_err());

        // SSH still needs a user; ProxyCommand is one line and excludes a jump host.
        let mut ssh = input();
        ssh.username = String::new();
        assert!(ssh.normalized().is_err());
        let mut pc = input();
        pc.proxy_command = Some("  ssh -W %h:%p jump  ".into());
        assert_eq!(
            pc.clone().normalized().unwrap().proxy_command.as_deref(),
            Some("ssh -W %h:%p jump")
        );
        pc.jump_host_id = Some("j".into());
        assert!(pc.clone().normalized().is_err());
        pc.jump_host_id = None;
        pc.proxy_command = Some("a\nb".into());
        assert!(pc.normalized().is_err());
    }

    #[test]
    fn validates_and_trims_the_proxy() {
        let mut i = input();
        i.proxy = Some(HostProxy {
            protocol: ProxyProtocol::Socks5,
            host: " proxy.corp ".into(),
            port: 1080,
            username: Some("  ".into()),
        });
        let n = i.normalized().unwrap();
        let p = n.proxy.unwrap();
        assert_eq!(p.host, "proxy.corp");
        assert_eq!(p.username, None);

        let mut i = input();
        i.proxy = Some(HostProxy {
            protocol: ProxyProtocol::Http,
            host: "".into(),
            port: 8080,
            username: None,
        });
        assert!(i.normalized().is_err());
    }

    #[test]
    fn normalizes_label_and_tags() {
        let n = input().normalized().unwrap();
        assert_eq!(n.hostname, "prod-db");
        assert_eq!(n.label, "prod-db");
        assert_eq!(n.tags, vec!["db", "eu"]);
    }

    #[test]
    fn rejects_bad_input() {
        let mut i = input();
        i.hostname = "two words".into();
        assert!(i.normalized().is_err());
        let mut i = input();
        i.auth = AuthKind::Key;
        assert!(i.normalized().is_err());
        let mut i = input();
        i.port = 0;
        assert!(i.normalized().is_err());
    }

    #[test]
    fn secret_change_serializes_tagged() {
        let v = serde_json::to_value(SecretChange::Set { value: "x".into() }).unwrap();
        assert_eq!(v["action"], "set");
    }
}

/// One command run in a terminal (history, §9.19).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    /// Id.
    pub id: String,
    /// Saved host, if the session was one.
    pub host_id: Option<String>,
    /// Host label (or `user@host:port`, or the local shell's name).
    pub host_label: String,
    /// The command line.
    pub command: String,
    /// Working directory, when the shell reported it (OSC 7).
    pub cwd: Option<String>,
    /// Exit code, when the shell reported it (OSC 133;D).
    pub exit_code: Option<i32>,
    /// From shell integration (`true`) or read back from typed input (`false`).
    pub exact: bool,
    /// RFC 3339.
    pub started_at: String,
    /// How long it ran, when known.
    pub duration_ms: Option<u32>,
}

/// One connection, from ready to closed (connection history, §9.19).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRecord {
    /// Id.
    pub id: String,
    /// Saved host, if any.
    pub host_id: Option<String>,
    /// Host label.
    pub host_label: String,
    /// "ssh" or "local".
    pub kind: String,
    /// RFC 3339.
    pub started_at: String,
    /// RFC 3339; `None` while still open (or if Hopr quit first).
    pub ended_at: Option<String>,
    /// "user_closed", "remote_exited", "connection_lost", "failed".
    pub end_reason: Option<String>,
    /// Bytes received.
    #[specta(type = u32)]
    pub bytes_in: f64,
    /// Bytes sent.
    #[specta(type = u32)]
    pub bytes_out: f64,
}

/// A saved session recording (asciicast v2, §9.18).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    /// File name inside the recordings folder (also its id).
    pub name: String,
    /// What was recorded (host label).
    pub title: String,
    /// RFC 3339 (from the file's header, else its modified time).
    pub started_at: String,
    /// File size in bytes.
    #[specta(type = u32)]
    pub size: f64,
    /// Length in seconds (time of the last event).
    #[specta(type = u32)]
    pub duration: f64,
}
