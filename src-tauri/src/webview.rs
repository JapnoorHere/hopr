//! The web views' shared settings: where WebView2 keeps its data and how much cache it may
//! use. Every Hopr window must be built with the same values (WebView2 refuses to mix
//! settings in one data folder).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, Runtime, WebviewWindowBuilder};

/// WebView2's own defaults (no mini menu, no SmartScreen on local pages), plus a 32 MiB
/// limit on the HTTP cache: Hopr's pages ship inside the app, so a big cache buys nothing.
const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disk-cache-size=33554432";

/// Caches WebView2 rebuilds on its own; cleared when the app version changes, since they
/// only hold copies of the previous version's pages.
const CACHES: [&str; 3] = ["Cache", "Code Cache", "GPUCache"];

/// Where the web views keep their data: dev builds use their own folder, so the many
/// separate files the dev server serves never fill the installed app's cache.
pub fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let base = app.path().app_local_data_dir().ok()?;
    Some(if cfg!(debug_assertions) {
        base.join("dev")
    } else {
        base
    })
}

/// Apply the shared settings to a window being built.
pub fn apply<'a, R: Runtime, M: Manager<R>>(
    b: WebviewWindowBuilder<'a, R, M>,
    app: &AppHandle<R>,
) -> WebviewWindowBuilder<'a, R, M> {
    let b = b.additional_browser_args(BROWSER_ARGS);
    match data_dir(app) {
        Some(dir) => b.data_directory(dir),
        None => b,
    }
}

/// At start-up, before any window: a new app version drops the old version's caches.
pub fn clear_stale_caches<R: Runtime>(app: &AppHandle<R>) {
    if let Some(dir) = data_dir(app) {
        clear_if_new_version(&dir, env!("CARGO_PKG_VERSION"));
    }
}

fn clear_if_new_version(dir: &Path, version: &str) {
    let marker = dir.join("hopr-cache-version");
    if std::fs::read_to_string(&marker).is_ok_and(|v| v.trim() == version) {
        return;
    }
    let profile = dir.join("EBWebView").join("Default");
    for name in CACHES {
        let _ = std::fs::remove_dir_all(profile.join(name));
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&marker, version);
}

#[cfg(test)]
mod tests {
    use super::clear_if_new_version;

    #[test]
    fn a_new_version_clears_the_caches_once() {
        let dir = std::env::temp_dir().join(format!("hopr-wv-{}", uuid::Uuid::new_v4().simple()));
        let cache = dir.join("EBWebView").join("Default").join("Code Cache");
        let keep = dir.join("EBWebView").join("Default").join("Local Storage");
        for d in [&cache, &keep] {
            std::fs::create_dir_all(d).unwrap();
        }
        clear_if_new_version(&dir, "1.0.0");
        assert!(!cache.exists(), "old cache cleared");
        assert!(keep.exists(), "settings kept");

        std::fs::create_dir_all(&cache).unwrap();
        clear_if_new_version(&dir, "1.0.0");
        assert!(cache.exists(), "same version: nothing cleared");

        clear_if_new_version(&dir, "1.0.1");
        assert!(!cache.exists(), "next version clears again");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
