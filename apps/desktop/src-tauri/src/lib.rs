//! RepoNyx desktop — Tauri 2 backend library.
//!
//! Phase 1 commands only. They read real state (the SQLite database,
//! the installed Git) and return real results; a fresh install returns
//! zeros, never fabricated data.
//!
//! Phase 5 auth commands live in `auth.rs`.

mod agent;
mod auth;
mod broker;
mod config;
mod diagnostics;
mod exe;
mod repositories;
mod schedules;
mod settings;
mod storage;
mod synchronization;
mod tray;
mod web_auth;
mod commands {
    use crate::auth::{CommandResult, require_db};
    use reponyx_core::{DashboardCounts, GitStatus};
    use reponyx_database::Database;
    use serde::Serialize;
    use tauri::State;

    #[derive(Debug, Clone, Serialize)]
    pub struct AppInfo {
        pub name: String,
        pub version: String,
        pub description: String,
    }

    #[tauri::command]
    pub fn get_app_info() -> AppInfo {
        AppInfo {
            name: "RepoNyx".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Your Git. Your Machine. Always in Sync.".to_string(),
        }
    }

    #[tauri::command]
    pub async fn get_git_status() -> GitStatus {
        reponyx_git_engine::detect::git_status().await
    }

    #[tauri::command]
    pub async fn get_dashboard_counts(
        db: State<'_, Option<Database>>,
    ) -> CommandResult<DashboardCounts> {
        // Audit DB-M4: a failed count query must reach the
        // Overview page's error banner, never silently render
        // as all-zero counts.
        let database = require_db(&db)?;
        Ok(reponyx_database::dashboard_counts(&database.pool).await?)
    }
}

use reponyx_database::open_database;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Stop the services the desktop owns: the background agent (IPC
/// shutdown, then the lock-PID kill fallback) and a broker child
/// this desktop process spawned. Best-effort — every step logs
/// instead of failing, so quitting never hangs on a wedged service.
async fn stop_services(app: &AppHandle) {
    match agent::stop_agent_if_running().await {
        Ok(true) => tracing::info!("background agent stopped on quit"),
        Ok(false) => {}
        Err(e) => tracing::warn!(error = %e.message, "agent did not stop on quit"),
    }
    if broker::stop_broker_if_spawned(app).await {
        tracing::info!("auth broker stopped on quit");
    }
}

/// Whether the quit shutdown is already running. Guards the
/// ExitRequested → async-stop → exit(0) loop from re-entering
/// itself, and keeps the Exit handler from stopping twice.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs_data_dir();
    let log_dir = data_dir.join("logs");
    let _ = reponyx_logging::init_logging(&log_dir);

    // A staged database restore (from the Settings page) is applied
    // here, before the database opens — see `diagnostics.rs`. A
    // failure leaves the live database untouched.
    diagnostics::apply_pending_restore(&data_dir);

    // Open (and migrate) the database before the UI starts. A failure
    // here must not crash the app: commands fail loudly via
    // `require_db` so the UI shows the error instead of empty lists,
    // and the error is logged; the database layer is retried on next
    // launch.
    let db_path = data_dir.join("data").join("reponyx.sqlite");
    let db = tauri::async_runtime::block_on(async { open_database(&db_path).await });
    let db_state: Option<reponyx_database::Database> = match db {
        Ok(db) => {
            tracing::info!(path = %db_path.display(), "database opened");
            Some(db)
        }
        Err(e) => {
            tracing::error!(error = %e, path = %db_path.display(), "failed to open database");
            None
        }
    };

    tauri::Builder::default()
        // Single-instance first (its README: plugins run in builder
        // order). A second launch — e.g. the browser handing back
        // `reponyx://auth?code=...` after OAuth — forwards its args
        // to this instance instead of opening a second window; the
        // deep-link feature turns the URL into an `onOpenUrl` event
        // the frontend handles. The callback brings the window up.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(db_state)
        .manage(auth::AuthState::new())
        .manage(broker::BrokerProcess(std::sync::Mutex::new(None)))
        .setup(|app| {
            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::warn!(error = %e, "tray setup failed; continuing without tray");
            }
            // The `reponyx://` scheme is registered by the installer,
            // but `tauri dev` never registers it — and a dev run is
            // how this app is usually exercised. Without a runtime
            // registration the browser has nowhere to send the OAuth
            // result (`reponyx://auth?code=...`): the user authorizes
            // on GitHub/Google and the desktop login silently never
            // completes. Register the scheme for this running binary
            // (best-effort — a failure only warns; the app still
            // opens and email login still works). The installed app
            // re-registers itself on its own start, so the handler
            // heals back to the installed binary.
            #[cfg(target_os = "windows")]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register_all() {
                    tracing::warn!(
                        error = %e,
                        "could not register reponyx:// deep-link scheme; desktop OAuth login will not return to the app"
                    );
                }
            }
            agent::start_progress_forwarder(app.handle());
            // One system: the app brings its services up with it.
            // The agent and the broker start here (best-effort —
            // the app must still open if a service cannot start)
            // and stop on quit (see the RunEvent handler below).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match agent::ensure_agent_running(Some(&handle)).await {
                    Ok(view) if view.reachable => {
                        tracing::info!("background agent running at startup");
                    }
                    Ok(view) => {
                        tracing::warn!(
                            message = %view.message,
                            "background agent not running at startup"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e.message,
                            "could not start background agent at startup"
                        );
                    }
                }
                broker::ensure_broker_best_effort(&handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_git_status,
            commands::get_dashboard_counts,
            agent::agent_health_command,
            agent::agent_ping_command,
            agent::agent_pause_command,
            agent::agent_resume_command,
            agent::agent_enqueue_command,
            agent::agent_cancel_command,
            agent::ensure_agent_running_command,
            agent::stop_agent_command,
            agent::autostart_status_command,
            agent::register_agent_autostart_command,
            agent::unregister_agent_autostart_command,
            auth::list_accounts_command,
            auth::add_pat_account_command,
            auth::replace_pat_account_command,
            auth::remove_pat_account_command,
            auth::delete_pat_account_command,
            auth::reconnect_pat_account_command,
            auth::validate_pat_account_command,
            auth::get_pat_expiry_status_command,
            auth::get_broker_status_command,
            auth::list_broker_installations_command,
            auth::create_broker_setup_state_command,
            auth::start_pairing_command,
            auth::complete_pairing_command,
            auth::validate_github_app_account_command,
            auth::revoke_device_pairing_command,
            schedules::list_schedules_command,
            schedules::create_schedule_command,
            schedules::update_schedule_command,
            schedules::delete_schedule_command,
            schedules::preview_next_runs_command,
            schedules::materialize_due_command,
            schedules::effective_schedule_command,
            repositories::list_repositories_command,
            repositories::get_repository_names_command,
            repositories::discover_repositories_command,
            repositories::update_repository_command,
            repositories::delete_repository_command,
            repositories::open_repository_location_command,
            repositories::open_repository_in_browser_command,
            synchronization::list_sync_jobs_command,
            synchronization::count_sync_jobs_command,
            synchronization::cancel_sync_job_command,
            synchronization::list_recent_history_command,
            synchronization::list_history_for_repository_command,
            storage::get_storage_overview_command,
            storage::open_storage_root_command,
            storage::delete_orphan_command,
            storage::delete_local_archive_command,
            storage::move_archive_for_rename_command,
            diagnostics::export_diagnostics_command,
            diagnostics::create_database_backup_command,
            diagnostics::list_database_backups_command,
            diagnostics::delete_database_backup_command,
            diagnostics::restore_database_backup_command,
            diagnostics::open_support_folder_command,
            config::export_config_command,
            config::import_config_command,
            settings::get_settings_command,
            settings::set_setting_command,
            settings::clear_setting_command,
            web_auth::web_login_command,
            web_auth::web_verify_2fa_command,
            web_auth::web_session_status_command,
            web_auth::web_logout_command,
            web_auth::web_open_oauth_command,
            web_auth::web_open_register_command,
            web_auth::web_open_dashboard_command,
            web_auth::web_open_url_command,
            web_auth::web_exchange_desktop_code_command,
            web_auth::web_link_github_account_command,
            web_auth::web_link_pat_account_command,
            web_auth::web_unlink_pat_account_command,
        ])
        .build(tauri::generate_context!())
        .expect("error while building RepoNyx")
        .run(|app, event| match event {
            // Quit means everything stops together: prevent the exit,
            // stop the agent + broker, then exit for real. The guard
            // keeps the exit(0) below from re-entering this arm.
            tauri::RunEvent::ExitRequested { api, .. } => {
                if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
                    return;
                }
                api.prevent_exit();
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    stop_services(&handle).await;
                    handle.exit(0);
                });
            }
            // Exits that bypass ExitRequested (a direct exit call):
            // stop the services synchronously, bounded, best-effort.
            tauri::RunEvent::Exit => {
                if !SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
                    let handle = app.clone();
                    let _ = tauri::async_runtime::block_on(async {
                        tokio::time::timeout(Duration::from_secs(10), stop_services(&handle)).await
                    });
                }
            }
            _ => {}
        });
}

/// Platform app-data directory for RepoNyx, without depending on Tauri's
/// path resolver (needed before the app is built). Single source of
/// truth: `reponyx_database::paths` (audit DB-M3).
fn dirs_data_dir() -> PathBuf {
    reponyx_database::data_dir()
}
