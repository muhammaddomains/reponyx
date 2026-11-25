//! Startup recovery for jobs left in `running` after a crash.
//!
//! Policy: a `running` job is abandoned only when its **lease has
//! expired** (`lease_expires_at <= now`). Jobs written before leases
//! existed (no `lease_expires_at`) fall back to the old grace rule:
//! `started_at` older than `recovery_grace` (or missing). Jobs still
//! within their lease are left alone — another live worker may still
//! own them.
//!
//! An abandoned attempt is marked `failed` (error code
//! `CrashRecovered`) with a history entry, and a new pending job of
//! the same type is created **carrying the attempt count**, so the
//! retry budget still binds. When the budget is already exhausted
//! the job is failed only (`failed_only`).

use chrono::{Duration as ChronoDuration, Utc};
use reponyx_core::SyncResult;
use reponyx_core::{JobStatus, JobType, SyncJob};
use reponyx_database::jobs::{
    JobFilter, JobOutcome, complete_job_with_history, create_retry_job, is_unique_violation,
    list_jobs,
};
use sqlx::SqlitePool;
use std::time::Duration;
use tracing::{info, warn};

/// Summary of a recovery pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub examined: usize,
    pub requeued: usize,
    pub failed_only: usize,
    pub skipped_fresh: usize,
    /// Jobs that could not be recovered because their own rows
    /// failed to update (bad row, transient DB lock). One bad job
    /// never aborts the pass — it is counted here and the pass
    /// moves on to the next job (audit AG-F3).
    pub errors: usize,
}

/// Scan `running` jobs and recover abandoned ones.
///
/// Per-job fault-isolated: a failure to complete or requeue one
/// job is logged and counted (`errors`), never returned — a single
/// bad row or a transient DB lock used to fail the whole pass,
/// and the pool start with it, taking the entire agent down
/// (audit AG-F3). Only listing the running jobs can still fail
/// the pass (the database itself is unreachable); the caller
/// decides what that means.
pub async fn recover_interrupted_jobs(
    pool: &SqlitePool,
    grace: Duration,
    max_attempts: u32,
) -> Result<RecoveryReport, reponyx_core::RepoNyxError> {
    let running = list_jobs(
        pool,
        JobFilter {
            repository_id: None,
            status: Some(JobStatus::Running),
            ..Default::default()
        },
    )
    .await?;

    let mut report = RecoveryReport {
        examined: running.len(),
        ..RecoveryReport::default()
    };

    let now = Utc::now();
    let grace_chrono = ChronoDuration::from_std(grace).unwrap_or(ChronoDuration::seconds(30));

    for job in running {
        let abandoned = match job.lease_expires_at {
            Some(expires_at) => expires_at <= now,
            None => {
                let started = job.started_at.unwrap_or(job.created_at);
                now.signed_duration_since(started) >= grace_chrono
            }
        };
        if !abandoned {
            report.skipped_fresh += 1;
            continue;
        }

        warn!(
            job_id = %job.id,
            repository_id = %job.repository_id,
            "recovering abandoned running job"
        );

        // Mark the abandoned attempt failed with history.
        if let Err(e) = complete_job_with_history(
            pool,
            job.id,
            JobOutcome {
                result: SyncResult::Failed,
                duration_ms: Some(
                    now.signed_duration_since(job.started_at.unwrap_or(job.created_at))
                        .num_milliseconds()
                        .max(0),
                ),
                bytes_received: None,
                error_code: Some("CrashRecovered".to_string()),
                error_message: Some(
                    "Job was still running after process restart; re-queued".to_string(),
                ),
            },
        )
        .await
        {
            warn!(job_id = %job.id, error = %e.message, "could not fail the abandoned job; skipping it this pass");
            report.errors += 1;
            continue;
        }

        if job.attempt_count >= max_attempts as i32 {
            // Budget already exhausted: fail only, do not requeue.
            report.failed_only += 1;
            info!(job_id = %job.id, "abandoned job failed; attempt budget exhausted");
            continue;
        }

        // New job — domain rule: never move terminal → pending.
        // The retry carries the attempt count so the budget binds.
        match create_retry_job(pool, &job, now).await {
            Ok(_) => {
                report.requeued += 1;
                info!(job_id = %job.id, "abandoned job failed + requeued");
            }
            Err(e) if is_unique_violation(&e) => {
                // Another writer already queued the same work.
                report.requeued += 1;
                info!(job_id = %job.id, "abandoned job failed; retry already queued");
            }
            Err(e) => {
                warn!(job_id = %job.id, error = %e.message, "could not requeue the abandoned job; it stays failed this pass");
                report.errors += 1;
            }
        }
    }

    Ok(report)
}

/// Whether two jobs are the same logical work unit (used by tests).
pub fn same_work(a: &SyncJob, b: &SyncJob) -> bool {
    a.repository_id == b.repository_id && a.job_type == b.job_type
}

/// Job types that the queue is willing to execute in Phase 6.
pub fn is_supported_job_type(t: JobType) -> bool {
    matches!(
        t,
        JobType::MirrorClone | JobType::WorkingClone | JobType::Fetch
    )
}
