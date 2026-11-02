//! What happened: the audit log, command and connection history, recordings.

use serde::{Deserialize, Serialize};
use specta::Type;

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
