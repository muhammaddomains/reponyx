//! Key/value settings registry: get / set (upsert) / delete / list.
//!
//! The V1 migration seeds four default settings (`storage.root`,
//! `sync.default_mode`, `sync.concurrency`, `ui.theme`); no values
//! here are secrets.

use crate::common::{col, map_error, now_iso, require_non_empty};
use reponyx_core::{RepoNyxError, Setting};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteRow;
use uuid::Uuid;

fn row_to_setting(row: &SqliteRow) -> Result<Setting, sqlx::Error> {
    Ok(Setting {
        id: col(row, "id")?,
        key: col(row, "key")?,
        value: col(row, "value")?,
        updated_at: col(row, "updated_at")?,
    })
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<Setting>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_setting(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

/// Insert or update a setting by key. The row id is generated only on
/// insert; an update keeps the existing id.
pub async fn set_setting(
    pool: &SqlitePool,
    key: &str,
    value: &str,
) -> Result<Setting, RepoNyxError> {
    require_non_empty("key", key)?;
    let now = now_iso();
    sqlx::query(
        r#"
        INSERT INTO settings (id, key, value, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT (key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(key.trim())
    .bind(value)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_setting(pool, key.trim()).await?.ok_or_else(|| {
        RepoNyxError::new(
            reponyx_core::ErrorCode::InvalidInput,
            "setting just saved but not found",
        )
    })
}

pub async fn delete_setting(pool: &SqlitePool, key: &str) -> Result<bool, RepoNyxError> {
    let result = sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await
        .map_err(map_error)?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_settings(pool: &SqlitePool) -> Result<Vec<Setting>, RepoNyxError> {
    let rows = sqlx::query("SELECT * FROM settings ORDER BY key ASC")
        .fetch_all(pool)
        .await
        .map_err(map_error)?;
    rows.iter()
        .map(row_to_setting)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}
