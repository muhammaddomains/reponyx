//! Schedule registry: CRUD, scope validation, effective-schedule
//! resolution, due schedules.
//!
//! Precedence for a repository is repository > account > global: the
//! most specific enabled schedule wins. Scope consistency is enforced
//! both here and by the V1 `CHECK` constraint (global: no ids;
//! account: account only; repository: repository id required).

use crate::common::{
    col, col_bool, col_uuid, invalid, map_error, now_iso, now_utc, opt_col, opt_col_uuid,
};
use crate::repositories::get_repository;
use chrono::{DateTime, Utc};
use reponyx_core::{IntervalType, RepoNyxError, Schedule, ScheduleScope};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, SqlitePool};
use std::str::FromStr;
use uuid::Uuid;

/// Fields for creating a schedule.
#[derive(Debug, Clone)]
pub struct NewSchedule {
    pub scope: ScheduleScope,
    pub account_id: Option<Uuid>,
    pub repository_id: Option<Uuid>,
    pub interval_type: IntervalType,
    pub interval_value: Option<String>,
    pub timezone: String,
    pub enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
}

/// Optional fields for `update_schedule`. `Option<Option<T>>`: outer
/// `None` leaves the column alone, inner `None` clears it.
#[derive(Debug, Clone, Default)]
pub struct SchedulePatch {
    pub interval_type: Option<IntervalType>,
    pub interval_value: Option<Option<String>>,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
    pub next_run_at: Option<Option<DateTime<Utc>>>,
}

fn row_to_schedule(row: &SqliteRow) -> Result<Schedule, sqlx::Error> {
    Ok(Schedule {
        id: col_uuid(row, "id")?,
        scope: ScheduleScope::from_str(&col::<String>(row, "scope")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        account_id: opt_col_uuid(row, "account_id")?,
        repository_id: opt_col_uuid(row, "repository_id")?,
        interval_type: IntervalType::from_str(&col::<String>(row, "interval_type")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        interval_value: opt_col(row, "interval_value")?,
        timezone: col(row, "timezone")?,
        enabled: col_bool(row, "enabled")?,
        next_run_at: opt_col(row, "next_run_at")?,
        created_at: col(row, "created_at")?,
        updated_at: col(row, "updated_at")?,
    })
}

/// Validate scope ↔ id consistency (mirrors the V1 CHECK constraint).
fn validate_scope(
    scope: ScheduleScope,
    account_id: Option<Uuid>,
    repository_id: Option<Uuid>,
) -> Result<(), RepoNyxError> {
    match scope {
        ScheduleScope::Global => {
            if account_id.is_some() || repository_id.is_some() {
                return Err(invalid(
                    "a global schedule must not reference an account or repository",
                ));
            }
        }
        ScheduleScope::Account => {
            if account_id.is_none() || repository_id.is_some() {
                return Err(invalid(
                    "an account schedule needs account_id and no repository_id",
                ));
            }
        }
        ScheduleScope::Repository => {
            if repository_id.is_none() {
                return Err(invalid("a repository schedule needs repository_id"));
            }
        }
    }
    Ok(())
}

pub async fn create_schedule(
    pool: &SqlitePool,
    input: NewSchedule,
) -> Result<Schedule, RepoNyxError> {
    validate_scope(input.scope, input.account_id, input.repository_id)?;
    if input.timezone.trim().is_empty() {
        return Err(invalid("timezone must not be empty"));
    }
    let id = Uuid::new_v4();
    let now = now_iso();
    sqlx::query(
        r#"
        INSERT INTO schedules
            (id, scope, account_id, repository_id, interval_type,
             interval_value, timezone, enabled, next_run_at, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.scope.as_str())
    .bind(input.account_id.map(|id| id.to_string()))
    .bind(input.repository_id.map(|id| id.to_string()))
    .bind(input.interval_type.as_str())
    .bind(input.interval_value)
    .bind(input.timezone.trim())
    .bind(i64::from(input.enabled))
    .bind(input.next_run_at)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_schedule(pool, id)
        .await?
        .ok_or_else(|| invalid("schedule just created but not found"))
}

pub async fn get_schedule(pool: &SqlitePool, id: Uuid) -> Result<Option<Schedule>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM schedules WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_schedule(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn list_schedules(pool: &SqlitePool) -> Result<Vec<Schedule>, RepoNyxError> {
    let rows = sqlx::query("SELECT * FROM schedules ORDER BY scope ASC, created_at ASC")
        .fetch_all(pool)
        .await
        .map_err(map_error)?;
    rows.iter()
        .map(row_to_schedule)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

async fn get_schedule_for(
    pool: &SqlitePool,
    scope: ScheduleScope,
    account_id: Option<Uuid>,
    repository_id: Option<Uuid>,
) -> Result<Option<Schedule>, RepoNyxError> {
    let mut builder = QueryBuilder::new("SELECT * FROM schedules WHERE scope = ");
    builder.push_bind(scope.as_str());
    // Repository-scope rows may or may not carry an account_id (the V1
    // CHECK only requires repository_id), so never filter on account_id
    // for that scope — the repository_id alone identifies them.
    if scope != ScheduleScope::Repository {
        match account_id {
            Some(id) => {
                builder.push(" AND account_id = ");
                builder.push_bind(id.to_string());
            }
            None => {
                builder.push(" AND account_id IS NULL");
            }
        }
    }
    match repository_id {
        Some(id) => {
            builder.push(" AND repository_id = ");
            builder.push_bind(id.to_string());
        }
        None => {
            builder.push(" AND repository_id IS NULL");
        }
    }
    builder.push(" ORDER BY updated_at DESC LIMIT 1");
    let row = builder
        .build()
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_schedule(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn get_schedule_for_repository(
    pool: &SqlitePool,
    repository_id: Uuid,
) -> Result<Option<Schedule>, RepoNyxError> {
    get_schedule_for(pool, ScheduleScope::Repository, None, Some(repository_id)).await
}

pub async fn get_schedule_for_account(
    pool: &SqlitePool,
    account_id: Uuid,
) -> Result<Option<Schedule>, RepoNyxError> {
    get_schedule_for(pool, ScheduleScope::Account, Some(account_id), None).await
}

pub async fn get_global_schedule(pool: &SqlitePool) -> Result<Option<Schedule>, RepoNyxError> {
    get_schedule_for(pool, ScheduleScope::Global, None, None).await
}

/// The schedule that governs a repository: the repository schedule if
/// one exists, else the account schedule, else the global schedule,
/// else `None`. Returns `InvalidInput` when the repository id is
/// unknown.
pub async fn effective_schedule_for(
    pool: &SqlitePool,
    repository_id: Uuid,
) -> Result<Option<Schedule>, RepoNyxError> {
    let repository = get_repository(pool, repository_id)
        .await?
        .ok_or_else(|| invalid(format!("repository {repository_id} not found")))?;
    if let Some(schedule) = get_schedule_for_repository(pool, repository_id).await? {
        return Ok(Some(schedule));
    }
    if let Some(schedule) = get_schedule_for_account(pool, repository.account_id).await? {
        return Ok(Some(schedule));
    }
    get_global_schedule(pool).await
}

/// Enabled schedules whose `next_run_at` has passed, oldest first.
/// A schedule with no `next_run_at` is never due.
pub async fn due_schedules(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Vec<Schedule>, RepoNyxError> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM schedules
        WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?
        ORDER BY next_run_at ASC
        "#,
    )
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    rows.iter()
        .map(row_to_schedule)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

/// Convenience: due schedules as of right now.
pub async fn due_schedules_now(pool: &SqlitePool) -> Result<Vec<Schedule>, RepoNyxError> {
    due_schedules(pool, now_utc()).await
}

pub async fn update_schedule(
    pool: &SqlitePool,
    id: Uuid,
    patch: SchedulePatch,
) -> Result<Schedule, RepoNyxError> {
    let mut builder = QueryBuilder::new("UPDATE schedules SET updated_at = ");
    builder.push_bind(now_iso());
    if let Some(interval_type) = patch.interval_type {
        builder.push(", interval_type = ");
        builder.push_bind(interval_type.as_str());
    }
    if let Some(interval_value) = patch.interval_value {
        builder.push(", interval_value = ");
        builder.push_bind(interval_value);
    }
    if let Some(timezone) = patch.timezone {
        if timezone.trim().is_empty() {
            return Err(invalid("timezone must not be empty"));
        }
        builder.push(", timezone = ");
        builder.push_bind(timezone);
    }
    if let Some(enabled) = patch.enabled {
        builder.push(", enabled = ");
        builder.push_bind(i64::from(enabled));
    }
    if let Some(next_run_at) = patch.next_run_at {
        builder.push(", next_run_at = ");
        builder.push_bind(next_run_at);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id.to_string());
    builder.build().execute(pool).await.map_err(map_error)?;
    get_schedule(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("schedule {id} not found")))
}

pub async fn delete_schedule(pool: &SqlitePool, id: Uuid) -> Result<bool, RepoNyxError> {
    let result = sqlx::query("DELETE FROM schedules WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    Ok(result.rows_affected() > 0)
}
