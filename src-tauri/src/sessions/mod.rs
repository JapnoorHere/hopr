//! Session IPC: open (saved host / quick connect / local shell), write, resize, ack,
//! host-key and prompt answers, close, plus state events.
//! Output streams over a per-session binary `Channel` (ADR 0006).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hopr_core::cast::CastWriter;
use hopr_core::session::SessionHandle;
use hopr_core::{AppError, ErrorCode, SessionId, SessionState};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri_specta::Event;

pub mod commands;
mod route;

pub(crate) use route::{host_route, quiet_params};

/// Registry of live session actors. The lock is never held across an `.await`.
#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<SessionId, SessionHandle>>,
    lives: Mutex<HashMap<SessionId, Arc<Live>>>,
}

/// Where a session's output goes. Swappable, so a tab can move to another window (T18)
/// without reconnecting: while it moves, output is held (bounded by flow control, since
/// held bytes are never acknowledged).
enum Sink {
    Window(Channel<RawBytes>),
    Held(Vec<u8>),
}

/// Who a session is for (history, recordings).
pub(crate) struct Who {
    pub host_id: Option<String>,
    pub label: String,
    /// "ssh" or "local".
    pub kind: &'static str,
}

/// An asciicast recording in progress.
pub(crate) struct Recorder {
    pub name: String,
    pub writer: CastWriter<BufWriter<File>>,
}

/// Per-session bookkeeping beside the actor: output sink, recording, counters.
pub(crate) struct Live {
    sink: Mutex<Sink>,
    pub rec: Mutex<Option<Recorder>>,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    /// Current grid size (a recording starts with it).
    pub size: Mutex<(u32, u32)>,
    pub who: Who,
    /// Connection history row, once the session was ready.
    conn_id: Mutex<Option<String>>,
}

impl Live {
    /// Stop recording, if it is. Returns the file name.
    pub fn stop_recording(&self) -> Option<String> {
        let mut rec = self.rec.lock().ok()?.take()?;
        let _ = rec.writer.flush();
        Some(rec.name)
    }
}

impl SessionManager {
    fn get(&self, id: &str) -> Result<SessionHandle, AppError> {
        self.lock()?
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::SessionNotFound, "This session has ended"))
    }

    fn insert(&self, handle: SessionHandle, live: Arc<Live>) -> Result<(), AppError> {
        if let Ok(mut lives) = self.lives.lock() {
            lives.insert(handle.id.clone(), live);
        }
        self.lock()?.insert(handle.id.clone(), handle);
        Ok(())
    }

    fn remove(&self, id: &str) {
        if let Ok(mut map) = self.lock() {
            map.remove(id);
        }
        if let Ok(mut lives) = self.lives.lock() {
            lives.remove(id);
        }
    }

    /// Send input to a session (snippets, broadcast), counted like typed input.
    pub(crate) fn write(&self, id: &str, data: Vec<u8>) -> Result<(), AppError> {
        let handle = self.get(id)?;
        if let Ok(live) = self.live(id) {
            live.bytes_out
                .fetch_add(data.len() as u64, Ordering::Relaxed);
        }
        handle.try_write(data)
    }

    pub(crate) fn live(&self, id: &str) -> Result<Arc<Live>, AppError> {
        self.lives
            .lock()
            .ok()
            .and_then(|s| s.get(id).cloned())
            .ok_or_else(|| AppError::new(ErrorCode::SessionNotFound, "This session has ended"))
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<SessionId, SessionHandle>>, AppError> {
        self.sessions
            .lock()
            .map_err(|_| AppError::new(ErrorCode::Internal, "Session registry poisoned"))
    }
}

/// Terminal bytes. Sent to JS as a raw `ArrayBuffer`, not a JSON number array.
#[derive(Type)]
pub struct RawBytes(Vec<u8>);

impl IpcResponse for RawBytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

/// What to connect to. Deliberately not `Debug`: quick connect may carry a password.
#[derive(Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectTarget {
    /// A saved host (credentials come from the vault).
    #[serde(rename_all = "camelCase")]
    Host {
        /// Host id.
        host_id: String,
    },
    /// An ad-hoc connection.
    Quick {
        /// Hostname or IP.
        host: String,
        /// Port.
        port: u16,
        /// Remote user.
        username: String,
        /// Password, if typed up front; otherwise the server will be asked how to log in.
        password: Option<String>,
    },
    /// A local shell.
    #[serde(rename_all = "camelCase")]
    Local {
        /// Shell profile id from `shells_list`.
        shell_id: String,
    },
}

/// Answer to an authentication prompt. Not `Debug`: carries secrets.
#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptReply {
    /// One value per prompt field.
    pub responses: Vec<String>,
    /// Save the typed password to the vault after a successful login.
    pub remember: bool,
}

/// Emitted on every session state transition.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SessionStateEvent {
    /// Session id.
    pub id: SessionId,
    /// New state.
    pub state: SessionState,
}

fn check_size(cols: u32, rows: u32) -> Result<(), AppError> {
    if !(1..=1000).contains(&cols) || !(1..=1000).contains(&rows) {
        return Err(AppError::invalid("Invalid terminal size"));
    }
    Ok(())
}

fn check_quick(host: &str, port: u16, username: &str) -> Result<(), AppError> {
    let host = host.trim();
    if host.is_empty()
        || host.len() > 253
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(AppError::invalid("Enter a valid hostname or IP address"));
    }
    if username.trim().is_empty() || username.len() > 64 || username.chars().any(char::is_control) {
        return Err(AppError::invalid("Enter a valid username"));
    }
    if port == 0 {
        return Err(AppError::invalid("Port must be between 1 and 65535"));
    }
    Ok(())
}
