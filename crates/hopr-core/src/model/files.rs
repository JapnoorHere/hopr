//! Remote files as the file browser sees them.

use serde::{Deserialize, Serialize};
use specta::Type;

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
