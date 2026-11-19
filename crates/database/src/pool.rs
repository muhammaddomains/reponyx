//! Database opening: create dirs, connect, enable WAL + FKs, migrate.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// A connected, migrated database.
#[derive(Debug, Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

/// Open (creating if needed) the SQLite database at `path`, run pending
/// migrations, and enable WAL + foreign keys. The parent directory is
/// created if missing.
pub async fn open_database(path: &Path) -> Result<Database, sqlx::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
    }
    let url = format!("sqlite://{}?mode=rwc", path.to_string_lossy());
    let options = SqliteConnectOptions::from_str(&url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("../../migrations/sqlite").run(&pool).await?;
    Ok(Database { pool })
}
