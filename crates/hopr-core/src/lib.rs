//! Domain types shared by every Hopr crate and exported to the UI via tauri-specta.

#![warn(missing_docs)]

pub mod session;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Closed set of error codes the UI maps to friendly copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Input failed validation before any work was done.
    InvalidInput,
    /// DNS resolution or TCP connect failed.
    ConnectFailed,
    /// The connection attempt exceeded its deadline.
    Timeout,
    /// The server rejected every offered credential.
    AuthFailed,
    /// The server's host key does not match the stored one.
    HostKeyMismatch,
    /// The referenced session does not exist (already closed).
    SessionNotFound,
    /// The connection dropped after it was established.
    Disconnected,
    /// The vault is locked; unlock before reading secrets.
    VaultLocked,
    /// A storage (SQLite) operation failed.
    Storage,
    /// Anything not covered above.
    Internal,
}

/// Error that crosses the IPC boundary. Never carries secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    /// Machine-readable category.
    pub code: ErrorCode,
    /// Short human-readable summary.
    pub message: String,
    /// Optional technical detail for the "details" expander.
    pub detail: Option<String>,
    /// Whether retrying the same action may succeed.
    pub retryable: bool,
}

impl AppError {
    /// Build an error with no detail; retryable is derived from the code.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable = matches!(
            code,
            ErrorCode::ConnectFailed | ErrorCode::Timeout | ErrorCode::Disconnected
        );
        Self {
            code,
            message: message.into(),
            detail: None,
            retryable,
        }
    }

    /// Attach a technical detail string.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Opaque session identifier.
pub type SessionId = String;

/// Connection lifecycle. Invalid states are unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionState {
    /// Opening the TCP connection to hop `hop` (1-based) of `total`.
    Connecting {
        /// Current hop, 1-based.
        hop: u32,
        /// Number of hops in the chain.
        total: u32,
    },
    /// Running authentication with the named method.
    Authenticating {
        /// e.g. "password", "publickey".
        method: String,
    },
    /// Shell is live.
    Ready,
    /// Connection lost; retrying.
    Reconnecting {
        /// Attempt number, 1-based.
        attempt: u32,
    },
    /// Session ended.
    Closed {
        /// Why it ended.
        reason: CloseReason,
    },
    /// Session failed before or during setup.
    Failed {
        /// What went wrong.
        error: AppError,
    },
}

/// Why a session closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// The user closed it.
    UserClosed,
    /// The remote shell exited.
    RemoteExited,
    /// The network or server dropped the connection.
    ConnectionLost,
}

impl SessionState {
    /// True once the session can no longer change state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed { .. } | Self::Failed { .. })
    }
}

/// Build and platform info shown in About and the status bar.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AppInfo {
    /// Semver of the app.
    pub version: String,
    /// "windows" | "macos" | "linux" | ...
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// True in debug builds.
    pub debug: bool,
}

impl AppInfo {
    /// Info for the current build.
    pub fn current(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            debug: cfg!(debug_assertions),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_follows_code() {
        assert!(AppError::new(ErrorCode::Timeout, "t").retryable);
        assert!(!AppError::new(ErrorCode::AuthFailed, "a").retryable);
    }

    #[test]
    fn session_state_serializes_tagged() {
        let json = serde_json::to_string(&SessionState::Connecting { hop: 1, total: 2 }).unwrap();
        assert_eq!(json, r#"{"kind":"connecting","hop":1,"total":2}"#);
        let closed = SessionState::Closed {
            reason: CloseReason::RemoteExited,
        };
        assert!(closed.is_terminal());
        assert!(!SessionState::Ready.is_terminal());
    }

    #[test]
    fn app_error_serializes_snake_case_code() {
        let e = AppError::new(ErrorCode::HostKeyMismatch, "changed").with_detail("sha256:x");
        let v: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "host_key_mismatch");
        assert_eq!(v["detail"], "sha256:x");
    }
}
