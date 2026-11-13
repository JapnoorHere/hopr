//! Desktop integration (U5, T18): tray menu, close-to-tray, the global quick-terminal
//! hotkey and its window, and torn-off tab windows.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hopr_core::AppError;
use hopr_core::hotkey::to_accelerator;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window,
    WindowEvent, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_specta::Event;

use crate::device::Device;
use crate::state::{AppState, VaultLockedEvent};
use crate::winmgr;

/// Settings key: closing the main window keeps Hopr in the tray.
pub const CLOSE_TO_TRAY_KEY: &str = "app.close_to_tray";
/// Settings key: global quick-terminal hotkey ("" = off).
pub const HOTKEY_KEY: &str = "app.quick_hotkey";
/// Default quick-terminal hotkey (HOPR_PLAN §11).
pub const HOTKEY_DEFAULT: &str = "Ctrl+`";

const QUICK: &str = "quick";
/// `hopr --quick` opens only the quick terminal; Hopr itself opens from its "Open Hopr" button.
pub const QUICK_ARG: &str = "--quick";
/// The quick terminal's taskbar identity: its own button, pinnable apart from Hopr.
#[cfg(windows)]
const QUICK_APP_ID: &str = "app.hopr.desktop.quick";

/// Whether this run was started as the quick terminal alone.
pub fn quick_mode() -> bool {
    std::env::args().any(|a| a == QUICK_ARG)
}
const TRAY_ID: &str = "hopr";

/// Window-integration state.
pub struct Platform {
    close_to_tray: AtomicBool,
    hotkey: Mutex<String>,
}

impl Platform {
    pub fn new(close_to_tray: bool, hotkey: String) -> Self {
        Self {
            close_to_tray: AtomicBool::new(close_to_tray),
            hotkey: Mutex::new(hotkey),
        }
    }
}

fn show_quick(w: &WebviewWindow) {
    let _ = w.unminimize();
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

/// Bring the quick terminal to the front (opening it on first use), or hide it when it is
/// already in front. It is an ordinary window: it stays open when another window is clicked.
pub fn toggle_quick(app: &AppHandle) {
    if let Some(w) = winmgr::window_here(app, winmgr::is_quick) {
        let in_front = w.is_visible().unwrap_or(false)
            && !w.is_minimized().unwrap_or(false)
            && w.is_focused().unwrap_or(false);
        if in_front {
            let _ = w.hide();
            return;
        }
    }
    open_quick(app);
}

/// Bring the quick terminal to the front, opening it if it isn't open. With quick
/// terminals open only on other virtual desktops, a new one opens on this desktop.
pub fn open_quick(app: &AppHandle) {
    if let Some(w) = winmgr::window_here(app, winmgr::is_quick) {
        show_quick(&w);
        return;
    }
    let label = if app.get_webview_window(QUICK).is_some() {
        format!(
            "{QUICK}-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        )
    } else {
        QUICK.to_owned()
    };
    // Built off the event-loop thread: the hotkey handler runs on it, and creating a
    // window there would wait on itself.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let b = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()));
        let built = crate::webview::apply(b, &app)
            .title("Hopr quick terminal")
            .inner_size(900.0, 520.0)
            .min_inner_size(480.0, 280.0)
            .center()
            .decorations(false)
            .visible(false)
            .build();
        match built {
            Ok(w) => {
                #[cfg(windows)]
                {
                    let tagged = w.clone();
                    let _ = w.run_on_main_thread(move || set_quick_taskbar_identity(&tagged));
                }
                show_quick(&w);
            }
            Err(e) => eprintln!("hopr: quick terminal window failed: {e}"),
        }
    });
}

/// Give the quick terminal its own taskbar button: pinning it relaunches `hopr --quick`,
/// which opens only the terminal. Runs on the main thread (the property store needs COM).
#[cfg(windows)]
#[allow(unsafe_code)] // Win32 property store: the one place Hopr needs unsafe (CODING-STANDARDS).
fn set_quick_taskbar_identity(w: &WebviewWindow) {
    use windows::Win32::Foundation::{HWND, PROPERTYKEY};
    use windows::Win32::Storage::EnhancedStorage::{
        PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
        PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
    };
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow,
    };
    use windows::Win32::UI::Shell::SHStrDupW;
    use windows::core::HSTRING;

    let (Ok(hwnd), Ok(exe)) = (w.hwnd(), std::env::current_exe()) else {
        return;
    };
    let exe = exe.display().to_string();
    // The relaunch details first: the taskbar reads them when the id is set.
    let props: [(PROPERTYKEY, String); 4] = [
        (
            PKEY_AppUserModel_RelaunchCommand,
            format!("\"{exe}\" {QUICK_ARG}"),
        ),
        (
            PKEY_AppUserModel_RelaunchDisplayNameResource,
            "Hopr quick terminal".to_owned(),
        ),
        (PKEY_AppUserModel_RelaunchIconResource, format!("{exe},0")),
        (PKEY_AppUserModel_ID, QUICK_APP_ID.to_owned()),
    ];
    // SAFETY: the window handle is live (we're on its thread); each PROPVARIANT owns a
    // CoTaskMem string from SHStrDupW and is cleared after the store has copied it.
    unsafe {
        let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(HWND(hwnd.0)) else {
            return;
        };
        for (key, value) in props {
            let Ok(text) = SHStrDupW(&HSTRING::from(value)) else {
                return;
            };
            let mut pv = PROPVARIANT::default();
            (*pv.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*pv.Anonymous.Anonymous).Anonymous.pwszVal = text;
            if let Err(e) = store.SetValue(&key, &pv) {
                eprintln!("hopr: quick terminal taskbar identity: {e}");
            }
            let _ = PropVariantClear(&mut pv);
        }
        let _ = store.Commit();
    }
}

/// Bring the main window back (from the tray or the quick terminal), opening it if this run
/// started as the quick terminal alone.
pub fn show_main(app: &AppHandle) {
    // A Hopr window on the desktop in front; with Hopr open only on other desktops, a new
    // window here instead of switching desktops.
    if let Some(w) = winmgr::window_here(app, winmgr::is_app) {
        winmgr::bring_up(&w);
        return;
    }
    if app.get_webview_window("main").is_some() {
        winmgr::open_app_window(app);
        return;
    }
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
    else {
        return;
    };
    // Built off the event-loop thread, like the quick terminal.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::from_config(&app, &config)
            .and_then(|b| crate::webview::apply(b, &app).build());
        match built {
            Ok(w) => winmgr::show_when_painted(w),
            Err(e) => eprintln!("hopr: main window failed: {e}"),
        }
    });
}

/// Hopr was launched again while running: bring up what that launch asked for, here.
pub fn on_second_instance(app: &AppHandle, args: &[String]) {
    if args.iter().any(|a| a == QUICK_ARG) {
        open_quick(app);
    } else {
        show_main(app);
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

/// Window events: main closes to the tray.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let ("main", WindowEvent::CloseRequested { api, .. }) = (window.label(), event) {
        let platform = window.state::<Platform>();
        if platform.close_to_tray.load(Ordering::Relaxed) {
            api.prevent_close();
            let _ = window.hide();
        } else {
            // Main closing means quitting: take the other windows with it.
            window.app_handle().exit(0);
        }
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
    device: State<'_, Device>,
    platform: State<'_, Platform>,
    enabled: bool,
) -> Result<(), AppError> {
    device.update(|d| d.close_to_tray = Some(enabled))?;
    platform.close_to_tray.store(enabled, Ordering::Relaxed);
    Ok(())
}

/// Change the global quick-terminal hotkey ("" turns it off). If the new one can't be
/// registered, the old one is put back.
#[tauri::command]
#[specta::specta]
pub async fn desktop_set_quick_hotkey(
    app: AppHandle,
    device: State<'_, Device>,
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
    device.update(|d| d.hotkey = Some(chord.clone()))?;
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

/// Whether a window exists and is showing (the end-to-end suite checks the tray and the quick
/// terminal with it).
#[tauri::command]
#[specta::specta]
pub fn window_visible(app: AppHandle, label: String) -> bool {
    app.get_webview_window(&label)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Restart Hopr, first noting why in `restarts.log` (the app's log folder), so a restart
/// nobody expected can be traced.
pub fn restart(app: &AppHandle, reason: &str) -> ! {
    if let Ok(dir) = app.path().app_log_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let line = format!("{secs} {reason}\n");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("restarts.log"))
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
    // Under `pnpm dev:app`, exit and let its loop start Hopr again: restarting in place would end
    // the process `tauri dev` watches and take the dev server with it.
    if cfg!(debug_assertions) && std::env::var_os("HOPR_DEV_LOOP").is_some() {
        let _ = std::fs::write(std::env::temp_dir().join("hopr-dev-restart"), reason);
        std::process::exit(0);
    }
    app.restart()
}

/// Restart from the UI (after a restore is staged), with the reason logged.
#[tauri::command]
#[specta::specta]
pub fn app_restart(app: AppHandle, reason: String) {
    restart(&app, &reason)
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
        h.entry(label.clone()).or_default().push(handoff);
    }
    winmgr::build_window(&app, &label, None);
    Ok(label)
}

/// The next tabs handed to this window (a tear-off, or tabs dropped on it), once each.
#[tauri::command]
#[specta::specta]
pub fn window_handoff_take(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Option<String> {
    let mut all = state.handoffs.lock().ok()?;
    let queue = all.get_mut(window.label())?;
    let next = (!queue.is_empty()).then(|| queue.remove(0));
    if queue.is_empty() {
        all.remove(window.label());
    }
    next
}
