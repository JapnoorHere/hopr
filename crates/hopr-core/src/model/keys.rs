//! SSH keys, certificates and trusted host keys.

use serde::{Deserialize, Serialize};
use specta::Type;

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
