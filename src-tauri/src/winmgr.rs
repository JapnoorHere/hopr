//! Hopr's windows: which window to bring up on the virtual desktop in front, and tabs dragged
//! out of a window (merged into the window under the pointer, or a new window there).
//!
//! Window kinds, by label: `main`; `win-*`, another full window (opened on a virtual desktop
//! with no Hopr window); `tab-*`, a window holding torn-off tabs (it closes with its last
//! tab); `quick` and `quick-*`, quick terminals.

use std::sync::Arc;

use hopr_core::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::state::AppState;

/// Height of the tab bar a dropped tab merges into (logical pixels from the window's top).
const TAB_BAR: f64 = 48.0;
/// Biggest handoff accepted (scrollback for every pane of a tab).
const MAX_HANDOFF: usize = 64 * 1024 * 1024;

pub fn is_quick(label: &str) -> bool {
    label == "quick" || label.starts_with("quick-")
}

/// A window that shows the app with tabs (not a quick terminal).
pub fn is_app(label: &str) -> bool {
    label == "main" || label.starts_with("win-") || label.starts_with("tab-")
}

fn new_label(prefix: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}-{}", &id[..12])
}

/// Whether a window is on the virtual desktop in front. True off Windows, and whenever
/// Windows can't tell.
#[cfg(windows)]
#[allow(unsafe_code)] // IVirtualDesktopManager has no safe API (CODING-STANDARDS §3.4).
fn on_this_desktop(w: &WebviewWindow) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
    let Ok(hwnd) = w.hwnd() else {
        return true;
    };
    // SAFETY: COM is initialised on the event-loop thread these run on (WebView2 needs it),
    // the handle is a live window, and the manager is released when dropped.
    unsafe {
        CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
            .and_then(|m| m.IsWindowOnCurrentVirtualDesktop(HWND(hwnd.0)))
            .map_or(true, |on| on.as_bool())
    }
}

#[cfg(not(windows))]
fn on_this_desktop(_: &WebviewWindow) -> bool {
    true
}

/// A window of a kind to bring up here: one on the desktop in front, focused first, then
/// visible ones. Never a visible one on another desktop: focusing it would switch desktops.
/// A hidden one (closed to the tray) counts as here: it is shown where the user is.
pub fn window_here(app: &AppHandle, kind: impl Fn(&str) -> bool) -> Option<WebviewWindow> {
    let mut found: Vec<WebviewWindow> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, w)| {
            kind(label) && (!w.is_visible().unwrap_or(false) || on_this_desktop(w))
        })
        .map(|(_, w)| w)
        .collect();
    let rank = |w: &WebviewWindow| {
        (
            !w.is_focused().unwrap_or(false),
            !w.is_visible().unwrap_or(false),
        )
    };
    found.sort_by_key(rank);
    found.into_iter().next()
}

pub fn bring_up(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

/// A new full Hopr window on the desktop in front (`win-*`).
pub fn open_app_window(app: &AppHandle) {
    build_window(app, &new_label("win"), None);
}

/// The app window (other than `from`) whose tab bar is under a screen point.
fn tab_bar_at(app: &AppHandle, from: &str, x: f64, y: f64) -> Option<WebviewWindow> {
    let mut hits: Vec<WebviewWindow> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, w)| {
            label != from
                && is_app(label)
                && w.is_visible().unwrap_or(false)
                && !w.is_minimized().unwrap_or(false)
        })
        .map(|(_, w)| w)
        .filter(|w| {
            let scale = w.scale_factor().unwrap_or(1.0);
            let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else {
                return false;
            };
            let (pos, size) = (pos.to_logical::<f64>(scale), size.to_logical::<f64>(scale));
            x >= pos.x && x <= pos.x + size.width && y >= pos.y && y <= pos.y + TAB_BAR
        })
        .collect();
    // Overlapping windows: the focused one is the one on top.
    hits.sort_by_key(|w| !w.is_focused().unwrap_or(false));
    hits.into_iter().next()
}

/// Tabs arrived in this window (merged by a drag): take them with `window_handoff_take`.
#[derive(Serialize, Deserialize, Clone, Type, Event)]
pub struct TabsArrivedEvent;

/// Tabs dragged out of their window and dropped at screen point (x, y), logical pixels:
/// into the tab bar of the Hopr window there, or a new window at that spot. `handoff` is
/// the tabs with their live session ids and scrollback (JSON), as for a tear-off.
/// Returns the label of the window that takes them.
#[tauri::command]
#[specta::specta]
pub fn window_tab_drop(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    handoff: String,
    x: f64,
    y: f64,
) -> Result<String, AppError> {
    if handoff.len() > MAX_HANDOFF {
        return Err(AppError::invalid("Too much to move to another window"));
    }
    let target = tab_bar_at(&app, window.label(), x, y);
    let label = match &target {
        Some(w) => w.label().to_owned(),
        // The new window opens under the pointer, the tab roughly where it was held.
        None => new_label("tab"),
    };
    if let Ok(mut h) = state.handoffs.lock() {
        h.entry(label.clone()).or_default().push(handoff);
    }
    match target {
        Some(w) => {
            let _ = TabsArrivedEvent.emit_to(&app, &label);
            bring_up(&w);
        }
        None => build_window(&app, &label, Some((x - 120.0, y - 18.0))),
    }
    Ok(label)
}

/// A full Hopr window (any handoff for it is already stored), centred or at a screen point.
/// Built off the event-loop thread: callers may run on it, and creating a window there
/// would wait on itself.
pub fn build_window(app: &AppHandle, label: &str, at: Option<(f64, f64)>) {
    let app = app.clone();
    let name = label.to_owned();
    tauri::async_runtime::spawn(async move {
        let mut b = WebviewWindowBuilder::new(&app, &name, WebviewUrl::App("index.html".into()))
            .title("Hopr")
            .inner_size(1000.0, 680.0)
            .min_inner_size(720.0, 460.0)
            .decorations(false);
        b = match at {
            Some((x, y)) => b.position(x.max(0.0), y.max(0.0)),
            None => b.center(),
        };
        match b.build() {
            Ok(w) => {
                let _ = w.set_focus();
            }
            Err(e) => eprintln!("hopr: window failed: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_kinds() {
        assert!(is_quick("quick") && is_quick("quick-1a2b"));
        assert!(!is_quick("main") && !is_quick("quicker"));
        for app in ["main", "win-abc", "tab-abc"] {
            assert!(is_app(app), "{app}");
        }
        assert!(!is_app("quick") && !is_app("settings"));
    }
}
