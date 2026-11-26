//! Diagnostics counts + database backup/restore helpers (Phase 9).
//!
//! - [`diagnostics_counts`] — the numbers the diagnostics export reports.
//! - [`backup_database`] — a consistent snapshot via `VACUUM INTO`.
//!   A plain file copy of a live WAL database can catch the WAL
//!   mid-checkpoint; `VACUUM INTO` writes a complete, standalone
//!   database instead, and works while the database is open.
//! - [`validate_backup_file`] — header check + `PRAGMA integrity_check`
//!   on a throwaway read-only connection, so a restore never stages
//!   a corrupt file. It also requires the `settings` table, so a
//!   random SQLite file is not accepted as a RepoNyx backup.
//! - [`is_safe_for_diagnostics`] — the settings allowlist for the
//!   export: only the four seeded non-secret keys. Everything else
//!   (device identifiers, anything added later) stays out.

use crate::common::{invalid, map_error};
use reponyx_core::RepoNyxError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::Path;

/// The numbers the diagnostics export reports. All counts are real
/// queries against the live database; a fresh database returns zeros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsCounts {
    pub accounts: i64,
    pub repositories: i64,
    pub synced: i64,
    pub syncing: i64,
    pub failed: i64,
    pub pending_jobs: i64,
    pub running_jobs: i64,
    pub failed_jobs: i64,
    pub schedules: i64,
    pub history_rows: i64,
    pub failed_history_7d: i64,
}

pub async fn diagnostics_counts(pool: &SqlitePool) -> Result<DiagnosticsCounts, RepoNyxError> {
    let row = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*) FROM accounts) AS accounts,
            (SELECT COUNT(*) FROM repositories) AS repositories,
            (SELECT COUNT(*) FROM repositories WHERE archive_status = 'synced') AS synced,
            (SELECT COUNT(*) FROM repositories WHERE archive_status = 'syncing') AS syncing,
            (SELECT COUNT(*) FROM repositories WHERE archive_status = 'failed') AS failed,
            (SELECT COUNT(*) FROM sync_jobs WHERE status = 'pending') AS pending_jobs,
            (SELECT COUNT(*) FROM sync_jobs WHERE status = 'running') AS running_jobs,
            (SELECT COUNT(*) FROM sync_jobs WHERE status = 'failed') AS failed_jobs,
            (SELECT COUNT(*) FROM schedules) AS schedules,
            (SELECT COUNT(*) FROM sync_history) AS history_rows,
            (SELECT COUNT(*) FROM sync_history
                WHERE result = 'failed'
                -- Audit DB-m5: `started_at` is RFC3339
                -- ("2026-11-25T07:00:00Z") while `datetime('now', ...)`
                -- returns space format — a raw string comparison
                -- miscounts rows on the cutoff day. Normalizing the
                -- stored value through `datetime()` first makes the
                -- compare a real timestamp compare.
                AND datetime(started_at) >= datetime('now', '-7 days')) AS failed_history_7d
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(map_error)?;
    Ok(DiagnosticsCounts {
        accounts: row.get("accounts"),
        repositories: row.get("repositories"),
        synced: row.get("synced"),
        syncing: row.get("syncing"),
        failed: row.get("failed"),
        pending_jobs: row.get("pending_jobs"),
        running_jobs: row.get("running_jobs"),
        failed_jobs: row.get("failed_jobs"),
        schedules: row.get("schedules"),
        history_rows: row.get("history_rows"),
        failed_history_7d: row.get("failed_history_7d"),
    })
}

/// Write a consistent snapshot of the database to `dest` and return
/// its size in bytes. `dest` must not already exist (`VACUUM INTO`
/// refuses to overwrite) and its parent directory is created.
///
/// The destination path is interpolated into the statement —
/// `VACUUM INTO` takes no bound parameters — so single quotes in
/// it are doubled. The path always comes from our own code (data
/// dir + timestamp), never from user input.
pub async fn backup_database(pool: &SqlitePool, dest: &Path) -> Result<u64, RepoNyxError> {
    if dest.exists() {
        return Err(invalid("backup destination already exists"));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| invalid(format!("cannot create backup directory: {e}")))?;
    }
    let escaped = dest.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{escaped}'");
    // `raw_sql` + `AssertSqlSafe`: the statement is dynamic by
    // necessity (`VACUUM INTO` takes no bound parameters), and the
    // path is escaped above. Audited: the only interpolation is
    // the escaped destination path, which our own code builds.
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(pool)
        .await
        .map_err(map_error)?;
    let bytes = std::fs::metadata(dest)
        .map(|meta| meta.len())
        .map_err(|e| invalid(format!("backup was not written: {e}")))?;
    Ok(bytes)
}

/// The 16-byte magic at the start of every SQLite database file.
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Check that `path` is a RepoNyx database backup: the SQLite magic,
/// a passing `PRAGMA integrity_check`, and the `settings` table.
/// The file is opened read-only and never modified.
pub async fn validate_backup_file(path: &Path) -> Result<(), RepoNyxError> {
    let head = std::fs::read(path).map_err(|e| invalid(format!("cannot read backup file: {e}")))?;
    if head.len() < SQLITE_MAGIC.len() || &head[..SQLITE_MAGIC.len()] != SQLITE_MAGIC {
        return Err(invalid("not a SQLite database file"));
    }
    // Audit DB-m4: no URL string — a path containing `%`, `#` or
    // `?` must not be parsed as URL syntax.
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(map_error)?;
    let result = validate_open_backup(&pool).await;
    pool.close().await;
    result
}

async fn validate_open_backup(pool: &SqlitePool) -> Result<(), RepoNyxError> {
    let row: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .map_err(map_error)?;
    if row != "ok" {
        return Err(invalid(format!("integrity check failed: {row}")));
    }
    let settings_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
    )
    .fetch_one(pool)
    .await
    .map_err(map_error)?;
    if settings_tables != 1 {
        return Err(invalid("not a RepoNyx database (no settings table)"));
    }
    Ok(())
}

/// Only these settings may appear in a diagnostics export: the four
/// seeded keys, none of which holds a secret. Device identifiers
/// and anything added later are excluded by default.
pub fn is_safe_for_diagnostics(key: &str) -> bool {
    matches!(
        key,
        "ui.theme" | "sync.default_mode" | "sync.concurrency" | "storage.root"
    )
}
