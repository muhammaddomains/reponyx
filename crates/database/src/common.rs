//! Shared helpers for the repositories: timestamps, row decoding,
//! and mapping `sqlx::Error` onto `RepoNyxError`.
//!
//! All SQL in this crate uses bound parameters only — never
//! string-concatenated values.

use chrono::{DateTime, Utc};
use reponyx_core::{ErrorCode, RepoNyxError};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use uuid::Uuid;

/// Current UTC time in the RFC 3339 form stored in every timestamp
/// column (e.g. `2026-11-18T19:00:00Z`).
pub(crate) fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Current UTC time, for binding into `next_run_at`-style columns.
pub(crate) fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

/// `InvalidInput` error with a plain message.
pub(crate) fn invalid(message: impl Into<String>) -> RepoNyxError {
    RepoNyxError::new(ErrorCode::InvalidInput, message)
}

/// Map a sqlx error onto the domain error type.
///
/// - Foreign-key violations (a row referencing a missing parent) and
///   unique-constraint violations (e.g. the V2 full_name index) are
///   `InvalidInput`: the caller supplied an identity that cannot exist.
/// - Everything else is `DatabaseError` with the sqlx message attached.
pub(crate) fn map_error(err: sqlx::Error) -> RepoNyxError {
    match &err {
        sqlx::Error::Database(db_err) => {
            let message = db_err.message();
            if message.contains("FOREIGN KEY constraint failed") {
                return RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "referenced row does not exist (foreign key violation)",
                )
                .with_details(message.to_string());
            }
            if message.contains("UNIQUE constraint failed") {
                return RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "a row with the same unique identity already exists",
                )
                .with_details(message.to_string());
            }
            RepoNyxError::new(ErrorCode::DatabaseError, "database operation failed")
                .with_details(message.to_string())
        }
        sqlx::Error::RowNotFound => RepoNyxError::new(ErrorCode::InvalidInput, "row not found"),
        other => RepoNyxError::new(ErrorCode::DatabaseError, "database operation failed")
            .with_details(other.to_string()),
    }
}

/// Read a required column.
pub(crate) fn col<T>(row: &SqliteRow, name: &str) -> Result<T, sqlx::Error>
where
    T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
{
    row.try_get(name)
}

/// Read an optional column.
pub(crate) fn opt_col<T>(row: &SqliteRow, name: &str) -> Result<Option<T>, sqlx::Error>
where
    T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
{
    row.try_get(name)
}

/// Read a UUID stored as TEXT.
pub(crate) fn col_uuid(row: &SqliteRow, name: &str) -> Result<Uuid, sqlx::Error> {
    let text: String = row.try_get(name)?;
    Uuid::parse_str(&text).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

/// Read an optional UUID stored as TEXT.
pub(crate) fn opt_col_uuid(row: &SqliteRow, name: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let text: Option<String> = row.try_get(name)?;
    match text {
        Some(text) => Uuid::parse_str(&text)
            .map(Some)
            .map_err(|e| sqlx::Error::Decode(Box::new(e))),
        None => Ok(None),
    }
}

/// Read a boolean stored as INTEGER 0/1.
pub(crate) fn col_bool(row: &SqliteRow, name: &str) -> Result<bool, sqlx::Error> {
    let value: i64 = row.try_get(name)?;
    Ok(value != 0)
}

/// Require a non-empty (after trimming) string value.
pub(crate) fn require_non_empty(field: &str, value: &str) -> Result<(), RepoNyxError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} must not be empty")));
    }
    Ok(())
}
