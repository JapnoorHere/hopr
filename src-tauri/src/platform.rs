//! Desktop integration (U5, T18): tray menu, close-to-tray, the global quick-terminal
//! hotkey and its drop-down window, and torn-off tab windows.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hopr_core::AppError;
use hopr_core::hotkey::to_accelerator;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, WindowEvent, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_specta::Event;

use crate::state::{AppState, VaultLockedEvent};

/// Settings key: closing the main window keeps Hopr in the tray.
pub const CLOSE_TO_TRAY_KEY: &str = "app.close_to_tray";
/// Settings key: global quick-terminal hotkey ("" = off).
pub const HOTKEY_KEY: &str = "app.quick_hotkey";
/// Default quick-terminal hotkey (HOPR_PLAN §11).
pub const HOTKEY_DEFAULT: &str = "Ctrl+`";

const QUICK: &str = "quick";
const TRAY_ID: &str = "hopr";

/// Window-integration state.
pub struct Platform {
    close_to_tray: AtomicBool,
    hotkey: Mutex<String>,
    /// The quick terminal has had focus since it was last shown (only then does losing
    /// focus hide it: Windows may refuse the focus when another app is in front).
    quick_had_focus: AtomicBool,
    /// When the quick terminal was last shown: focus can flicker while a window is created
    /// or raised, so losing it right after showing doesn't count.
    quick_shown_at: Mutex<Option<Instant>>,
}

impl Platform {
    pub fn new(close_to_tray: bool, hotkey: String) -> Self {
        Self {
            close_to_tray: AtomicBool::new(close_to_tray),
            hotkey: Mutex::new(hotkey),
            quick_had_focus: AtomicBool::new(false),
            quick_shown_at: Mutex::new(None),
        }
    }

    /// Whether losing focus now should hide the quick terminal.
    fn quick_blur_hides(&self) -> bool {
        let settled = self
            .quick_shown_at
            .lock()
            .ok()
            .and_then(|t| *t)
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        settled && self.quick_had_focus.swap(false, Ordering::Relaxed)
    }
}

fn show_quick(app: &AppHandle, w: &WebviewWindow) {
    let platform = app.state::<Platform>();
    platform.quick_had_focus.store(false, Ordering::Relaxed);
    if let Ok(mut t) = platform.quick_shown_at.lock() {
        *t = Some(Instant::now());
    }
    place_quick(app, w);
    let _ = w.show();
    let _ = w.set_focus();
}

/// A host listed under "Recent hosts" in the tray.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrayHost {
    /// Host id.
    pub id: String,
    /// Label.
    pub label: String,
}

/// The user picked a host in the tray menu.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TrayConnectEvent {
    /// Host id.
    pub host_id: String,
}

/// Desktop preferences shown in Settings.
#[derive(Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPrefs {
    /// Closing the window keeps Hopr running in the tray.
    pub close_to_tray: bool,
    /// Global quick-terminal hotkey ("" = off).
    pub quick_hotkey: String,
}

fn internal(e: impl std::fmt::Display) -> AppError {
    AppError::internal("Window operation failed", e)
}

fn register_hotkey(app: &AppHandle, chord: &str) -> Result<(), AppError> {
    if chord.is_empty() {
        return Ok(());
    }
    let accel = to_accelerator(chord)
        .ok_or_else(|| AppError::invalid("Use a modifier plus a key, like Ctrl+`"))?;
    let shortcut = Shortcut::from_str(&accel)
        .map_err(|_| AppError::invalid("That key can't be a global hotkey"))?;
    app.global_shortcut().register(shortcut).map_err(|e| {
        AppError::invalid("Another app already uses that hotkey").with_detail(e.to_string())
    })
}

/// Called at startup: register the saved hotkey (a failure only logs).
pub fn init_hotkey(app: &AppHandle) {
    let chord = app
        .state::<Platform>()
        .hotkey
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();
    if let Err(e) = register_hotkey(app, &chord) {
        eprintln!(
            "hopr: quick-terminal hotkey {chord} not registered: {}",
            e.message
        );
    }
}

/// Global-shortcut handler: the only registered shortcut toggles the quick terminal.
pub fn on_shortcut(app: &AppHandle, state: ShortcutState) {
    if state == ShortcutState::Pressed {
        toggle_quick(app);
    }
}

fn place_quick(app: &AppHandle, w: &WebviewWindow) {
    let monitor = app
        .get_webview_window("main")
        .and_then(|m| m.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let size = m.size();
        let pos = m.position();
        let width = size.width * 7 / 10;
        let height = size.height * 4 / 10;
        let _ = w.set_size(PhysicalSize { width, height });
        let x = pos.x + i32::try_from((size.width - width) / 2).unwrap_or(0);
        let _ = w.set_position(PhysicalPosition { x, y: pos.y });
    }
}

/// Show the quick terminal (creating it on first use), or hide it if it has focus.
pub fn toggle_quick(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(QUICK) {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_quick(app, &w);
        }
        return;
    }
    // Built off the event-loop thread: the hotkey handler runs on it, and creating a
    // window there would wait on itself.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::new(&app, QUICK, WebviewUrl::App("index.html".into()))
            .title("Hopr quick terminal")
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build();
        match built {
            Ok(w) => show_quick(&app, &w),
            Err(e) => eprintln!("hopr: quick terminal window failed: {e}"),
        }
    });
}

/// Bring the main window back (from the tray or the quick terminal).
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_menu(app: &AppHandle, recent: &[TrayHost]) -> tauri::Result<Menu<Wry>> {
    let show = MenuItem::with_id(app, "show", "Show Hopr", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Quick terminal", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let recent_menu = Submenu::with_id(app, "recent", "Recent hosts", !recent.is_empty())?;
    for h in recent.iter().take(8) {
        recent_menu.append(&MenuItem::with_id(
            app,
            format!("host:{}", h.id),
            &h.label,
            true,
            None::<&str>,
        )?)?;
    }
    let sep2 = PredefinedMenuItem::separator(app)?;
    let lock = MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Hopr", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[&show, &quick, &sep1, &recent_menu, &sep2, &lock, &quit],
    )
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "quick" => toggle_quick(app),
        "lock" => {
            let state = app.state::<Arc<AppState>>();
            if state.is_unlocked() {
                state.lock();
                state.audit("vault.lock", None, "Locked from the tray");
                let _ = VaultLockedEvent {
                    reason: "manual".into(),
                }
                .emit(app);
            }
        }
        "quit" => app.exit(0),
        other => {
            if let Some(host_id) = other.strip_prefix("host:") {
                show_main(app);
                let _ = TrayConnectEvent {
                    host_id: host_id.to_owned(),
                }
                .emit(app);
            }
        }
    }
}

/// Create the tray icon.
pub fn init_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, &[])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Hopr")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| on_menu(app, e.id.as_ref()))
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Window events: main closes to the tray; the quick terminal hides when it loses focus.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match (window.label(), event) {
        ("main", WindowEvent::CloseRequested { api, .. }) => {
            let platform = window.state::<Platform>();
            if platform.close_to_tray.load(Ordering::Relaxed) {
                api.prevent_close();
                let _ = window.hide();
            } else {
                // Main closing means quitting: take the other windows with it.
                window.app_handle().exit(0);
            }
        }
        (QUICK, WindowEvent::Focused(true)) => {
            window
                .state::<Platform>()
                .quick_had_focus
                .store(true, Ordering::Relaxed);
        }
        (QUICK, WindowEvent::Focused(false)) if window.state::<Platform>().quick_blur_hides() => {
            let _ = window.hide();
        }
        _ => {}
    }
}

/// Desktop preferences.
#[tauri::command]
#[specta::specta]
pub fn desktop_prefs(platform: State<'_, Platform>) -> DesktopPrefs {
    DesktopPrefs {
        close_to_tray: platform.close_to_tray.load(Ordering::Relaxed),
        quick_hotkey: platform
            .hotkey
            .lock()
            .map(|h| h.clone())
            .unwrap_or_default(),
    }
}

/// Keep Hopr running in the tray when the window is closed.
#[tauri::command]
#[specta::specta]
pub async fn desktop_set_close_to_tray(
    state: State<'_, Arc<AppState>>,
    platform: State<'_, Platform>,
    enabled: bool,
) -> Result<(), AppError> {
    state.store.set_setting(CLOSE_TO_TRAY_KEY, &enabled).await?;
    platform.close_to_tray.store(enabled, Ordering::Relaxed);
    Ok(())
}

/// Change the global quick-terminal hotkey ("" turns it off). If the new one can't be
/// registered, the old one is put back.
#[tauri::command]
#[specta::specta]
pub async fn desktop_set_quick_hotkey(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: State<'_, Platform>,
    chord: String,
) -> Result<(), AppError> {
    let chord = chord.trim().to_owned();
    let old = platform
        .hotkey
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();
    if chord == old {
        return Ok(());
    }
    let _ = app.global_shortcut().unregister_all();
    if let Err(e) = register_hotkey(&app, &chord) {
        let _ = register_hotkey(&app, &old);
        return Err(e);
    }
    state.store.set_setting(HOTKEY_KEY, &chord).await?;
    if let Ok(mut h) = platform.hotkey.lock() {
        *h = chord;
    }
    Ok(())
}

/// Replace the tray's "Recent hosts" list.
#[tauri::command]
#[specta::specta]
pub fn tray_set_recent(app: AppHandle, hosts: Vec<TrayHost>) -> Result<(), AppError> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let menu = build_menu(&app, &hosts).map_err(internal)?;
        tray.set_menu(Some(menu)).map_err(internal)?;
    }
    Ok(())
}

/// Toggle the quick terminal (also bound to the global hotkey).
#[tauri::command]
#[specta::specta]
pub fn quick_toggle(app: AppHandle) {
    toggle_quick(&app);
}

/// Whether a window exists and is showing (tray and quick-terminal checks).
#[tauri::command]
#[specta::specta]
pub fn window_visible(app: AppHandle, label: String) -> bool {
    app.get_webview_window(&label)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Bring the main window to the front.
#[tauri::command]
#[specta::specta]
pub fn main_show(app: AppHandle) {
    show_main(&app);
}

/// Open a new window for tabs torn off another one. `handoff` (the tabs, their live
/// session ids and scrollback, as JSON) waits until the new window takes it.
#[tauri::command]
#[specta::specta]
pub async fn window_tear_off(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    handoff: String,
) -> Result<String, AppError> {
    if handoff.len() > 64 * 1024 * 1024 {
        return Err(AppError::invalid("Too much to move to a new window"));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let label = format!("tab-{}", &id[..12]);
    if let Ok(mut h) = state.handoffs.lock() {
        h.insert(label.clone(), handoff);
    }
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("Hopr")
        .inner_size(1000.0, 680.0)
        .min_inner_size(720.0, 460.0)
        .decorations(false)
        .build()
        .map_err(internal)?;
    Ok(label)
}

/// The tabs handed to this window, once.
#[tauri::command]
#[specta::specta]
pub fn window_handoff_take(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Option<String> {
    state
        .handoffs
        .lock()
        .ok()
        .and_then(|mut h| h.remove(window.label()))
}
