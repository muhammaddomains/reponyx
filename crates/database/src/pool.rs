//! Database opening: create dirs, connect, enable WAL + FKs, migrate.

use crate::common::{invalid, map_error};
use crate::diagnostics::backup_database;
use chrono::Utc;
use reponyx_core::{ErrorCode, RepoNyxError};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A connected, migrated database.
#[derive(Debug, Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

/// How many pre-migration backups to keep beside the database.
const PRE_MIGRATE_BACKUPS_KEPT: usize = 3;

/// Open (creating if needed) the SQLite database at `path`, run pending
/// migrations, and enable WAL + foreign keys. The parent directory is
/// created if missing.
///
/// Crash safety (audit B7 / DB-M5): when the file already exists, a
/// `VACUUM INTO` backup is written to `<parent>/backups/` *before*
/// migrations run, and `PRAGMA integrity_check` runs before the
/// first migration — a failed migration used to leave the whole
/// database unusable with nothing to restore from. A failed backup
/// is logged and the open continues (a backup failure must not
/// brick startup); a failed integrity check or migration fails the
/// open with the backup location in the message, so the user can
/// restore it from Settings.
///
/// A fresh database is logged loudly (audit B6): a wrong data dir
/// used to produce a healthy-looking empty app with no hint that
/// the real database lives elsewhere.
pub async fn open_database(path: &Path) -> Result<Database, RepoNyxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| map_error(sqlx::Error::Io(e)))?;
    }
    let existed = path.exists();
    let url = format!("sqlite://{}?mode=rwc", path.to_string_lossy());
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(map_error)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        // Writers (queue workers, scheduler, desktop, agent) contend
        // for SQLite's single write lock. Without a busy timeout a
        // colliding writer fails immediately with "database is
        // locked" and the job is left `running`; with a timeout the
        // writer waits its turn. 5 s is far above any transaction
        // this app runs (all are single-statement or a few rows).
        .busy_timeout(std::time::Duration::from_secs(5))
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| {
            // The connect itself runs the WAL pragma, so a file
            // that is not a database at all fails here — name the
            // path and the SQLite reason instead of a bare code.
            RepoNyxError::new(
                ErrorCode::DatabaseError,
                format!("cannot open database at {}: {e}", path.display()),
            )
        })?;

    if existed {
        let backup = match pre_migrate_backup(&pool, path).await {
            Ok(backup) => {
                if let Some(backup) = &backup {
                    tracing::info!(backup = %backup.display(), "pre-migration backup written");
                }
                backup
            }
            Err(e) => {
                tracing::warn!(error = %e.message, "pre-migration backup failed; continuing without one");
                None
            }
        };
        integrity_check(&pool, path, backup.as_deref()).await?;
    } else {
        tracing::warn!(
            path = %path.display(),
            "no existing database found; creating a new empty database"
        );
    }

    sqlx::migrate!("../../migrations/sqlite")
        .run(&pool)
        .await
        .map_err(|e| map_error(sqlx::Error::from(e)))?;
    tracing::info!(path = %path.display(), existed, "database opened");
    Ok(Database { pool })
}

/// Write a `VACUUM INTO` backup of the live database into
/// `<db-parent>/backups/reponyx-pre-migrate-<timestamp>.sqlite`,
/// pruning older pre-migration backups to the last
/// [`PRE_MIGRATE_BACKUPS_KEPT`]. Returns the backup path. A
/// same-second open (desktop + agent starting together) retries
/// with a counter suffix instead of failing on the existing file.
async fn pre_migrate_backup(
    pool: &SqlitePool,
    path: &Path,
) -> Result<Option<PathBuf>, RepoNyxError> {
    let Some(parent) = path.parent() else {
        return Ok(None);
    };
    let backups_dir = parent.join("backups");
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    for attempt in 0..100 {
        let name = if attempt == 0 {
            format!("reponyx-pre-migrate-{stamp}.sqlite")
        } else {
            format!("reponyx-pre-migrate-{stamp}-{attempt}.sqlite")
        };
        let dest = backups_dir.join(name);
        if dest.exists() {
            continue;
        }
        match backup_database(pool, &dest).await {
            Ok(_) => {
                prune_pre_migrate_backups(&backups_dir);
                return Ok(Some(dest));
            }
            Err(e) if e.message.contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    Err(invalid(
        "could not pick a free pre-migration backup name".to_string(),
    ))
}

/// Delete pre-migration backups older than the newest
/// [`PRE_MIGRATE_BACKUPS_KEPT`]. Best-effort: a failure here never
/// fails the open.
fn prune_pre_migrate_backups(backups_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(backups_dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    n.to_string_lossy().starts_with("reponyx-pre-migrate-")
                        && n.to_string_lossy().ends_with(".sqlite")
                })
                .unwrap_or(false)
        })
        .collect();
    backups.sort();
    while backups.len() > PRE_MIGRATE_BACKUPS_KEPT {
        if let Some(oldest) = backups.first() {
            let _ = std::fs::remove_file(oldest);
        }
        backups.remove(0);
    }
}

/// Fail the open when the database file is corrupt, before any
/// migration can make it worse. The message names the database
/// path and the pre-migration backup to restore (when one was
/// written), so the failure is recoverable instead of a dead end.
async fn integrity_check(
    pool: &SqlitePool,
    path: &Path,
    backup: Option<&Path>,
) -> Result<(), RepoNyxError> {
    let rows = match sqlx::query("PRAGMA integrity_check").fetch_all(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            return Err(invalid(format!(
                "database integrity check failed for {}: {e}",
                path.display()
            )));
        }
    };
    let mut problems = Vec::new();
    for row in &rows {
        let message: String = row.try_get(0).map_err(map_error)?;
        if message != "ok" {
            problems.push(message);
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    let restore = match backup {
        Some(backup) => format!("Restore the pre-migration backup at {}.", backup.display()),
        None => {
            "No pre-migration backup could be written; restore the latest backup from Settings."
                .to_string()
        }
    };
    Err(invalid(format!(
        "database integrity check failed for {}: {}. {}",
        path.display(),
        problems.join("; "),
        restore
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn open_creates_a_fresh_database_and_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("reponyx.sqlite");
        let db = open_database(&path).await.unwrap();
        // Migrations ran: the accounts table exists.
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM accounts")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(row.0, 0);
    }

    #[tokio::test]
    async fn reopen_writes_a_pre_migrate_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("reponyx.sqlite");
        let first = open_database(&path).await.unwrap();
        drop(first);
        let _second = open_database(&path).await.unwrap();
        let backups_dir = dir.path().join("data").join("backups");
        let backups: Vec<_> = std::fs::read_dir(&backups_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].starts_with("reponyx-pre-migrate-"));
        assert!(backups[0].ends_with(".sqlite"));
    }

    #[tokio::test]
    async fn open_fails_on_a_file_that_is_not_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reponyx.sqlite");
        std::fs::write(&path, b"this is not a sqlite database at all").unwrap();
        let err = open_database(&path).await.unwrap_err();
        assert!(
            err.message.contains("cannot open database"),
            "unexpected error: {}",
            err.message
        );
        assert!(err.message.contains("reponyx.sqlite"));
    }

    #[tokio::test]
    async fn integrity_check_fails_on_a_corrupt_database() {
        // A valid database whose first page is scrambled past the
        // header (a fresh database is a single 4 KiB page holding
        // the schema): the header stays intact so the connect
        // succeeds, but the integrity check must catch the
        // corruption before any migration runs — and the error
        // must name the database path.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reponyx.sqlite");
        let first = open_database(&path).await.unwrap();
        // Close (not just drop) the pool: the close checkpoints
        // the WAL into the main file, so the bytes read below are
        // the real database. A bare `drop` leaves the checkpoint
        // to a background task that can overwrite the corruption
        // after this test writes it.
        first.pool.close().await;
        let mut bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.len() > 1024,
            "fresh database is {} bytes",
            bytes.len()
        );
        // Scramble page 2 (offset 4096..4608): the header stays
        // intact so the connect succeeds, but the page is
        // malformed and the integrity check must catch it before
        // any migration runs. (The file's tail is freelist space
        // that integrity_check tolerates, so corrupting there
        // proves nothing.)
        assert!(
            bytes.len() > 8192,
            "fresh database is {} bytes",
            bytes.len()
        );
        for byte in bytes.iter_mut().skip(4096).take(512) {
            *byte = 0xFF;
        }
        std::fs::write(&path, &bytes).unwrap();
        let err = open_database(&path).await.unwrap_err();
        assert!(
            err.message.contains("integrity check failed")
                || err.message.contains("cannot open database"),
            "unexpected error: {}",
            err.message
        );
        assert!(err.message.contains("reponyx.sqlite"));
    }

    #[tokio::test]
    async fn prune_keeps_only_the_newest_backups() {
        let dir = tempfile::tempdir().unwrap();
        for stamp in [
            "20260101-000000",
            "20260102-000000",
            "20260103-000000",
            "20260104-000000",
        ] {
            std::fs::write(
                dir.path()
                    .join(format!("reponyx-pre-migrate-{stamp}.sqlite")),
                b"x",
            )
            .unwrap();
        }
        std::fs::write(dir.path().join("something-else.sqlite"), b"x").unwrap();
        prune_pre_migrate_backups(dir.path());
        let remaining: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(remaining.len(), 4); // 3 backups + the unrelated file
        assert!(!remaining.contains(&"reponyx-pre-migrate-20260101-000000.sqlite".to_string()));
    }

    #[tokio::test]
    async fn pre_migrate_backup_is_a_valid_database() {
        // The backup must itself pass the same checks a restore
        // would run: SQLite magic + integrity.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("reponyx.sqlite");
        let first = open_database(&path).await.unwrap();
        drop(first);
        let second = open_database(&path).await.unwrap();
        drop(second);
        let backups_dir = dir.path().join("data").join("backups");
        let backup = std::fs::read_dir(&backups_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .next()
            .unwrap();
        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}?mode=ro",
            backup.to_string_lossy()
        ))
        .unwrap()
        .read_only(true);
        let backup_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let row: (String,) = sqlx::query_as("PRAGMA integrity_check")
            .fetch_one(&backup_pool)
            .await
            .unwrap();
        assert_eq!(row.0, "ok");
    }
}
