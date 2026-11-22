//! RepoNyx desktop — Tauri 2 backend library.
//!
//! Phase 1 commands only. They read real state (the SQLite database,
//! the installed Git) and return real results; a fresh install returns
//! zeros, never fabricated data.
//!
//! Phase 5 auth commands live in `auth.rs`.

mod agent;
mod auth;
mod repositories;
mod schedules;
mod storage;
mod synchronization;
mod tray;
mod commands {
    use crate::auth::CommandResult;
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
        Ok(match db.inner() {
            Some(database) => reponyx_database::dashboard_counts(&database.pool)
                .await
                .unwrap_or_default(),
            None => DashboardCounts::default(),
        })
    }
}

use reponyx_database::open_database;
use std::path::PathBuf;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs_data_dir();
    let log_dir = data_dir.join("logs");
    let _ = reponyx_logging::init_logging(&log_dir);

    // Open (and migrate) the database before the UI starts. A failure
    // here must not crash the app: the UI reports zero counts and the
    // error is logged; the database layer is retried on next launch.
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(db_state)
        .manage(auth::AuthState::new())
        .setup(|app| {
            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::warn!(error = %e, "tray setup failed; continuing without tray");
            }
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
            agent::register_agent_autostart_command,
            agent::unregister_agent_autostart_command,
            auth::list_accounts_command,
            auth::add_pat_account_command,
            auth::replace_pat_account_command,
            auth::remove_pat_account_command,
            auth::reconnect_pat_account_command,
            auth::validate_pat_account_command,
            auth::get_pat_expiry_status_command,
            auth::set_broker_setup_token_command,
            auth::get_broker_status_command,
            auth::list_broker_installations_command,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running RepoNyx");
}

/// Platform app-data directory for RepoNyx, without depending on Tauri's
/// path resolver (needed before the app is built).
fn dirs_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("REPONYX_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("RepoNyx");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("RepoNyx");
    }
    PathBuf::from(".reponyx-data")
}
