//! Thin Tauri glue: commands validate input, call core crates, map errors to `AppError`.

mod commands;
mod sessions;

use tauri_specta::{Builder, collect_commands, collect_events};

/// Path of the generated TypeScript bindings, anchored to this crate (not the cwd).
pub const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/ipc/bindings.ts");

/// Every IPC command and event the UI may use. The single source of truth for bindings.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app_info,
            sessions::session_open,
            sessions::session_write,
            sessions::session_resize,
            sessions::session_ack,
            sessions::session_close,
        ])
        .events(collect_events![sessions::SessionStateEvent])
}

/// Write `src/lib/ipc/bindings.ts`.
pub fn export_bindings() -> Result<(), String> {
    specta_builder()
        .export(
            specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */"),
            BINDINGS_PATH,
        )
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `hopr --export-bindings` (used by `just bindings` and CI) writes the TS file and exits.
    // A unit test can't do this on Windows: test binaries lack the app manifest the webview needs.
    if std::env::args().any(|a| a == "--export-bindings") {
        match export_bindings() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("failed to export TS bindings: {e}");
                std::process::exit(1);
            }
        }
    }

    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings() {
        eprintln!("failed to export TS bindings: {e}");
    }

    let result = tauri::Builder::default()
        .manage(sessions::SessionManager::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("error while running Hopr: {e}");
        std::process::exit(1);
    }
}
