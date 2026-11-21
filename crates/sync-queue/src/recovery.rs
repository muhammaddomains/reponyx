//! Startup recovery for jobs left in `running` after a crash.
//!
//! Policy: a `running` job whose `started_at` is older than
//! `recovery_grace` (or missing) is treated as abandoned. We mark it
//! `failed` with a stable error code and enqueue a **new** pending job
//! of the same type so work is not lost and is not duplicated under the
//! same row id. Jobs still within the grace window are left alone
//! (another live worker may still own them).

use chrono::{Duration as ChronoDuration, Utc};
use reponyx_core::{JobStatus, JobType, SyncJob};
use reponyx_database::jobs::{
    JobFilter, JobOutcome, NewJob, complete_job_with_history, create_job, list_jobs,
};
use reponyx_core::SyncResult;
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
}

/// Scan `running` jobs and recover abandoned ones.
pub async fn recover_interrupted_jobs(
    pool: &SqlitePool,
    grace: Duration,
) -> Result<RecoveryReport, reponyx_core::RepoNyxError> {
    let running = list_jobs(
        pool,
        JobFilter {
            repository_id: None,
            status: Some(JobStatus::Running),
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
        let started = job.started_at.unwrap_or(job.created_at);
        let age = now.signed_duration_since(started);
        if age < grace_chrono {
            report.skipped_fresh += 1;
            continue;
        }

        warn!(
            job_id = %job.id,
            repository_id = %job.repository_id,
            age_secs = age.num_seconds(),
            "recovering abandoned running job"
        );

        // Mark the abandoned attempt failed with history.
        let _ = complete_job_with_history(
            pool,
            job.id,
            JobOutcome {
                result: SyncResult::Failed,
                duration_ms: Some(age.num_milliseconds().max(0)),
                bytes_received: None,
                error_code: Some("CrashRecovered".to_string()),
                error_message: Some(
                    "Job was still running after process restart; re-queued".to_string(),
                ),
            },
        )
        .await?;

        // New job — domain rule: never move terminal → pending.
        create_job(
            pool,
            NewJob {
                repository_id: job.repository_id,
                job_type: job.job_type,
                priority: job.priority,
            },
        )
        .await?;

        report.requeued += 1;
        info!(job_id = %job.id, "abandoned job failed + requeued");
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
