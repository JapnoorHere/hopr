//! Thin Tauri glue: commands validate input, call core crates, map errors to `AppError`.

mod backups;
mod clipboard;
mod commands;
mod device;
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
mod webview;
mod winmgr;
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
            vault::vault_recover,
            vault::vault_change_password,
            vault::vault_lock,
            vault::vault_forget_device,
            vault::vault_activity,
            vault::vault_set_auto_lock,
            clipboard::clipboard_read_text,
            vault::vault_set_clipboard_clear,
            library::library_get,
            library::hosts::host_create,
            library::hosts::host_update,
            library::hosts::hosts_delete,
            library::hosts::hosts_restore,
            library::hosts::host_duplicate,
            library::hosts::hosts_bulk_edit,
            library::hosts::hosts_export,
            library::hosts::hosts_ping,
            library::hosts::host_copy_password,
            library::keys::key_deploy,
            library::audit_list,
            library::hosts::host_set_favorite,
            library::hosts::group_create,
            library::hosts::group_rename,
            library::hosts::group_delete,
            library::keys::key_generate,
            library::keys::key_import,
            library::keys::key_rename,
            library::keys::key_set_certificate,
            library::keys::key_delete,
            library::keys::known_hosts_list,
            library::keys::known_host_delete,
            library::system::shells_list,
            library::system::serial_ports,
            library::import::import_ssh_config_preview,
            library::import::import_ssh_config_apply,
            library::import::import_known_hosts,
            foreign::import_foreign_preview,
            foreign::import_foreign_apply,
            sessions::commands::session_open,
            sessions::commands::session_write,
            sessions::commands::session_resize,
            sessions::commands::session_ack,
            sessions::commands::session_host_key_decision,
            sessions::commands::session_prompt_response,
            sessions::commands::session_detach,
            sessions::commands::session_attach,
            sessions::commands::session_close,
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
            platform::app_restart,
            platform::window_tear_off,
            platform::window_handoff_take,
            winmgr::window_tab_drop,
            winmgr::window_list,
            winmgr::window_tab_send,
            updates::update_check,
            updates::update_download,
            updates::update_install,
            sftp::browse::sftp_open,
            sftp::browse::sftp_list,
            sftp::browse::sftp_resolve,
            sftp::browse::sftp_mkdir,
            sftp::browse::sftp_touch,
            sftp::browse::sftp_rename,
            sftp::browse::sftp_remove,
            sftp::browse::sftp_chmod,
            sftp::browse::sftp_preview,
            sftp::browse::sftp_close,
            sftp::edit::sftp_edit,
            sftp::edit::sftp_edit_resolve,
            sftp::edit::sftp_edit_stop,
            sftp::edit::sftp_edit_path,
            sftp::enqueue::transfers_upload,
            sftp::enqueue::transfers_download,
            sftp::transfers::transfers_list,
            sftp::transfers::transfer_cancel,
            sftp::transfers::transfer_pause,
            sftp::transfers::transfer_resume,
            sftp::transfers::transfers_clear,
            sftp::transfers::transfers_settings,
            sftp::transfers::transfers_set_settings,
            sftp::local::local_list,
            sftp::local::local_home,
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
            tunnels::commands::tunnels_list,
            tunnels::commands::tunnel_save,
            tunnels::commands::tunnel_delete,
            tunnels::commands::tunnel_start,
            tunnels::commands::tunnel_stop,
            tunnels::commands::tunnels_autostart,
            tunnels::commands::port_free,
            tunnels::commands::port_pick,
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
            winmgr::TabsArrivedEvent,
            state::VaultLockedEvent,
            platform::TrayConnectEvent,
            sftp::Transfer,
            sftp::edit::EditEvent,
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

/// Debug builds keep the TS bindings current at start, but write the file only when it
/// changed: rewriting it unchanged makes the dev server reload the page a second time.
#[cfg(debug_assertions)]
fn refresh_bindings() {
    let fresh = std::env::temp_dir().join("hopr-bindings.ts");
    let exported = specta_builder().export(
        specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */"),
        &fresh,
    );
    if let Err(e) = exported {
        eprintln!("failed to export TS bindings: {e}");
        return;
    }
    let (Ok(new), current) = (std::fs::read(&fresh), std::fs::read(BINDINGS_PATH)) else {
        return;
    };
    if current.ok().as_deref() != Some(new.as_slice())
        && let Err(e) = std::fs::write(BINDINGS_PATH, new)
    {
        eprintln!("failed to write TS bindings: {e}");
    }
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
    refresh_bindings();

    let quick_mode = platform::quick_mode();
    let mut app_builder = tauri::Builder::default();
    // One Hopr at a time: launching it again (or the pinned quick terminal) hands over to the
    // running copy. Dev builds and HOPR_DATA_DIR runs (tests, portable) stay independent.
    if cfg!(not(debug_assertions)) && std::env::var_os("HOPR_DATA_DIR").is_none() {
        app_builder = app_builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            platform::on_second_instance(app, &args);
        }));
    }
    let result = app_builder
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
            sftp::edit::clear_temp();
            webview::clear_stale_caches(app.handle());
            // HOPR_DATA_DIR lets tests (and portable installs) keep data out of the user's profile.
            let custom = std::env::var_os("HOPR_DATA_DIR");
            let root = match &custom {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            if custom.is_some() {
                state::keychain::set_data_scope(&root);
            }
            // Window, hotkey and transfer settings belong to this computer, not a profile.
            let device = device::Device::open(root.clone());
            // Each profile is its own data folder; open the last used one.
            let profiles = profiles::Profiles::open(root);
            if quick_mode {
                profiles.skip_ask();
            }
            let data_dir = profiles.data_dir();
            std::fs::create_dir_all(&data_dir)?;
            state::keychain::set_profile(&profiles.current());
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
                    // Device settings; one not saved there yet comes from this profile once.
                    let saved = device.get();
                    let close_to_tray = match saved.close_to_tray {
                        Some(v) => v,
                        None => store
                            .setting::<bool>(platform::CLOSE_TO_TRAY_KEY)
                            .await?
                            .unwrap_or(true),
                    };
                    let hotkey = match saved.hotkey {
                        Some(v) => v,
                        None => store
                            .setting::<String>(platform::HOTKEY_KEY)
                            .await?
                            .unwrap_or_else(|| platform::HOTKEY_DEFAULT.to_owned()),
                    };
                    let state = Arc::new(state::AppState::new(store, data_dir, minutes, clear));
                    let transfer = sftp::TransferSettings {
                        concurrency: match saved.transfer_concurrency {
                            Some(v) => v,
                            None => state
                                .store
                                .setting(sftp::CONCURRENCY_KEY)
                                .await?
                                .unwrap_or(3),
                        },
                        limit_kbps: match saved.transfer_limit_kbps {
                            Some(v) => v,
                            None => state.store.setting(sftp::LIMIT_KEY).await?.unwrap_or(0),
                        },
                    };
                    Ok::<_, hopr_store::StoreError>((
                        state,
                        close_to_tray,
                        hotkey,
                        transfer,
                        history,
                    ))
                })?;
            device.update(|d| {
                d.close_to_tray = Some(close_to_tray);
                d.hotkey = Some(hotkey.clone());
                d.transfer_concurrency = Some(transfer.concurrency);
                d.transfer_limit_kbps = Some(transfer.limit_kbps);
            })?;
            app.manage(device);
            state::spawn_auto_lock(app.handle().clone(), state.clone());
            app.manage(state);
            app.state::<sftp::Files>().apply(&transfer);
            app.state::<history::HistoryState>().set(history);
            app.manage(platform::Platform::new(close_to_tray, hotkey));
            platform::init_tray(app.handle())?;
            platform::init_hotkey(app.handle());
            // `hopr --quick` opens only the quick terminal; Hopr opens from its button.
            if quick_mode {
                platform::open_quick(app.handle());
            } else {
                platform::show_main(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                sftp::edit::clear_temp();
            }
        }),
        Err(e) => {
            eprintln!("error while running Hopr: {e}");
            std::process::exit(1);
        }
    }
}
