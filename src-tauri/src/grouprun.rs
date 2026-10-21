//! Run on group (§9.13 mode B, R3): one command on many hosts over exec, a few at a
//! time, with stop-on-fail and cancel. Each host's result streams to the UI as an event.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hopr_core::model::{Host, Protocol};
use hopr_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::sync::Semaphore;

use crate::sessions;
use crate::state::AppState;

/// Longest one host may take.
const PER_HOST_TIMEOUT: Duration = Duration::from_secs(300);
/// Output kept per stream in an event.
const MAX_EVENT_OUTPUT: usize = 64 * 1024;

fn invalid(msg: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, msg)
}

/// Where one host is in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HostRunState {
    /// Waiting for a slot.
    Queued,
    /// Running now.
    Running,
    /// Exited 0.
    Ok,
    /// Non-zero exit, or couldn't run.
    Failed,
    /// Not run (cancelled, or stopped after a failure).
    Skipped,
}

/// One host's progress or result.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct GroupRunEvent {
    /// Run id.
    pub run_id: String,
    /// Host id.
    pub host_id: String,
    /// Host label.
    pub label: String,
    /// State.
    pub state: HostRunState,
    /// Exit status, when it ran.
    pub exit_status: Option<u32>,
    /// How long it took.
    pub duration_ms: Option<u32>,
    /// Standard output (last 64 KB).
    pub stdout: String,
    /// Standard error (last 64 KB).
    pub stderr: String,
    /// Why it couldn't run.
    pub error: Option<String>,
}

/// Cancel flags of runs in progress.
#[derive(Default)]
pub struct GroupRuns(Mutex<HashMap<String, Arc<AtomicBool>>>);

fn tail(s: String) -> String {
    if s.len() <= MAX_EVENT_OUTPUT {
        return s;
    }
    let mut start = s.len() - MAX_EVENT_OUTPUT;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_owned()
}

fn event(run: &str, host: &Host, state: HostRunState) -> GroupRunEvent {
    GroupRunEvent {
        run_id: run.to_owned(),
        host_id: host.id.clone(),
        label: host.label.clone(),
        state,
        exit_status: None,
        duration_ms: None,
        stdout: String::new(),
        stderr: String::new(),
        error: None,
    }
}

struct Ctx {
    app: AppHandle,
    state: Arc<AppState>,
    slots: Arc<Semaphore>,
    cancel: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    run_id: String,
    command: String,
    stop_on_fail: bool,
}

async fn run_one(ctx: Arc<Ctx>, h: Host) {
    let Ok(_permit) = ctx.slots.clone().acquire_owned().await else {
        return;
    };
    if ctx.cancel.load(Ordering::Relaxed)
        || (ctx.stop_on_fail && ctx.failed.load(Ordering::Relaxed))
    {
        let _ = event(&ctx.run_id, &h, HostRunState::Skipped).emit(&ctx.app);
        return;
    }
    let mut ev = event(&ctx.run_id, &h, HostRunState::Running);
    let _ = ev.clone().emit(&ctx.app);
    let started = Instant::now();
    let result = if h.protocol == Protocol::Ssh {
        match sessions::quiet_params(&ctx.state, &h.id, hopr_ssh::SessionHooks::default()).await {
            Ok(params) => {
                tokio::time::timeout(PER_HOST_TIMEOUT, hopr_ssh::exec(params, &ctx.command, &[]))
                    .await
                    .unwrap_or_else(|_| {
                        Err(AppError::new(
                            ErrorCode::Timeout,
                            "Took longer than 5 minutes",
                        ))
                    })
            }
            Err(e) => Err(e),
        }
    } else {
        Err(invalid("Not an SSH host"))
    };
    ev.duration_ms = Some(u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX));
    match result {
        Ok(out) => {
            ev.exit_status = out.exit_status;
            ev.stdout = tail(out.stdout);
            ev.stderr = tail(out.stderr);
            ev.state = if out.exit_status == Some(0) {
                HostRunState::Ok
            } else {
                HostRunState::Failed
            };
        }
        Err(e) => {
            ev.state = HostRunState::Failed;
            ev.error = Some(e.message);
        }
    }
    if ev.state == HostRunState::Failed {
        ctx.failed.store(true, Ordering::Relaxed);
    }
    let _ = ev.emit(&ctx.app);
}

/// Start running `command` on `host_ids`. Returns the run id at once; results arrive as
/// `GroupRunEvent`s.
#[tauri::command]
#[specta::specta]
pub async fn group_run(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    runs: State<'_, GroupRuns>,
    host_ids: Vec<String>,
    command: String,
    parallel: u32,
    stop_on_fail: bool,
) -> Result<String, AppError> {
    state.require_unlocked()?;
    if host_ids.is_empty() || host_ids.len() > 500 {
        return Err(invalid("Choose between 1 and 500 hosts"));
    }
    if command.trim().is_empty() || command.len() > 16 * 1024 {
        return Err(invalid("Enter a command (up to 16 KB)"));
    }
    let parallel = parallel.clamp(1, 32);
    let mut hosts = Vec::with_capacity(host_ids.len());
    for id in &host_ids {
        hosts.push(state.store.get_host(id).await?);
    }
    let run_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut m) = runs.0.lock() {
        m.insert(run_id.clone(), cancel.clone());
    }
    state.audit(
        "group.run",
        None,
        format!(
            "Ran on {} hosts ({parallel} at a time): {}",
            hosts.len(),
            command.lines().next().unwrap_or_default()
        ),
    );
    for h in &hosts {
        let _ = event(&run_id, h, HostRunState::Queued).emit(&app);
    }
    let ctx = Arc::new(Ctx {
        app: app.clone(),
        state: state.inner().clone(),
        slots: Arc::new(Semaphore::new(usize::try_from(parallel).unwrap_or(1))),
        cancel,
        failed: Arc::new(AtomicBool::new(false)),
        run_id: run_id.clone(),
        command,
        stop_on_fail,
    });
    tauri::async_runtime::spawn(async move {
        let tasks: Vec<_> = hosts
            .into_iter()
            .map(|h| tauri::async_runtime::spawn(run_one(ctx.clone(), h)))
            .collect();
        for t in tasks {
            let _ = t.await;
        }
        if let Ok(mut m) = ctx.app.state::<GroupRuns>().0.lock() {
            m.remove(&ctx.run_id);
        }
    });
    Ok(run_id)
}

/// Stop a run: hosts not started yet are skipped (running ones finish).
#[tauri::command]
#[specta::specta]
pub fn group_run_cancel(runs: State<'_, GroupRuns>, run_id: String) {
    if let Ok(m) = runs.0.lock()
        && let Some(c) = m.get(&run_id)
    {
        c.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_EVENT_OUTPUT, tail};

    #[test]
    fn tail_keeps_the_end_on_a_char_boundary() {
        assert_eq!(tail("short".into()), "short");
        let long = format!("é{}", "x".repeat(MAX_EVENT_OUTPUT));
        let t = tail(long);
        assert_eq!(t.len(), MAX_EVENT_OUTPUT);
        assert!(t.chars().all(|c| c == 'x'));
    }
}
