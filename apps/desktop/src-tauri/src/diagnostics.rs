//! Tauri commands for diagnostics + database backup (Phase 9).
//!
//! Two tools for when something goes wrong and for moving the
//! database around safely:
//!
//! - **Diagnostics export** — one JSON file under
//!   `<data_dir>/diagnostics/` with the app version, OS, git/LFS
//!   versions, database counts, storage sizes, the non-secret
//!   settings, and a redacted tail of the current log. It never
//!   contains credentials or repository contents: every free-text
//!   field goes through [`redact_secrets`], and the finished file
//!   is checked by [`verify_no_secrets`] before it is written — a
//!   failed scan writes nothing.
//! - **Database backup / restore** — backups are consistent
//!   snapshots (`VACUUM INTO`, not a file copy of a live WAL
//!   database) under `<data_dir>/backups/`. A restore first
//!   validates the file (SQLite header + `integrity_check` +
//!   `settings` table), stops the background agent (it holds the
//!   database open), and stages the file as
//!   `reponyx.sqlite.restore-pending`. The swap happens at the
//!   next startup, before the database is opened
//!   ([`apply_pending_restore`]); the replaced live database is
//!   kept as `reponyx.sqlite.pre-restore-<timestamp>`.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. The testable pieces (secret scan,
//! log tail, backup + validation, settings allowlist) live in
//! `reponyx-logging` and `reponyx-database` and are covered there.

use crate::agent::stop_agent_if_running;
use crate::auth::{CommandError, CommandResult, require_db};
use crate::repositories::open_with_os_app;
use crate::storage::{display_path, effective_storage_root};
use reponyx_core::{GitStatus, LfsStatus};
use reponyx_database::diagnostics::DiagnosticsCounts;
use reponyx_database::settings::list_settings;
use reponyx_database::{
    Database, backup_database, diagnostics_counts, is_safe_for_diagnostics, validate_backup_file,
};
use reponyx_git_engine::detect;
use reponyx_git_engine::storage::dir_size;
use reponyx_ipc::agent_reachable;
use reponyx_logging::{redact_secrets, tail_lines, verify_no_secrets};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExportDto {
    pub path: String,
    pub bytes: u64,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    pub path: String,
    pub bytes: u64,
    pub created_at: String,
}

/// The staged restore: the file is in place, but nothing changes
/// until RepoNyx restarts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreStagedDto {
    pub pending_path: String,
    pub agent_was_running: bool,
}

// -- The export file itself ------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsExport {
    generated_at: String,
    app_version: String,
    os: String,
    git: GitStatus,
    lfs: LfsStatus,
    agent_reachable: bool,
    counts: DiagnosticsCountsDto,
    storage: StorageSummaryDto,
    settings: Vec<SettingDto>,
    log_tail: Vec<String>,
    recent_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsCountsDto {
    accounts: i64,
    repositories: i64,
    synced: i64,
    syncing: i64,
    failed: i64,
    pending_jobs: i64,
    running_jobs: i64,
    failed_jobs: i64,
    schedules: i64,
    history_rows: i64,
    failed_history_7d: i64,
}

impl From<DiagnosticsCounts> for DiagnosticsCountsDto {
    fn from(c: DiagnosticsCounts) -> Self {
        Self {
            accounts: c.accounts,
            repositories: c.repositories,
            synced: c.synced,
            syncing: c.syncing,
            failed: c.failed,
            pending_jobs: c.pending_jobs,
            running_jobs: c.running_jobs,
            failed_jobs: c.failed_jobs,
            schedules: c.schedules,
            history_rows: c.history_rows,
            failed_history_7d: c.failed_history_7d,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageSummaryDto {
    root: String,
    mirrors_bytes: u64,
    database_bytes: u64,
    logs_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingDto {
    key: String,
    value: String,
}

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError {
        code: "InvalidInput".to_string(),
        message: message.into(),
    }
}

fn internal(message: impl Into<String>) -> CommandError {
    CommandError {
        code: "InternalError".to_string(),
        message: message.into(),
    }
}

/// `YYYYMMDD-HHMMSS` (UTC) for export and backup file names.
fn timestamp_now() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn rfc3339_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// The newest `reponyx.log*` file in the log directory, if any.
fn newest_log_file(log_dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(log_dir).ok()?;
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_ok_and(|t| t.is_file())
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("reponyx.log")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn diagnostics_dir() -> PathBuf {
    crate::dirs_data_dir().join("diagnostics")
}

fn backups_dir() -> PathBuf {
    crate::dirs_data_dir().join("backups")
}

/// Resolve `path` under `dir`, refusing anything outside it
/// (canonicalized on both sides, so `..` and symlinks cannot
/// escape). Used for backup delete/restore, where the path comes
/// from the UI.
fn resolve_under(dir: &Path, path: &str) -> Result<PathBuf, CommandError> {
    let dir = std::fs::canonicalize(dir)
        .map_err(|e| invalid(format!("cannot resolve {}: {e}", dir.display())))?;
    let target = std::fs::canonicalize(Path::new(path))
        .map_err(|_| invalid("that backup does not exist"))?;
    if !target.starts_with(&dir) {
        return Err(invalid("that path is not under the backups folder"));
    }
    Ok(target)
}

// -- Commands ---------------------------------------------------------

/// Build the diagnostics export and write it under
/// `<data_dir>/diagnostics/`. Nothing is written when the secret
/// scan fails.
#[tauri::command]
pub async fn export_diagnostics_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<DiagnosticsExportDto> {
    let database = require_db(&db)?;
    let generated_at = rfc3339_now();
    let counts = diagnostics_counts(&database.pool).await?;
    let (root, _) = effective_storage_root(database).await?;
    let data_dir = crate::dirs_data_dir();
    let log_dir = data_dir.join("logs");
    let settings: Vec<SettingDto> = list_settings(&database.pool)
        .await?
        .into_iter()
        .filter(|s| is_safe_for_diagnostics(&s.key))
        .map(|s| SettingDto {
            key: s.key,
            value: redact_secrets(&s.value),
        })
        .collect();
    let (log_tail, recent_errors) = match newest_log_file(&log_dir) {
        Some(log_file) => {
            let recent = tail_lines(&log_file, 500).unwrap_or_default();
            let tail: Vec<String> = recent
                .iter()
                .rev()
                .take(100)
                .rev()
                .map(|line| redact_secrets(line))
                .collect();
            let mut errors: Vec<String> = recent
                .iter()
                .filter(|line| line.contains("ERROR"))
                .map(|line| redact_secrets(line))
                .collect();
            // Keep only the last 50, in order. (A `.rev().take(50)
            // .rev()` chain does not compile here: `Take` is only
            // `DoubleEndedIterator` over an `ExactSizeIterator`,
            // and `Filter` is not one.)
            if errors.len() > 50 {
                errors.drain(..errors.len() - 50);
            }
            (tail, errors)
        }
        None => (Vec::new(), Vec::new()),
    };
    let export = DiagnosticsExport {
        generated_at: generated_at.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        git: detect::git_status().await,
        lfs: detect::lfs_status().await,
        agent_reachable: agent_reachable(&data_dir).await,
        counts: counts.into(),
        storage: StorageSummaryDto {
            root: display_path(&root),
            mirrors_bytes: dir_size(&root.join("repositories"))
                + dir_size(&root.join("working-clones")),
            database_bytes: std::fs::metadata(data_dir.join("data").join("reponyx.sqlite"))
                .map(|meta| meta.len())
                .unwrap_or(0),
            logs_bytes: dir_size(&log_dir),
        },
        settings,
        log_tail,
        recent_errors,
    };
    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| internal(format!("cannot serialize diagnostics: {e}")))?;
    if let Err(kind) = verify_no_secrets(&json) {
        return Err(internal(format!(
            "diagnostics export failed the secret scan ({kind}); nothing was written"
        )));
    }
    let dir = diagnostics_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| internal(format!("cannot create diagnostics folder: {e}")))?;
    let path = dir.join(format!("reponyx-diagnostics-{}.json", timestamp_now()));
    std::fs::write(&path, &json)
        .map_err(|e| internal(format!("cannot write diagnostics file: {e}")))?;
    let bytes = json.len() as u64;
    Ok(DiagnosticsExportDto {
        path: path.to_string_lossy().into_owned(),
        bytes,
        generated_at,
    })
}

/// Create a consistent database snapshot under
/// `<data_dir>/backups/`.
#[tauri::command]
pub async fn create_database_backup_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<BackupDto> {
    let database = require_db(&db)?;
    let dir = backups_dir();
    let path = dir.join(format!("reponyx-backup-{}.sqlite", timestamp_now()));
    let bytes = backup_database(&database.pool, &path).await?;
    Ok(BackupDto {
        path: path.to_string_lossy().into_owned(),
        bytes,
        created_at: rfc3339_now(),
    })
}

/// List the database backups, newest first.
#[tauri::command]
pub async fn list_database_backups_command() -> CommandResult<Vec<BackupDto>> {
    let dir = backups_dir();
    let mut backups = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_backup = entry.file_type().is_ok_and(|t| t.is_file())
                && path.extension().is_some_and(|ext| ext == "sqlite");
            if !is_backup {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let created_at = meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            backups.push(BackupDto {
                path: path.to_string_lossy().into_owned(),
                bytes: meta.len(),
                created_at,
            });
        }
    }
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

/// Delete one database backup.
#[tauri::command]
pub async fn delete_database_backup_command(path: String) -> CommandResult<()> {
    let target = resolve_under(&backups_dir(), &path)?;
    std::fs::remove_file(&target).map_err(|e| internal(format!("cannot delete backup: {e}")))?;
    Ok(())
}

/// Audit DB-m6: recover from a restore that was cut off between
/// moving the live database aside and moving the staged file into
/// place. If the live database is missing and a `pre-restore-*`
/// fallback exists, the newest fallback is the database from
/// before the restore — move it back so startup never sees an
/// empty data directory. If a staged pending file also survived,
/// the normal restore below then re-applies it on this startup.
fn recover_interrupted_restore(data_dir: &Path) {
    let data = data_dir.join("data");
    let live = data.join("reponyx.sqlite");
    if live.exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&data) else {
        return;
    };
    // Fallback names carry a `%Y%m%d-%H%M%S` stamp, so the
    // lexicographically largest name is the newest.
    let newest = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| {
                    name.to_string_lossy()
                        .starts_with("reponyx.sqlite.pre-restore-")
                })
                .unwrap_or(false)
        })
        .max();
    let Some(newest) = newest else {
        return;
    };
    match std::fs::rename(&newest, &live) {
        Ok(()) => tracing::warn!(
            fallback = %newest.display(),
            "recovered the database from an interrupted restore — the previous startup was cut off mid-restore"
        ),
        Err(e) => tracing::error!(
            error = %e,
            fallback = %newest.display(),
            "could not recover the database from an interrupted restore; the live database is missing"
        ),
    }
}

/// Validate a backup, stop the background agent, and stage the
#[tauri::command]
pub async fn restore_database_backup_command(path: String) -> CommandResult<RestoreStagedDto> {
    let target = resolve_under(&backups_dir(), &path)?;
    validate_backup_file(&target).await?;
    let agent_was_running = stop_agent_if_running().await?;
    let data_dir = crate::dirs_data_dir();
    let pending = data_dir.join("data").join("reponyx.sqlite.restore-pending");
    std::fs::copy(&target, &pending)
        .map_err(|e| internal(format!("cannot stage the restore: {e}")))?;
    Ok(RestoreStagedDto {
        pending_path: pending.to_string_lossy().into_owned(),
        agent_was_running,
    })
}

/// Open the diagnostics or backups folder in the OS file manager.
#[tauri::command]
pub async fn open_support_folder_command(folder: String) -> CommandResult<()> {
    let dir = match folder.as_str() {
        "diagnostics" => diagnostics_dir(),
        "backups" => backups_dir(),
        _ => return Err(invalid("unknown folder: expected diagnostics or backups")),
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| internal(format!("cannot create {}: {e}", dir.display())))?;
    open_with_os_app(&dir.to_string_lossy())
}

// -- Startup ----------------------------------------------------------

/// Apply a staged database restore, called before the database is
/// opened in `run()`. The pending file was validated when the
/// restore was staged; the header is checked again here, and the
/// replaced live database is kept as a `pre-restore` fallback.
/// Any failure leaves the live database untouched and logs the
/// reason — a failed restore must never lose the working database.
pub(crate) fn apply_pending_restore(data_dir: &Path) {
    // Audit DB-m6: a crash between the two renames below used to
    // leave no live database at all, and `open_database` then
    // created a fresh empty one while the real data sat in the
    // pre-restore fallback. Recover first, before anything else.
    recover_interrupted_restore(data_dir);
    let pending = data_dir.join("data").join("reponyx.sqlite.restore-pending");
    if !pending.exists() {
        return;
    }
    let live = data_dir.join("data").join("reponyx.sqlite");
    let head = match std::fs::read(&pending) {
        Ok(head) => head,
        Err(e) => {
            tracing::error!(error = %e, "pending database restore is unreadable; keeping the live database");
            return;
        }
    };
    if head.len() < 16 || &head[..16] != b"SQLite format 3\0" {
        tracing::error!("pending database restore is not a SQLite file; keeping the live database");
        return;
    }
    let fallback = data_dir
        .join("data")
        .join(format!("reponyx.sqlite.pre-restore-{}", timestamp_now()));
    if live.exists() {
        if let Err(e) = std::fs::rename(&live, &fallback) {
            tracing::error!(error = %e, "cannot move the live database aside for the pending restore; keeping it");
            return;
        }
    }
    // Stale WAL sidecars belong to the old database. The staged file
    // is a clean `VACUUM INTO` output with no pending frames, so drop
    // them rather than letting SQLite replay old frames into the
    // restored file.
    let _ = std::fs::remove_file(data_dir.join("data").join("reponyx.sqlite-wal"));
    let _ = std::fs::remove_file(data_dir.join("data").join("reponyx.sqlite-shm"));
    match std::fs::rename(&pending, &live) {
        Ok(()) => tracing::info!("restored the database from the staged backup"),
        Err(e) => {
            tracing::error!(error = %e, "could not apply the pending database restore; trying to put the live database back");
            if fallback.exists() {
                let _ = std::fs::rename(&fallback, &live);
            }
        }
    }
}
