//! Command and connection history (§9.19) and command-finished notifications.
//! Commands are reported by the UI (shell integration or typed input); the session they
//! ran in decides which host they belong to.

use std::sync::{Arc, Mutex};

use hopr_core::model::{CommandRecord, ConnectionRecord};
use hopr_core::{AppError, ErrorCode};
use hopr_store::NewCommand;
use hopr_store::history::is_excluded;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};

use crate::sessions::SessionManager;
use crate::state::AppState;

/// Settings key.
pub const SETTINGS_KEY: &str = "history.settings";

/// History and recording preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistorySettings {
    /// Save commands at all.
    pub enabled: bool,
    /// Skip commands typed with a leading space.
    pub ignore_space: bool,
    /// Commands matching these globs (`*` = anything, case-insensitive) are never saved.
    pub exclude: Vec<String>,
    /// Record every new session (asciicast).
    pub record_all: bool,
    /// Notify when a command that ran at least this long finishes in the background
    /// (0 = never).
    pub notify_after_secs: u32,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            ignore_space: true,
            exclude: ["*password*", "*passwd*", "*secret*", "*token=*"]
                .map(str::to_owned)
                .to_vec(),
            record_all: false,
            notify_after_secs: 10,
        }
    }
}

/// The settings in memory, so saving a command needs no settings read.
#[derive(Default)]
pub struct HistoryState(Mutex<HistorySettings>);

impl HistoryState {
    pub fn get(&self) -> HistorySettings {
        self.0.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn set(&self, s: HistorySettings) {
        if let Ok(mut cur) = self.0.lock() {
            *cur = s;
        }
    }
}

fn invalid(msg: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, msg)
}

#[tauri::command]
#[specta::specta]
pub fn history_settings(history: State<'_, HistoryState>) -> HistorySettings {
    history.get()
}

#[tauri::command]
#[specta::specta]
pub async fn history_set_settings(
    state: State<'_, Arc<AppState>>,
    history: State<'_, HistoryState>,
    settings: HistorySettings,
) -> Result<(), AppError> {
    if settings.exclude.len() > 100 || settings.exclude.iter().any(|p| p.len() > 200) {
        return Err(invalid("Up to 100 exclude patterns of 200 characters each"));
    }
    if settings.notify_after_secs > 86_400 {
        return Err(invalid("Notify after at most a day"));
    }
    let mut s = settings;
    s.exclude = s
        .exclude
        .into_iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect();
    state.store.set_setting(SETTINGS_KEY, &s).await?;
    history.set(s);
    Ok(())
}

/// A command the UI saw run.
#[derive(Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandInput {
    /// Session it ran in.
    pub session_id: String,
    /// The command line.
    pub command: String,
    /// Working directory (OSC 7).
    pub cwd: Option<String>,
    /// Exit code (OSC 133;D).
    pub exit_code: Option<i32>,
    /// From shell integration.
    pub exact: bool,
    /// Run time.
    pub duration_ms: Option<u32>,
}

/// Save a command, unless history is off or the command is excluded. Returns what was
/// saved.
#[tauri::command]
#[specta::specta]
pub async fn history_add(
    state: State<'_, Arc<AppState>>,
    history: State<'_, HistoryState>,
    manager: State<'_, SessionManager>,
    entry: CommandInput,
) -> Result<Option<CommandRecord>, AppError> {
    let settings = history.get();
    let command = entry.command.trim_end();
    if !settings.enabled
        || command.len() > 8192
        || is_excluded(command, settings.ignore_space, &settings.exclude)
    {
        return Ok(None);
    }
    // The session may have just ended (`exit`): then there's nothing to attribute it to.
    let Ok(live) = manager.live(&entry.session_id) else {
        return Ok(None);
    };
    let cwd = entry.cwd.filter(|c| c.len() <= 4096 && !c.is_empty());
    let saved = state
        .store
        .add_command(NewCommand {
            host_id: live.who.host_id.as_deref(),
            host_label: &live.who.label,
            session_id: &entry.session_id,
            command: command.trim_start(),
            cwd: cwd.as_deref(),
            exit_code: entry.exit_code,
            exact: entry.exact,
            started_at: None,
            duration_ms: entry.duration_ms,
        })
        .await?;
    Ok(Some(saved))
}

/// Commands, newest first. `host_id` "" means quick-connect and local sessions.
#[tauri::command]
#[specta::specta]
pub async fn history_list(
    state: State<'_, Arc<AppState>>,
    query: String,
    host_id: Option<String>,
    limit: u32,
) -> Result<Vec<CommandRecord>, AppError> {
    state.require_unlocked()?;
    Ok(state
        .store
        .list_commands(&query, host_id.as_deref(), limit)
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn history_delete(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.delete_command(&id).await?)
}

/// Forget every command and connection.
#[tauri::command]
#[specta::specta]
pub async fn history_clear(state: State<'_, Arc<AppState>>) -> Result<u32, AppError> {
    state.require_unlocked()?;
    let n = state.store.clear_history().await?;
    state.audit("history.clear", None, format!("Cleared {n} commands"));
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Connections, newest first.
#[tauri::command]
#[specta::specta]
pub async fn connections_list(
    state: State<'_, Arc<AppState>>,
    host_id: Option<String>,
    limit: u32,
) -> Result<Vec<ConnectionRecord>, AppError> {
    state.require_unlocked()?;
    Ok(state
        .store
        .list_connections(host_id.as_deref(), limit)
        .await?)
}

/// Show a system notification (a long command finished while Hopr was in the background).
#[tauri::command]
#[specta::specta]
pub fn notify(app: AppHandle, title: String, body: String) -> Result<(), AppError> {
    use tauri_plugin_notification::NotificationExt;
    let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    app.notification()
        .builder()
        .title(clip(&title, 120))
        .body(clip(&body, 400))
        .show()
        .map_err(|e| {
            AppError::new(ErrorCode::Internal, "Couldn't show a notification")
                .with_detail(e.to_string())
        })
}
