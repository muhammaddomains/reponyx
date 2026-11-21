//! Tauri commands for the Phase 8 Synchronization page.
//!
//! These commands are thin delegates: the rows live in
//! `reponyx-database`. They read job state directly from SQLite, so
//! the page works whether or not the background agent is running.
//! Cancelling a job writes through `request_cancel_job`: a pending
//! job becomes cancelled at once, a running job gets
//! `cancel_requested_at` set and the agent's worker cancels it on
//! its next heartbeat (within one third of the lease). Retrying a
//! job is not a command here — the page re-enqueues the repository
//! through the agent (`agent_enqueue_command` in `agent.rs`), which
//! is the same path "Sync now" uses.
//!
//! Live per-job progress is NOT available on this page yet: the
//! queue emits `JobProgress` events on its broadcast channel, but
//! IPC does not forward them to the desktop. The page polls the job
//! rows instead and says so.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works.

use crate::auth::{CommandResult, invalid_input, require_db};
use reponyx_core::{JobStatus, SyncJob};
use reponyx_database::Database;
use reponyx_database::jobs::{JobFilter, count_jobs_by_status, list_jobs, request_cancel_job};
use serde::Serialize;
use std::str::FromStr;
use tauri::State;
use uuid::Uuid;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncJobDto {
    pub id: String,
    pub repository_id: String,
    pub job_type: String,
    pub status: String,
    pub priority: i64,
    pub attempt_count: i64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub available_at: Option<String>,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub cancel_requested_at: Option<String>,
    pub dedupe_key: Option<String>,
}

impl SyncJobDto {
    fn from_job(job: &SyncJob) -> Self {
        Self {
            id: job.id.to_string(),
            repository_id: job.repository_id.to_string(),
            job_type: job.job_type.as_str().to_string(),
            status: job.status.as_str().to_string(),
            priority: job.priority,
            attempt_count: job.attempt_count,
            started_at: job.started_at.map(|at| at.to_rfc3339()),
            completed_at: job.completed_at.map(|at| at.to_rfc3339()),
            error_code: job.error_code.clone(),
            error_message: job.error_message.clone(),
            created_at: job.created_at.to_rfc3339(),
            available_at: job.available_at.map(|at| at.to_rfc3339()),
            worker_id: job.worker_id.clone(),
            lease_expires_at: job.lease_expires_at.map(|at| at.to_rfc3339()),
            heartbeat_at: job.heartbeat_at.map(|at| at.to_rfc3339()),
            cancel_requested_at: job.cancel_requested_at.map(|at| at.to_rfc3339()),
            dedupe_key: job.dedupe_key.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusCountDto {
    pub status: String,
    pub count: i64,
}

// -- Commands ---------------------------------------------------------

#[tauri::command]
pub async fn list_sync_jobs_command(
    db: State<'_, Option<Database>>,
    status: Option<String>,
    repository_id: Option<String>,
) -> CommandResult<Vec<SyncJobDto>> {
    let database = require_db(&db)?;
    let status = status
        .map(|value| {
            JobStatus::from_str(&value)
                .map_err(|_| invalid_input(format!("unknown job status: {value}")))
        })
        .transpose()?;
    let repository_id = repository_id
        .map(|id| {
            Uuid::parse_str(&id).map_err(|_| invalid_input(format!("invalid repository id: {id}")))
        })
        .transpose()?;
    let jobs = list_jobs(
        &database.pool,
        JobFilter {
            repository_id,
            status,
        },
    )
    .await?;
    Ok(jobs.iter().map(SyncJobDto::from_job).collect())
}

#[tauri::command]
pub async fn count_sync_jobs_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<Vec<JobStatusCountDto>> {
    let database = require_db(&db)?;
    let counts = count_jobs_by_status(&database.pool).await?;
    Ok(counts
        .iter()
        .map(|count| JobStatusCountDto {
            status: count.status.as_str().to_string(),
            count: count.count,
        })
        .collect())
}

/// Cancel a job. A pending job is cancelled immediately; a running
/// job gets a cancel request the agent's worker honors on its next
/// heartbeat. The returned job shows which one happened.
#[tauri::command]
pub async fn cancel_sync_job_command(
    db: State<'_, Option<Database>>,
    id: String,
) -> CommandResult<SyncJobDto> {
    let database = require_db(&db)?;
    let id = Uuid::parse_str(&id).map_err(|_| invalid_input(format!("invalid job id: {id}")))?;
    let job = request_cancel_job(&database.pool, id).await?;
    Ok(SyncJobDto::from_job(&job))
}
