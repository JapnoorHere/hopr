//! Session IPC: open / write / resize / ack / close, plus state events.
//! Output streams over a per-session binary `Channel` (ADR 0006).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hopr_core::session::{BatchConfig, FlowControl, OUTPUT_CAPACITY, SessionHandle, run_batcher};
use hopr_core::{AppError, ErrorCode, SessionId, SessionState};
use hopr_ssh::ConnectParams;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::sync::mpsc;

/// Registry of live session actors. The lock is never held across an `.await`.
#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<SessionId, SessionHandle>>,
}

impl SessionManager {
    fn get(&self, id: &str) -> Result<SessionHandle, AppError> {
        self.lock()?
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::SessionNotFound, "This session has ended"))
    }

    fn insert(&self, handle: SessionHandle) -> Result<(), AppError> {
        self.lock()?.insert(handle.id.clone(), handle);
        Ok(())
    }

    fn remove(&self, id: &str) {
        if let Ok(mut map) = self.lock() {
            map.remove(id);
        }
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

/// Quick-connect request. Deliberately not `Debug`: it carries a password.
#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    host: String,
    port: u16,
    username: String,
    password: String,
    cols: u32,
    rows: u32,
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

fn invalid(msg: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, msg)
}

fn validate(r: &ConnectRequest) -> Result<(), AppError> {
    let host = r.host.trim();
    if host.is_empty()
        || host.len() > 253
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid("Enter a valid hostname or IP address"));
    }
    if r.username.trim().is_empty()
        || r.username.len() > 64
        || r.username.chars().any(char::is_control)
    {
        return Err(invalid("Enter a valid username"));
    }
    if r.port == 0 {
        return Err(invalid("Port must be between 1 and 65535"));
    }
    if !(1..=1000).contains(&r.cols) || !(1..=1000).contains(&r.rows) {
        return Err(invalid("Invalid terminal size"));
    }
    Ok(())
}

/// Open an SSH session. Returns its id at once; progress arrives as `SessionStateEvent`s
/// and output on `output`.
#[tauri::command]
#[specta::specta]
pub async fn session_open(
    app: AppHandle,
    manager: State<'_, SessionManager>,
    request: ConnectRequest,
    output: Channel<RawBytes>,
) -> Result<SessionId, AppError> {
    validate(&request)?;
    let id = uuid::Uuid::new_v4().to_string();
    let params = ConnectParams {
        host: request.host.trim().to_owned(),
        port: request.port,
        username: request.username.trim().to_owned(),
        password: request.password.into(),
        cols: request.cols,
        rows: request.rows,
        connect_timeout: Duration::from_secs(20),
    };

    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let flow = Arc::new(FlowControl::default());
    let handle = hopr_ssh::spawn_session(id.clone(), params, tx, flow.clone());
    tauri::async_runtime::spawn(run_batcher(
        rx,
        flow,
        BatchConfig::default(),
        move |bytes| output.send(RawBytes(bytes)).is_ok(),
    ));
    manager.insert(handle.clone())?;
    forward_state(app, handle);
    Ok(id)
}

fn forward_state(app: AppHandle, handle: SessionHandle) {
    tauri::async_runtime::spawn(async move {
        let mut rx = handle.state();
        loop {
            let state = rx.borrow_and_update().clone();
            let terminal = state.is_terminal();
            let _ = SessionStateEvent {
                id: handle.id.clone(),
                state,
            }
            .emit(&app);
            if terminal {
                app.state::<SessionManager>().remove(&handle.id);
                break;
            }
            if rx.changed().await.is_err() {
                break;
            }
        }
    });
}

/// Send input. Synchronous on purpose: sync commands run in order, keystrokes must too.
#[tauri::command]
#[specta::specta]
pub fn session_write(
    manager: State<'_, SessionManager>,
    id: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    manager.get(&id)?.try_write(data)
}

/// Propagate a terminal resize.
#[tauri::command]
#[specta::specta]
pub fn session_resize(
    manager: State<'_, SessionManager>,
    id: String,
    cols: u32,
    rows: u32,
) -> Result<(), AppError> {
    if !(1..=1000).contains(&cols) || !(1..=1000).contains(&rows) {
        return Err(invalid("Invalid terminal size"));
    }
    manager.get(&id)?.try_resize(cols, rows)
}

/// The UI rendered `bytes` of output (flow control).
#[tauri::command]
#[specta::specta]
pub fn session_ack(
    manager: State<'_, SessionManager>,
    id: String,
    bytes: u32,
) -> Result<(), AppError> {
    manager.get(&id)?.ack(u64::from(bytes));
    Ok(())
}

/// Close a session. Closing one that already ended is not an error.
#[tauri::command]
#[specta::specta]
pub async fn session_close(manager: State<'_, SessionManager>, id: String) -> Result<(), AppError> {
    match manager.get(&id) {
        Ok(handle) => handle.close().await.or(Ok(())),
        Err(_) => Ok(()),
    }
}
