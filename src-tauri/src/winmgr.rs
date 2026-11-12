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

/// Another Hopr window a tab can move to (tab menu, "Move to window").
#[derive(Serialize, Deserialize, Clone, Type)]
pub struct WindowEntry {
    pub label: String,
    /// The window's title: Hopr, then the tab in front.
    pub title: String,
}

/// The other app windows (not quick terminals), main first.
#[tauri::command]
#[specta::specta]
pub fn window_list(app: AppHandle, window: WebviewWindow) -> Vec<WindowEntry> {
    let mut out: Vec<WindowEntry> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, w)| {
            label != window.label() && is_app(label) && w.is_visible().unwrap_or(false)
        })
        .map(|(label, w)| WindowEntry {
            title: w.title().unwrap_or_else(|_| "Hopr".to_owned()),
            label,
        })
        .collect();
    out.sort_by_key(|e| (e.label != "main", e.label.clone()));
    out
}

/// Move tabs (a handoff, as for a tear-off) into another open window, and bring it up.
#[tauri::command]
#[specta::specta]
pub fn window_tab_send(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    handoff: String,
    to: String,
) -> Result<(), AppError> {
    if handoff.len() > MAX_HANDOFF {
        return Err(AppError::invalid("Too much to move to another window"));
    }
    let Some(w) = app.get_webview_window(&to).filter(|_| is_app(&to)) else {
        return Err(AppError::invalid("That window has closed"));
    };
    if let Ok(mut h) = state.handoffs.lock() {
        h.entry(to.clone()).or_default().push(handoff);
    }
    let _ = TabsArrivedEvent.emit_to(&app, &to);
    bring_up(&w);
    Ok(())
}

/// Longest a new window stays hidden waiting for its first frame.
const SHOW_FALLBACK: std::time::Duration = std::time::Duration::from_secs(4);

/// A new window opens hidden and the page shows it once it has painted (no black frame
/// while WebView2 starts). If the page never says so, it is shown anyway.
pub fn show_when_painted(w: WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_FALLBACK).await;
        if !w.is_visible().unwrap_or(true) {
            bring_up(&w);
        }
    });
}

/// A new window's size (logical pixels).
const SIZE: (f64, f64) = (1000.0, 680.0);

/// A rectangle: x, y, width, height (logical pixels).
type Rect = (f64, f64, f64, f64);

/// The usable area (screen minus taskbar) of the monitor showing a screen point, or the
/// nearest monitor's when the point is on none (e.g. past the taskbar edge).
fn work_area_at(app: &AppHandle, x: f64, y: f64) -> Option<Rect> {
    let areas: Vec<Rect> = app
        .available_monitors()
        .ok()?
        .iter()
        .map(|m| {
            let s = m.scale_factor();
            let a = m.work_area();
            (
                f64::from(a.position.x) / s,
                f64::from(a.position.y) / s,
                f64::from(a.size.width) / s,
                f64::from(a.size.height) / s,
            )
        })
        .collect();
    let gap = |r: &Rect| {
        let dx = (r.0 - x).max(0.0).max(x - (r.0 + r.2));
        let dy = (r.1 - y).max(0.0).max(y - (r.1 + r.3));
        dx.hypot(dy)
    };
    areas.into_iter().min_by(|a, b| gap(a).total_cmp(&gap(b)))
}

/// Place a window of `size` at `at`, moved (and shrunk if need be) to lie inside `area`.
fn fit(at: (f64, f64), size: (f64, f64), area: Option<Rect>) -> Rect {
    let Some((ax, ay, aw, ah)) = area else {
        return (at.0.max(0.0), at.1.max(0.0), size.0, size.1);
    };
    let w = size.0.min(aw);
    let h = size.1.min(ah);
    let x = at.0.clamp(ax, ax + aw - w);
    let y = at.1.clamp(ay, ay + ah - h);
    (x, y, w, h)
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
            .min_inner_size(720.0, 460.0)
            .decorations(false)
            .visible(false);
        b = match at {
            // Dropped on the taskbar or a screen edge: the window still opens fully on screen.
            Some((x, y)) => {
                let r = fit((x, y), SIZE, work_area_at(&app, x + 120.0, y + 18.0));
                b.position(r.0, r.1).inner_size(r.2, r.3)
            }
            None => b.center().inner_size(SIZE.0, SIZE.1),
        };
        match b.build() {
            Ok(w) => show_when_painted(w),
            Err(e) => eprintln!("hopr: window failed: {e}"),
        }
    });
}

#[cfg(test)]
mod fit_tests {
    use super::fit;

    const SCREEN: Option<(f64, f64, f64, f64)> = Some((0.0, 0.0, 1920.0, 1150.0));

    #[test]
    fn dropped_on_the_taskbar_opens_above_it() {
        assert_eq!(
            fit((800.0, 1170.0), (1000.0, 680.0), SCREEN),
            (800.0, 470.0, 1000.0, 680.0)
        );
    }

    #[test]
    fn off_the_left_or_right_edge_comes_back_on_screen() {
        assert_eq!(fit((-300.0, 100.0), (1000.0, 680.0), SCREEN).0, 0.0);
        assert_eq!(fit((1800.0, 100.0), (1000.0, 680.0), SCREEN).0, 920.0);
    }

    #[test]
    fn a_small_screen_shrinks_the_window() {
        let r = fit((0.0, 0.0), (1000.0, 680.0), Some((0.0, 0.0, 800.0, 600.0)));
        assert_eq!(r, (0.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn inside_the_screen_stays_where_dropped() {
        assert_eq!(
            fit((200.0, 150.0), (1000.0, 680.0), SCREEN),
            (200.0, 150.0, 1000.0, 680.0)
        );
    }
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
