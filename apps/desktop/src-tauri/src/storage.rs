//! Tauri commands for the Phase 8 Storage page.
//!
//! The page is read-only: it shows where the archives live, how much
//! space they take, and how full the volume is. Sizes are computed on
//! demand (a walk of every stored repository), so a first load can
//! take a moment on a large archive; the walk runs on a blocking
//! thread, off the async runtime. There is no delete action in v1 —
//! deleting a local archive is destructive and needs its own
//! confirm flow (the spec forbids silent deletion); the page notes
//! that instead of offering a half-working button.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works, and the CI windows-build job compiles this crate.

use crate::auth::{CommandError, CommandResult, require_db};
use crate::repositories::open_with_os_app;
use reponyx_database::Database;
use reponyx_database::repositories::{RepositoryFilter, list_repositories};
use reponyx_database::settings::get_setting;
use reponyx_git_engine::storage::{dir_size, disk_usage};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRepositoryDto {
    pub repository_id: String,
    pub account_id: String,
    pub full_name: String,
    pub local_path: String,
    pub bytes: u64,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageOverviewDto {
    pub root: String,
    pub root_configured: bool,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub mirrors_bytes: u64,
    pub database_bytes: u64,
    pub logs_bytes: u64,
    pub stored_repository_count: i64,
    pub repositories: Vec<StoredRepositoryDto>,
}

/// Effective storage root: the `storage.root` setting when it is set
/// and non-empty, otherwise the product data directory (the same
/// default the background agent seeds on start).
async fn effective_storage_root(database: &Database) -> Result<(PathBuf, bool), CommandError> {
    let setting = get_setting(&database.pool, "storage.root").await?;
    match setting {
        Some(setting) if !setting.value.trim().is_empty() => {
            Ok((PathBuf::from(setting.value), true))
        }
        _ => Ok((crate::dirs_data_dir(), false)),
    }
}

// -- Commands ---------------------------------------------------------

#[tauri::command]
pub async fn get_storage_overview_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<StorageOverviewDto> {
    let database = require_db(&db)?;
    let (root, root_configured) = effective_storage_root(database).await?;
    let repos = list_repositories(&database.pool, RepositoryFilter::default()).await?;
    let stored: Vec<(String, String, String, PathBuf, Option<String>)> = repos
        .iter()
        .filter_map(|repo| {
            repo.local_path.as_ref().map(|path| {
                (
                    repo.id.to_string(),
                    repo.account_id.to_string(),
                    repo.full_name.clone(),
                    PathBuf::from(path),
                    repo.last_synced_at.map(|at| at.to_rfc3339()),
                )
            })
        })
        .collect();

    let data_dir = crate::dirs_data_dir();
    let db_path = data_dir.join("data").join("reponyx.sqlite");
    let logs_dir = data_dir.join("logs");
    let paths: Vec<PathBuf> = stored.iter().map(|row| row.3.clone()).collect();
    let root_for_scan = root.clone();
    // Blocking filesystem walk — run off the async runtime.
    let scan = tokio::task::spawn_blocking(move || {
        let per_repo: Vec<u64> = paths.iter().map(|path| dir_size(path)).collect();
        let database_bytes = std::fs::metadata(&db_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        let logs_bytes = dir_size(&logs_dir);
        let disk = disk_usage(&root_for_scan).ok();
        (per_repo, database_bytes, logs_bytes, disk)
    })
    .await
    .map_err(|err| CommandError {
        code: "InternalError".to_string(),
        message: format!("storage scan failed: {err}"),
    })?;
    let (per_repo, database_bytes, logs_bytes, disk) = scan;

    let mut repositories: Vec<StoredRepositoryDto> = stored
        .into_iter()
        .zip(per_repo)
        .map(|(row, bytes)| StoredRepositoryDto {
            repository_id: row.0,
            account_id: row.1,
            full_name: row.2,
            local_path: row.3.to_string_lossy().into_owned(),
            bytes,
            last_synced_at: row.4,
        })
        .collect();
    repositories.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.full_name.cmp(&b.full_name)));
    let mirrors_bytes = repositories.iter().map(|repo| repo.bytes).sum();

    Ok(StorageOverviewDto {
        root: root.to_string_lossy().into_owned(),
        root_configured,
        total_bytes: disk.map(|usage| usage.total_bytes),
        available_bytes: disk.map(|usage| usage.available_bytes),
        mirrors_bytes,
        database_bytes,
        logs_bytes,
        stored_repository_count: repositories.len() as i64,
        repositories,
    })
}

/// Open the storage root in the OS file manager.
#[tauri::command]
pub async fn open_storage_root_command(db: State<'_, Option<Database>>) -> CommandResult<()> {
    let database = require_db(&db)?;
    let (root, _) = effective_storage_root(database).await?;
    open_with_os_app(&root.to_string_lossy())
}
