//! Sync-job registry: create / claim / finish, with history.
//!
//! Claiming is atomic: the next pending job (highest priority, oldest
//! first) is picked and moved to `running` by a single
//! `UPDATE ... RETURNING` statement, so two claimers can never take
//! the same job.
//!
//! Finishing a job and recording its history entry happen in one
//! transaction (`complete_job_with_history`): either both land or
//! neither does.

use crate::common::{col, col_uuid, invalid, map_error, now_iso, now_utc, opt_col};
use crate::history::{NewHistory, record_history_in_tx};
use chrono::{DateTime, Utc};
use reponyx_core::{JobStatus, JobType, RepoNyxError, SyncHistory, SyncJob, SyncResult};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, SqlitePool};
use std::str::FromStr;
use uuid::Uuid;

/// Fields for creating a job. Jobs always start `pending`.
#[derive(Debug, Clone)]
pub struct NewJob {
    pub repository_id: Uuid,
    pub job_type: JobType,
    pub priority: i32,
}

/// Outcome for finishing a job with its history entry.
#[derive(Debug, Clone)]
pub struct JobOutcome {
    pub result: SyncResult,
    pub duration_ms: Option<i64>,
    pub bytes_received: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// One status bucket from `count_jobs_by_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobStatusCount {
    pub status: JobStatus,
    pub count: i64,
}

fn row_to_job(row: &SqliteRow) -> Result<SyncJob, sqlx::Error> {
    Ok(SyncJob {
        id: col_uuid(row, "id")?,
        repository_id: col_uuid(row, "repository_id")?,
        job_type: JobType::from_str(&col::<String>(row, "job_type")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        status: JobStatus::from_str(&col::<String>(row, "status")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        priority: col(row, "priority")?,
        attempt_count: col(row, "attempt_count")?,
        started_at: opt_col(row, "started_at")?,
        completed_at: opt_col(row, "completed_at")?,
        error_code: opt_col(row, "error_code")?,
        error_message: opt_col(row, "error_message")?,
        created_at: col(row, "created_at")?,
        available_at: opt_col(row, "available_at")?,
        worker_id: opt_col(row, "worker_id")?,
        lease_expires_at: opt_col(row, "lease_expires_at")?,
        heartbeat_at: opt_col(row, "heartbeat_at")?,
        cancel_requested_at: opt_col(row, "cancel_requested_at")?,
        dedupe_key: opt_col(row, "dedupe_key")?,
    })
}

/// Deterministic dedup key: at most one active job per (repository,
/// job type). The partial unique index enforces it; callers also
/// check before enqueueing so the common path never hits the index.
pub fn dedupe_key_for(repository_id: Uuid, job_type: JobType) -> String {
    format!("repo:{repository_id}:{}", job_type.as_str())
}

/// True when `err` is a SQLite unique-constraint violation (e.g. the
/// dedupe index). Callers use this to tolerate an enqueue race: if
/// another writer already queued the same work, the job exists and
/// there is nothing to retry.
/// A unique-constraint failure. `map_error` rewrites the message to a
/// friendly "already exists" string and puts the raw SQLite text in
/// `details`, so both must be checked — checking only `message` misses
/// every mapped violation (found by the 2026-11-21 live-run review).
pub fn is_unique_violation(err: &RepoNyxError) -> bool {
    err.message.contains("UNIQUE constraint failed")
        || err
            .details
            .as_deref()
            .is_some_and(|d| d.contains("UNIQUE constraint failed"))
}

pub async fn create_job(pool: &SqlitePool, input: NewJob) -> Result<SyncJob, RepoNyxError> {
    let id = Uuid::new_v4();
    let now = now_iso();
    let dedupe_key = dedupe_key_for(input.repository_id, input.job_type);
    sqlx::query(
        r#"
        INSERT INTO sync_jobs
            (id, repository_id, job_type, status, priority,
             attempt_count, created_at, dedupe_key)
        VALUES (?, ?, ?, 'pending', ?, 0, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.repository_id.to_string())
    .bind(input.job_type.as_str())
    .bind(input.priority)
    .bind(&now)
    .bind(&dedupe_key)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_job(pool, id)
        .await?
        .ok_or_else(|| invalid("job just created but not found"))
}

/// Create the next attempt of a failed job as a new row (a terminal job
/// is never moved back to `pending`).
///
/// The retry row **carries the parent's `attempt_count`** so the
/// attempt budget binds across retries, and it is not claimable before
/// `available_at` (the computed backoff), so the delay is enforced by
/// the claim query instead of by whichever worker happened to fail it.
pub async fn create_retry_job(
    pool: &SqlitePool,
    parent: &SyncJob,
    available_at: DateTime<Utc>,
) -> Result<SyncJob, RepoNyxError> {
    let id = Uuid::new_v4();
    let now = now_iso();
    let dedupe_key = dedupe_key_for(parent.repository_id, parent.job_type);
    sqlx::query(
        r#"
        INSERT INTO sync_jobs
            (id, repository_id, job_type, status, priority,
             attempt_count, created_at, available_at, dedupe_key)
        VALUES (?, ?, ?, 'pending', ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(parent.repository_id.to_string())
    .bind(parent.job_type.as_str())
    .bind(parent.priority)
    .bind(parent.attempt_count)
    .bind(&now)
    .bind(available_at)
    .bind(&dedupe_key)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_job(pool, id)
        .await?
        .ok_or_else(|| invalid("retry job just created but not found"))
}

pub async fn get_job(pool: &SqlitePool, id: Uuid) -> Result<Option<SyncJob>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM sync_jobs WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_job(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

/// Filters for listing jobs.
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub repository_id: Option<Uuid>,
    pub status: Option<JobStatus>,
}

pub async fn list_jobs(pool: &SqlitePool, filter: JobFilter) -> Result<Vec<SyncJob>, RepoNyxError> {
    let mut builder = QueryBuilder::new("SELECT * FROM sync_jobs");
    let mut first = true;
    if let Some(repository_id) = filter.repository_id {
        builder.push(" WHERE repository_id = ");
        builder.push_bind(repository_id.to_string());
        first = false;
    }
    if let Some(status) = filter.status {
        if first {
            builder.push(" WHERE ");
        } else {
            builder.push(" AND ");
        }
        builder.push("status = ");
        builder.push_bind(status.as_str());
    }
    builder.push(" ORDER BY priority DESC, created_at ASC");
    let rows = builder.build().fetch_all(pool).await.map_err(map_error)?;
    rows.iter()
        .map(row_to_job)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

/// Job counts per status (only statuses that have jobs are returned).
pub async fn count_jobs_by_status(pool: &SqlitePool) -> Result<Vec<JobStatusCount>, RepoNyxError> {
    let rows = sqlx::query(
        "SELECT status, COUNT(*) AS n FROM sync_jobs GROUP BY status ORDER BY status ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(map_error)?;
    let mut counts = Vec::with_capacity(rows.len());
    for row in &rows {
        let status = JobStatus::from_str(&row.try_get::<String, _>("status").map_err(map_error)?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))
            .map_err(map_error)?;
        counts.push(JobStatusCount {
            status,
            count: row.get("n"),
        });
    }
    Ok(counts)
}

/// Atomically claim the next pending job and take a lease on it.
/// Returns `None` when no job is claimable.
///
/// This is a single `UPDATE ... RETURNING` statement, so the pick and
/// the move to `running` happen under one write lock — two claimers
/// can never take the same job. (A select-then-update transaction
/// deadlocks here: both claimers hold a shared read lock and neither
/// can upgrade to write.)
///
/// Claiming records the owning `worker_id` plus a lease
/// (`lease_expires_at` / `heartbeat_at`, `lease` from now). Recovery
/// only treats a running job as abandoned after its lease expires.
///
/// Selection rules:
/// - Only jobs whose `available_at` is null or already passed are
///   eligible (retry backoff and rate-limit defer are enforced here,
///   not by whichever worker failed the job).
/// - Priority ordering with aging: a pending job gains up to +3
///   effective priority after 3 hours waiting
///   (`MIN(3, hours_waiting)`), so low-priority work cannot starve
///   forever while higher-priority work keeps arriving. FIFO is
///   preserved within the same effective priority.
pub async fn claim_next_job(
    pool: &SqlitePool,
    worker_id: &str,
    lease: std::time::Duration,
) -> Result<Option<SyncJob>, RepoNyxError> {
    let now = now_utc();
    let lease_expires_at =
        now + chrono::Duration::from_std(lease).unwrap_or(chrono::Duration::seconds(60));
    let row = sqlx::query(
        r#"
        UPDATE sync_jobs
        SET status = 'running', started_at = ?, attempt_count = attempt_count + 1,
            worker_id = ?, lease_expires_at = ?, heartbeat_at = ?
        WHERE id = (
            SELECT id FROM sync_jobs
            WHERE status = 'pending'
              AND (available_at IS NULL OR available_at <= ?)
            ORDER BY (priority + COALESCE(CAST(MIN(3.0, (julianday('now') - julianday(created_at)) * 24.0) AS INTEGER), 0)) DESC,
                     created_at ASC
            LIMIT 1
        )
        RETURNING id
        "#,
    )
    .bind(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    .bind(worker_id)
    .bind(lease_expires_at)
    .bind(now)
    .bind(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    .fetch_optional(pool)
    .await
    .map_err(map_error)?;
    match row {
        Some(row) => {
            let id: String = row.get("id");
            let job_id = Uuid::parse_str(&id)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))
                .map_err(map_error)?;
            Ok(get_job(pool, job_id).await?)
        }
        None => Ok(None),
    }
}

/// Renew a running job's lease. Only the owning worker may renew
/// (`worker_id` must match and the job must still be `running`);
/// returns `None` when the job is no longer owned by this worker
/// (e.g. recovery already reclaimed it).
pub async fn heartbeat_job(
    pool: &SqlitePool,
    id: Uuid,
    worker_id: &str,
    lease: std::time::Duration,
) -> Result<Option<SyncJob>, RepoNyxError> {
    let now = now_utc();
    let lease_expires_at =
        now + chrono::Duration::from_std(lease).unwrap_or(chrono::Duration::seconds(60));
    let affected = sqlx::query(
        r#"
        UPDATE sync_jobs
        SET heartbeat_at = ?, lease_expires_at = ?
        WHERE id = ? AND status = 'running' AND worker_id = ?
        "#,
    )
    .bind(now)
    .bind(lease_expires_at)
    .bind(id.to_string())
    .bind(worker_id)
    .execute(pool)
    .await
    .map_err(map_error)?
    .rows_affected();
    if affected == 0 {
        return Ok(None);
    }
    get_job(pool, id).await
}

/// Request cancellation of a job.
///
/// - `pending` → cancelled immediately (terminal) and the request is
///   recorded.
/// - `running` → only `cancel_requested_at` is set; the owning worker
///   sees it on its next heartbeat check and cancels cooperatively.
/// - terminal jobs are returned unchanged.
pub async fn request_cancel_job(pool: &SqlitePool, id: Uuid) -> Result<SyncJob, RepoNyxError> {
    let current = get_job(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("job {id} not found")))?;
    match current.status {
        JobStatus::Pending => {
            sqlx::query("UPDATE sync_jobs SET cancel_requested_at = ? WHERE id = ?")
                .bind(now_utc())
                .bind(id.to_string())
                .execute(pool)
                .await
                .map_err(map_error)?;
            transition_job(pool, id, JobStatus::Cancelled, None, None).await
        }
        JobStatus::Running => {
            sqlx::query(
                "UPDATE sync_jobs SET cancel_requested_at = ? WHERE id = ? AND status = 'running'",
            )
            .bind(now_utc())
            .bind(id.to_string())
            .execute(pool)
            .await
            .map_err(map_error)?;
            get_job(pool, id)
                .await?
                .ok_or_else(|| invalid(format!("job {id} not found")))
        }
        _ => Ok(current),
    }
}

/// Queue-internal defer: put a claimed-but-not-executed job back to
/// `pending` with a not-before time (rate-limit path only). The
/// consumed attempt is refunded (`attempt_count - 1`) because the job
/// never ran. This is deliberately NOT a general state transition —
/// the transition table stays strict (a failed job is never moved
/// back to `pending`; retries are new rows).
pub async fn defer_job(
    pool: &SqlitePool,
    id: Uuid,
    available_at: DateTime<Utc>,
) -> Result<SyncJob, RepoNyxError> {
    sqlx::query(
        r#"
        UPDATE sync_jobs
        SET status = 'pending', started_at = NULL, worker_id = NULL,
            lease_expires_at = NULL, heartbeat_at = NULL,
            available_at = ?, attempt_count = MAX(0, attempt_count - 1)
        WHERE id = ? AND status = 'running'
        "#,
    )
    .bind(available_at)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_job(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("job {id} not found")))
}

/// Move a job to a new status, enforcing the allowed-transition table.
/// Sets `completed_at` for terminal statuses.
async fn transition_job(
    pool: &SqlitePool,
    id: Uuid,
    new_status: JobStatus,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> Result<SyncJob, RepoNyxError> {
    let current = get_job(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("job {id} not found")))?;
    if current.status != new_status && !current.status.allowed_transitions().contains(&new_status) {
        return Err(invalid(format!(
            "job status cannot move from {} to {}",
            current.status.as_str(),
            new_status.as_str()
        )));
    }
    let completed_at: Option<DateTime<Utc>> = match new_status {
        JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled => Some(now_utc()),
        _ => None,
    };
    let affected = sqlx::query(
        r#"
        UPDATE sync_jobs
        SET status = ?, completed_at = ?, error_code = ?, error_message = ?
        WHERE id = ?
        "#,
    )
    .bind(new_status.as_str())
    .bind(completed_at)
    .bind(error_code)
    .bind(error_message)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(map_error)?
    .rows_affected();
    debug_assert_eq!(affected, 1, "job {id} must exist for transition");
    get_job(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("job {id} not found")))
}

pub async fn complete_job(pool: &SqlitePool, id: Uuid) -> Result<SyncJob, RepoNyxError> {
    transition_job(pool, id, JobStatus::Completed, None, None).await
}

pub async fn fail_job(
    pool: &SqlitePool,
    id: Uuid,
    error_code: &str,
    error_message: &str,
) -> Result<SyncJob, RepoNyxError> {
    transition_job(
        pool,
        id,
        JobStatus::Failed,
        Some(error_code),
        Some(error_message),
    )
    .await
}

pub async fn cancel_job(pool: &SqlitePool, id: Uuid) -> Result<SyncJob, RepoNyxError> {
    transition_job(pool, id, JobStatus::Cancelled, None, None).await
}

/// Finish a job and record its history entry in one transaction: the
/// job moves to the status implied by `outcome.result` (`success` →
/// `completed`, `failed` → `failed`, `cancelled` → `cancelled`) and
/// the history row is written with the job's type as `operation`.
/// Either both land or the transaction rolls back.
///
/// The transaction phase is retried on `database is locked`: the
/// transaction starts deferred, so its `SELECT` takes a shared lock
/// and the `UPDATE` must upgrade it — when two workers do that at
/// once neither can upgrade (upgrade deadlock) and one side fails
/// fast even with a busy timeout. Retrying the whole transaction is
/// safe (a failed attempt rolls back; the check re-reads the row),
/// and the final read-back is deliberately outside the retry loop
/// so a committed job is never completed twice.
pub async fn complete_job_with_history(
    pool: &SqlitePool,
    job_id: Uuid,
    outcome: JobOutcome,
) -> Result<(SyncJob, SyncHistory), RepoNyxError> {
    let mut attempt = 0;
    let history = loop {
        match complete_job_in_tx(pool, job_id, &outcome).await {
            Ok(history) => break history,
            Err(e) if is_locked(&e) && attempt < 8 => {
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_millis(10 * attempt as u64)).await;
            }
            Err(e) => return Err(e),
        }
    };
    let job = get_job(pool, job_id)
        .await?
        .ok_or_else(|| invalid(format!("job {job_id} not found")))?;
    Ok((job, history))
}

/// True when `err` is a SQLite lock-contention failure. Used to retry
/// a transaction that lost an upgrade race instead of failing the job.
fn is_locked(err: &RepoNyxError) -> bool {
    err.details
        .as_deref()
        .is_some_and(|d| d.contains("database is locked"))
}

async fn complete_job_in_tx(
    pool: &SqlitePool,
    job_id: Uuid,
    outcome: &JobOutcome,
) -> Result<SyncHistory, RepoNyxError> {
    let job_status = match outcome.result {
        SyncResult::Success => JobStatus::Completed,
        SyncResult::Failed => JobStatus::Failed,
        SyncResult::Cancelled => JobStatus::Cancelled,
    };
    let mut tx = pool.begin().await.map_err(map_error)?;
    let current_row = sqlx::query("SELECT * FROM sync_jobs WHERE id = ?")
        .bind(job_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
    let Some(current_row) = current_row else {
        tx.rollback().await.map_err(map_error)?;
        return Err(invalid(format!("job {job_id} not found")));
    };
    let current = row_to_job(&current_row).map_err(map_error)?;
    if current.status != job_status && !current.status.allowed_transitions().contains(&job_status) {
        tx.rollback().await.map_err(map_error)?;
        return Err(invalid(format!(
            "job status cannot move from {} to {}",
            current.status.as_str(),
            job_status.as_str()
        )));
    }
    let completed_at = now_utc();
    sqlx::query(
        r#"
        UPDATE sync_jobs
        SET status = ?, completed_at = ?, error_code = ?, error_message = ?
        WHERE id = ?
        "#,
    )
    .bind(job_status.as_str())
    .bind(completed_at)
    .bind(outcome.error_code.as_deref())
    .bind(outcome.error_message.as_deref())
    .bind(job_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;
    let history = record_history_in_tx(
        &mut tx,
        NewHistory {
            repository_id: current.repository_id,
            job_id: Some(current.id),
            operation: current.job_type.as_str().to_string(),
            result: outcome.result,
            duration_ms: outcome.duration_ms,
            bytes_received: outcome.bytes_received,
            started_at: current.started_at.unwrap_or(current.created_at),
            completed_at: Some(completed_at),
        },
    )
    .await?;
    tx.commit().await.map_err(map_error)?;
    Ok(history)
}
