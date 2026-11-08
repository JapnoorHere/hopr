//! The session commands: open, write, resize, ack, answers, detach/attach and close.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hopr_core::model::Protocol;
use hopr_core::session::{
    BatchConfig, FlowControl, OUTPUT_CAPACITY, PromptAnswer, SessionHandle, run_batcher,
};
use hopr_core::{AppError, CloseReason, ErrorCode, HostKeyDecision, SessionId, SessionState};
use secrecy::SecretString;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::sync::mpsc;

use super::route::*;
use super::*;
use crate::state::AppState;

/// Open a session. Returns its id at once; progress arrives as `SessionStateEvent`s
/// and output on `output`.
#[tauri::command]
#[specta::specta]
pub async fn session_open(
    app: AppHandle,
    manager: State<'_, SessionManager>,
    state: State<'_, Arc<AppState>>,
    target: ConnectTarget,
    cols: u32,
    rows: u32,
    output: Channel<RawBytes>,
) -> Result<SessionId, AppError> {
    check_size(cols, rows)?;
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
    let flow = Arc::new(FlowControl::default());

    let mut audit_target = None;
    let mut local_name = None;
    // "telnet" / "serial" for those hosts (history shows the kind).
    let mut plain_kind: Option<&'static str> = None;
    let handle = match target {
        // A local shell runs as the user on this computer and uses nothing from the vault, so
        // it opens while Hopr is locked (the quick terminal keeps working through auto-lock).
        ConnectTarget::Local { shell_id } => {
            let shells = tokio::task::spawn_blocking(hopr_term::detect_shells)
                .await
                .map_err(|e| AppError::internal("Shell detection failed", e))?;
            let shell = shells
                .iter()
                .find(|s| s.id == shell_id)
                .ok_or_else(|| AppError::invalid("That shell is no longer available"))?;
            local_name = Some(shell.name.clone());
            hopr_term::spawn_local(id.clone(), shell, cols, rows, tx, flow.clone())?
        }
        ConnectTarget::Host { host_id } => {
            state.require_unlocked()?;
            let host = state.store.get_host(&host_id).await?;
            match host.protocol {
                Protocol::Ssh => {
                    let plan = SshPlan::Host { host_id };
                    audit_target = Some(plan.audit_target(&state).await);
                    let params =
                        build_params(state.inner().clone(), plan, None, cols, rows).await?;
                    hopr_ssh::spawn_session(id.clone(), params, tx, flow.clone())
                }
                Protocol::Telnet | Protocol::Serial => {
                    let target = AuditTarget {
                        host_id: Some(host.id.clone()),
                        label: host.label.clone(),
                    };
                    let handle = if host.protocol == Protocol::Telnet {
                        target.record(&state, "session.connect", "Telnet (unencrypted)");
                        hopr_term::telnet::spawn_telnet(
                            id.clone(),
                            host.hostname.clone(),
                            host.port,
                            cols,
                            rows,
                            Duration::from_secs(20),
                            tx,
                            flow.clone(),
                        )
                    } else {
                        let cfg = host.serial.unwrap_or_default();
                        let h = hopr_term::serial::spawn_serial(
                            id.clone(),
                            &host.hostname,
                            &cfg,
                            tx,
                            flow.clone(),
                        )?;
                        target.record(
                            &state,
                            "session.connect",
                            format!("Serial {}", host.hostname),
                        );
                        h
                    };
                    let _ = state.store.mark_connected(&host.id).await;
                    plain_kind = Some(host.protocol.as_str());
                    audit_target = Some(target);
                    handle
                }
            }
        }
        ConnectTarget::Quick {
            host,
            port,
            username,
            password,
        } => {
            check_quick(&host, port, &username)?;
            let plan = SshPlan::Quick {
                host: host.trim().to_owned(),
                port,
                username: username.trim().to_owned(),
            };
            let password = password.filter(|p| !p.is_empty()).map(SecretString::from);
            audit_target = Some(plan.audit_target(&state).await);
            let params = build_params(state.inner().clone(), plan, password, cols, rows).await?;
            hopr_ssh::spawn_session(id.clone(), params, tx, flow.clone())
        }
    };
    let who = match (&audit_target, local_name) {
        (Some(t), _) => Who {
            host_id: t.host_id.clone(),
            label: t.label.clone(),
            kind: plain_kind.unwrap_or("ssh"),
        },
        (None, name) => Who {
            host_id: None,
            label: name.unwrap_or_else(|| "Local shell".to_owned()),
            kind: "local",
        },
    };
    let live = Arc::new(Live {
        sink: Mutex::new(Sink::Window(output)),
        rec: Mutex::new(None),
        bytes_in: AtomicU64::new(0),
        bytes_out: AtomicU64::new(0),
        size: Mutex::new((cols, rows)),
        who,
        conn_id: Mutex::new(None),
    });
    if app.state::<crate::history::HistoryState>().get().record_all
        && let Err(e) = crate::recording::start(&state, &live)
    {
        eprintln!("hopr: couldn't start recording: {}", e.message);
    }
    let batch_live = live.clone();
    tauri::async_runtime::spawn(run_batcher(
        rx,
        flow,
        BatchConfig::default(),
        move |bytes| {
            batch_live
                .bytes_in
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            if let Ok(mut rec) = batch_live.rec.lock()
                && let Some(r) = rec.as_mut()
                && r.writer.output(&bytes).is_err()
            {
                // Disk full or similar: stop instead of failing on every chunk.
                *rec = None;
            }
            match batch_live.sink.lock().as_deref_mut() {
                Ok(Sink::Window(ch)) => ch.send(RawBytes(bytes)).is_ok(),
                Ok(Sink::Held(buf)) => {
                    buf.extend_from_slice(&bytes);
                    true
                }
                Err(_) => false,
            }
        },
    ));
    manager.insert(handle.clone(), live.clone())?;
    forward_state(app, handle, state.inner().clone(), audit_target, live);
    Ok(id)
}

pub(super) fn close_reason(state: &SessionState) -> &'static str {
    match state {
        SessionState::Closed { reason } => match reason {
            CloseReason::UserClosed => "user_closed",
            CloseReason::RemoteExited => "remote_exited",
            CloseReason::ConnectionLost => "connection_lost",
        },
        _ => "failed",
    }
}

pub(super) fn forward_state(
    app: AppHandle,
    handle: SessionHandle,
    app_state: Arc<AppState>,
    audit: Option<AuditTarget>,
    live: Arc<Live>,
) {
    tauri::async_runtime::spawn(async move {
        let mut rx = handle.state();
        loop {
            let state = rx.borrow_and_update().clone();
            let terminal = state.is_terminal();
            let first_ready =
                state == SessionState::Ready && live.conn_id.lock().is_ok_and(|c| c.is_none());
            if first_ready {
                match app_state
                    .store
                    .connection_started(live.who.host_id.as_deref(), &live.who.label, live.who.kind)
                    .await
                {
                    Ok(cid) => {
                        if let Ok(mut c) = live.conn_id.lock() {
                            *c = Some(cid);
                        }
                    }
                    Err(e) => eprintln!("hopr: connection history: {e}"),
                }
            }
            if terminal {
                live.stop_recording();
                let cid = live.conn_id.lock().ok().and_then(|mut c| c.take());
                if let Some(cid) = cid {
                    let _ = app_state
                        .store
                        .connection_ended(
                            &cid,
                            close_reason(&state),
                            live.bytes_in.load(Ordering::Relaxed),
                            live.bytes_out.load(Ordering::Relaxed),
                        )
                        .await;
                }
            }
            if let (Some(target), SessionState::Failed { error }) = (&audit, &state) {
                match error.code {
                    ErrorCode::AuthFailed => {
                        target.record(&app_state, "session.auth_failed", error.message.clone());
                    }
                    ErrorCode::HostKeyMismatch => {
                        target.record(&app_state, "host_key.blocked", error.message.clone());
                    }
                    _ => {}
                }
            }
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
    state: State<'_, Arc<AppState>>,
    id: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    state.touch();
    let handle = manager.get(&id)?;
    if let Ok(live) = manager.live(&id) {
        live.bytes_out
            .fetch_add(data.len() as u64, Ordering::Relaxed);
    }
    handle.try_write(data)
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
    check_size(cols, rows)?;
    let handle = manager.get(&id)?;
    if let Ok(live) = manager.live(&id) {
        if let Ok(mut size) = live.size.lock() {
            *size = (cols, rows);
        }
        if let Ok(mut rec) = live.rec.lock()
            && let Some(r) = rec.as_mut()
        {
            let _ = r.writer.resize(cols, rows);
        }
    }
    handle.try_resize(cols, rows)
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

/// Answer a host key prompt.
#[tauri::command]
#[specta::specta]
pub fn session_host_key_decision(
    manager: State<'_, SessionManager>,
    id: String,
    decision: HostKeyDecision,
) -> Result<(), AppError> {
    manager.get(&id)?.try_host_key_decision(decision)
}

/// Answer (or cancel, with `null`) an authentication prompt.
#[tauri::command]
#[specta::specta]
pub fn session_prompt_response(
    manager: State<'_, SessionManager>,
    id: String,
    reply: Option<PromptReply>,
) -> Result<(), AppError> {
    let answer = reply.map(|r| PromptAnswer {
        responses: r.responses.into_iter().map(SecretString::from).collect(),
        remember: r.remember,
    });
    manager.get(&id)?.try_prompt_response(answer)
}

/// Hold sessions' output while their tab moves to another window.
#[tauri::command]
#[specta::specta]
pub fn session_detach(
    manager: State<'_, SessionManager>,
    ids: Vec<String>,
) -> Result<(), AppError> {
    for id in ids {
        let live = manager.live(&id)?;
        if let Ok(mut s) = live.sink.lock()
            && matches!(*s, Sink::Window(_))
        {
            *s = Sink::Held(Vec::new());
        }
    }
    Ok(())
}

/// Send a session's output to this window's channel, starting with anything held.
#[tauri::command]
#[specta::specta]
pub fn session_attach(
    manager: State<'_, SessionManager>,
    id: String,
    output: Channel<RawBytes>,
) -> Result<(), AppError> {
    let live = manager.live(&id)?;
    let mut s = live
        .sink
        .lock()
        .map_err(|_| AppError::new(ErrorCode::Internal, "Session output poisoned"))?;
    if let Sink::Held(buf) = &mut *s
        && !buf.is_empty()
    {
        let _ = output.send(RawBytes(std::mem::take(buf)));
    }
    *s = Sink::Window(output);
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
