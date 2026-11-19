//! Sync-history registry: record + list.
//!
//! History rows are written in the same transaction as the job outcome
//! they describe (see `complete_job_with_history` in `jobs`); the
//! helpers here also accept a transaction so callers can do that.

use crate::common::{col, col_uuid, map_error, opt_col, opt_col_uuid};
use chrono::{DateTime, Utc};
use reponyx_core::{RepoNyxError, SyncHistory, SyncResult};
use sqlx::sqlite::{SqliteRow, SqliteTransaction};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use uuid::Uuid;

/// Fields for recording one history entry.
#[derive(Debug, Clone)]
pub struct NewHistory {
    pub repository_id: Uuid,
    pub job_id: Option<Uuid>,
    pub operation: String,
    pub result: SyncResult,
    pub duration_ms: Option<i64>,
    pub bytes_received: Option<i64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

fn row_to_history(row: &SqliteRow) -> Result<SyncHistory, sqlx::Error> {
    Ok(SyncHistory {
        id: col_uuid(row, "id")?,
        repository_id: col_uuid(row, "repository_id")?,
        job_id: opt_col_uuid(row, "job_id")?,
        operation: col(row, "operation")?,
        result: SyncResult::from_str(&col::<String>(row, "result")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        duration_ms: opt_col(row, "duration_ms")?,
        bytes_received: opt_col(row, "bytes_received")?,
        started_at: col(row, "started_at")?,
        completed_at: opt_col(row, "completed_at")?,
    })
}

async fn insert_history(
    tx: &mut SqliteTransaction<'_>,
    input: &NewHistory,
) -> Result<SyncHistory, RepoNyxError> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO sync_history
            (id, repository_id, job_id, operation, result,
             duration_ms, bytes_received, started_at, completed_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.repository_id.to_string())
    .bind(input.job_id.map(|id| id.to_string()))
    .bind(&input.operation)
    .bind(input.result.as_str())
    .bind(input.duration_ms)
    .bind(input.bytes_received)
    .bind(input.started_at)
    .bind(input.completed_at)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    let row = sqlx::query("SELECT * FROM sync_history WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_error)?;
    row_to_history(&row).map_err(map_error)
}

/// Record one history entry (own transaction).
pub async fn record_history(
    pool: &SqlitePool,
    input: NewHistory,
) -> Result<SyncHistory, RepoNyxError> {
    let mut tx = pool.begin().await.map_err(map_error)?;
    let entry = insert_history(&mut tx, &input).await?;
    tx.commit().await.map_err(map_error)?;
    Ok(entry)
}

/// Record one history entry inside an existing transaction. Used by
/// `jobs::complete_job_with_history` so the job outcome and its
/// history entry commit — or roll back — together.
pub async fn record_history_in_tx(
    tx: &mut SqliteTransaction<'_>,
    input: NewHistory,
) -> Result<SyncHistory, RepoNyxError> {
    insert_history(tx, &input).await
}

pub async fn get_history(pool: &SqlitePool, id: Uuid) -> Result<Option<SyncHistory>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM sync_history WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_history(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

/// Most recent history entries for a repository, newest first.
pub async fn list_history_for_repository(
    pool: &SqlitePool,
    repository_id: Uuid,
    limit: i64,
) -> Result<Vec<SyncHistory>, RepoNyxError> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM sync_history
        WHERE repository_id = ?
        ORDER BY started_at DESC
        LIMIT ?
        "#,
    )
    .bind(repository_id.to_string())
    .bind(limit.max(1))
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    rows.iter()
        .map(row_to_history)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

pub async fn list_history_for_job(
    pool: &SqlitePool,
    job_id: Uuid,
) -> Result<Vec<SyncHistory>, RepoNyxError> {
    let rows = sqlx::query("SELECT * FROM sync_history WHERE job_id = ? ORDER BY started_at DESC")
        .bind(job_id.to_string())
        .fetch_all(pool)
        .await
        .map_err(map_error)?;
    rows.iter()
        .map(row_to_history)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

pub async fn count_history_for_repository(
    pool: &SqlitePool,
    repository_id: Uuid,
) -> Result<i64, RepoNyxError> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM sync_history WHERE repository_id = ?")
        .bind(repository_id.to_string())
        .fetch_one(pool)
        .await
        .map_err(map_error)?;
    Ok(row.get("n"))
}
