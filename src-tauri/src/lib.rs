//! Thin Tauri glue: commands validate input, call core crates, map errors to `AppError`.

mod backups;
mod biometric;
mod clipboard;
mod commands;
mod foreign;
mod grouprun;
mod history;
mod library;
mod platform;
mod profiles;
mod recording;
mod sessions;
mod sftp;
mod snippets;
mod state;
mod sync;
mod tunnels;
mod updates;
mod vault;
mod workspaces;

use std::sync::Arc;

use tauri::Manager;

use tauri_specta::{Builder, collect_commands, collect_events};

/// Path of the generated TypeScript bindings, anchored to this crate (not the cwd).
pub const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/ipc/bindings.ts");

/// Every IPC command and event the UI may use. The single source of truth for bindings.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app_info,
            profiles::profiles_get,
            profiles::profile_create,
            profiles::profile_update,
            profiles::profile_set_look,
            profiles::profiles_set_ask_at_start,
            profiles::profile_delete,
            profiles::profile_switch,
            vault::vault_status,
            vault::vault_create,
            vault::vault_unlock,
            vault::vault_unlock_remembered,
            vault::vault_unlock_biometric,
            vault::vault_set_biometric,
            vault::vault_recover,
            vault::vault_change_password,
            vault::vault_lock,
            vault::vault_forget_device,
            vault::vault_activity,
            vault::vault_set_auto_lock,
            vault::vault_set_clipboard_clear,
            library::library_get,
            library::host_create,
            library::host_update,
            library::hosts_delete,
            library::hosts_restore,
            library::host_duplicate,
            library::hosts_bulk_edit,
            library::hosts_export,
            library::hosts_ping,
            library::host_copy_password,
            library::key_deploy,
            library::audit_list,
            library::host_set_favorite,
            library::group_create,
            library::group_rename,
            library::group_delete,
            library::key_generate,
            library::key_import,
            library::key_rename,
            library::key_set_certificate,
            library::key_delete,
            library::known_hosts_list,
            library::known_host_delete,
            library::shells_list,
            library::serial_ports,
            library::import_ssh_config_preview,
            library::import_ssh_config_apply,
            library::import_known_hosts,
            foreign::import_foreign_preview,
            foreign::import_foreign_apply,
            sessions::session_open,
            sessions::session_write,
            sessions::session_resize,
            sessions::session_ack,
            sessions::session_host_key_decision,
            sessions::session_prompt_response,
            sessions::session_detach,
            sessions::session_attach,
            sessions::session_close,
            workspaces::workspaces_list,
            workspaces::workspace_save,
            workspaces::workspace_autosave,
            workspaces::workspace_delete,
            workspaces::workspace_set_startup,
            backups::backups_list,
            backups::backups_dir,
            backups::backup_now,
            backups::backup_restore,
            backups::backup_restore_pending,
            platform::desktop_prefs,
            platform::desktop_set_close_to_tray,
            platform::desktop_set_quick_hotkey,
            platform::tray_set_recent,
            platform::quick_toggle,
            platform::main_show,
            platform::window_visible,
            platform::window_tear_off,
            platform::window_handoff_take,
            updates::update_check,
            updates::update_download,
            updates::update_install,
            sftp::sftp_open,
            sftp::sftp_list,
            sftp::sftp_resolve,
            sftp::sftp_mkdir,
            sftp::sftp_touch,
            sftp::sftp_rename,
            sftp::sftp_remove,
            sftp::sftp_chmod,
            sftp::sftp_preview,
            sftp::sftp_close,
            sftp::sftp_edit,
            sftp::sftp_edit_resolve,
            sftp::sftp_edit_stop,
            sftp::sftp_edit_path,
            sftp::transfers_upload,
            sftp::transfers_download,
            sftp::transfers_list,
            sftp::transfer_cancel,
            sftp::transfer_pause,
            sftp::transfer_resume,
            sftp::transfers_clear,
            sftp::transfers_settings,
            sftp::transfers_set_settings,
            sftp::local_list,
            sftp::local_home,
            history::history_settings,
            history::history_set_settings,
            history::history_add,
            history::history_list,
            history::history_delete,
            history::history_clear,
            history::connections_list,
            history::notify,
            recording::recording_start,
            recording::recording_stop,
            recording::recording_status,
            recording::recordings_list,
            recording::recording_read,
            recording::recording_delete,
            recording::recording_export,
            tunnels::tunnels_list,
            tunnels::tunnel_save,
            tunnels::tunnel_delete,
            tunnels::tunnel_start,
            tunnels::tunnel_stop,
            tunnels::tunnels_autostart,
            tunnels::port_free,
            tunnels::port_pick,
            snippets::snippets_list,
            snippets::snippet_save,
            snippets::snippet_delete,
            snippets::secrets_list,
            snippets::secret_set,
            snippets::secret_delete,
            snippets::session_type,
            grouprun::group_run,
            grouprun::group_run_cancel,
            sync::sync_status,
            sync::sync_enable,
            sync::sync_now,
            sync::sync_disable,
        ])
        .events(collect_events![
            sessions::SessionStateEvent,
            state::VaultLockedEvent,
            platform::TrayConnectEvent,
            sftp::Transfer,
            sftp::EditEvent,
            tunnels::TunnelStatsEvent,
            grouprun::GroupRunEvent
        ])
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
        .manage(updates::Pending::default())
        .manage(sftp::Files::default())
        .manage(history::HistoryState::default())
        .manage(tunnels::TunnelManager::default())
        .manage(grouprun::GroupRuns::default())
        .manage(sync::SyncLock::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| platform::on_shortcut(app, event.state))
                .build(),
        )
        .invoke_handler(builder.invoke_handler())
        .on_window_event(platform::on_window_event)
        .setup(move |app| {
            builder.mount_events(app);
            // HOPR_DATA_DIR lets tests (and portable installs) keep data out of the user's profile.
            let custom = std::env::var_os("HOPR_DATA_DIR");
            let root = match &custom {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            if custom.is_some() {
                state::keychain::set_data_scope(&root);
            }
            // Each profile is its own data folder; open the last used one.
            let profiles = profiles::Profiles::open(root);
            let data_dir = profiles.data_dir();
            std::fs::create_dir_all(&data_dir)?;
            state::keychain::set_profile(profiles.current());
            app.manage(profiles);
            let db_path = data_dir.join("hopr.db");
            // A restore staged before the last restart replaces the database now.
            if hopr_store::backup::apply_pending_restore(&db_path)? {
                eprintln!("hopr: restored the database from a backup");
            }
            let (state, close_to_tray, hotkey, transfer, history) =
                tauri::async_runtime::block_on(async {
                    let store = hopr_store::Store::open(&db_path).await?;
                    let _ = store.purge_deleted_hosts().await;
                    let _ = store.prune_history().await;
                    let history = store
                        .setting::<history::HistorySettings>(history::SETTINGS_KEY)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let minutes = store
                        .setting::<u32>(state::AUTO_LOCK_KEY)
                        .await?
                        .unwrap_or(state::AUTO_LOCK_DEFAULT);
                    let clear = store
                        .setting::<u32>(clipboard::CLEAR_KEY)
                        .await?
                        .unwrap_or(clipboard::CLEAR_DEFAULT);
                    let close_to_tray = store
                        .setting::<bool>(platform::CLOSE_TO_TRAY_KEY)
                        .await?
                        .unwrap_or(true);
                    let hotkey = store
                        .setting::<String>(platform::HOTKEY_KEY)
                        .await?
                        .unwrap_or_else(|| platform::HOTKEY_DEFAULT.to_owned());
                    let state = Arc::new(state::AppState::new(store, data_dir, minutes, clear));
                    let transfer = sftp::TransferSettings {
                        concurrency: state
                            .store
                            .setting(sftp::CONCURRENCY_KEY)
                            .await?
                            .unwrap_or(3),
                        limit_kbps: state.store.setting(sftp::LIMIT_KEY).await?.unwrap_or(0),
                    };
                    Ok::<_, hopr_store::StoreError>((
                        state,
                        close_to_tray,
                        hotkey,
                        transfer,
                        history,
                    ))
                })?;
            state::spawn_auto_lock(app.handle().clone(), state.clone());
            app.manage(state);
            app.state::<sftp::Files>().apply(&transfer);
            app.state::<history::HistoryState>().set(history);
            app.manage(platform::Platform::new(close_to_tray, hotkey));
            platform::init_tray(app.handle())?;
            platform::init_hotkey(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("error while running Hopr: {e}");
        std::process::exit(1);
    }
}
