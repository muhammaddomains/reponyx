//! Single-job lifecycle after a successful claim.
//!
//! Flow:
//! 1. Emit `JobStarted`
//! 2. Call [`JobRunner::run`]
//! 3. On success → `complete_job_with_history` (Success)
//! 4. On failure → classify; either permanent fail, cancel, or fail +
//!    schedule a new retry job after backoff

use chrono::Utc;
use reponyx_core::{JobStatus, RepoNyxError, SyncJob, SyncResult};
use reponyx_database::jobs::{
    JobOutcome, complete_job_with_history, create_retry_job, get_job, is_unique_violation,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::config::QueueConfig;
use crate::progress::SyncProgressEvent;
use crate::retry::{FailureClass, classify_error, compute_backoff, should_retry};
use crate::runner::{JobContext, JobRunner, operation_label};

/// What the worker should do after handling one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Succeeded,
    FailedPermanent,
    FailedWillRetry { delay: Duration },
    Cancelled,
}

/// Run one already-claimed (`running`) job to a terminal state.
///
/// `pool` and `config` are taken by value so the worker can run the
/// job in a child task (panic isolation); both are cheap clones.
pub async fn execute_claimed_job(
    pool: SqlitePool,
    job: SyncJob,
    runner: Arc<dyn JobRunner>,
    config: QueueConfig,
    cancel: CancellationToken,
    events: Option<broadcast::Sender<SyncProgressEvent>>,
) -> Result<ExecutionOutcome, RepoNyxError> {
    let started = Utc::now();
    emit(
        &events,
        SyncProgressEvent::JobStarted {
            job_id: job.id,
            repository_id: job.repository_id,
            attempt: job.attempt_count,
            operation: operation_label(job.job_type).to_string(),
        },
    );

    // Attempt budget: claim already incremented attempt_count. If we are
    // past the budget, fail without calling the runner.
    if job.attempt_count > config.max_attempts as i32 {
        let msg = format!(
            "attempt budget exhausted ({}/{})",
            job.attempt_count, config.max_attempts
        );
        complete_job_with_history(
            &pool,
            job.id,
            JobOutcome {
                result: SyncResult::Failed,
                duration_ms: Some(0),
                bytes_received: None,
                error_code: Some("AttemptBudgetExhausted".to_string()),
                error_message: Some(msg.clone()),
            },
        )
        .await?;
        emit(
            &events,
            SyncProgressEvent::JobFailed {
                job_id: job.id,
                repository_id: job.repository_id,
                error_code: "AttemptBudgetExhausted".to_string(),
                message: msg,
                will_retry: false,
                next_attempt: None,
            },
        );
        return Ok(ExecutionOutcome::FailedPermanent);
    }

    let ctx = JobContext {
        pool: pool.clone(),
        job: job.clone(),
        timeout: config.job_timeout,
        cancel: cancel.child_token(),
        progress: events.clone(),
    };

    let run_result = runner.run(ctx).await;
    let duration_ms = (Utc::now() - started).num_milliseconds().max(0);

    match run_result {
        Ok(result) => {
            complete_job_with_history(
                &pool,
                job.id,
                JobOutcome {
                    result: SyncResult::Success,
                    duration_ms: result.duration_ms.or(Some(duration_ms)),
                    bytes_received: result.bytes_received,
                    error_code: None,
                    error_message: None,
                },
            )
            .await?;
            emit(
                &events,
                SyncProgressEvent::JobCompleted {
                    job_id: job.id,
                    repository_id: job.repository_id,
                    duration_ms: Some(duration_ms),
                },
            );
            info!(job_id = %job.id, duration_ms, "job completed");
            Ok(ExecutionOutcome::Succeeded)
        }
        Err(err) => {
            let class = classify_error(&err);
            handle_failure(&pool, &job, &err, class, duration_ms, &config, &events).await
        }
    }
}

async fn handle_failure(
    pool: &SqlitePool,
    job: &SyncJob,
    err: &RepoNyxError,
    class: FailureClass,
    duration_ms: i64,
    config: &QueueConfig,
    events: &Option<broadcast::Sender<SyncProgressEvent>>,
) -> Result<ExecutionOutcome, RepoNyxError> {
    let result = match class {
        FailureClass::Cancelled => SyncResult::Cancelled,
        _ => SyncResult::Failed,
    };

    complete_job_with_history(
        pool,
        job.id,
        JobOutcome {
            result,
            duration_ms: Some(duration_ms),
            bytes_received: None,
            error_code: Some(err.code.as_str().to_string()),
            error_message: Some(err.message.clone()),
        },
    )
    .await?;

    // Confirm terminal state (guards against double-complete races).
    if let Ok(Some(updated)) = get_job(pool, job.id).await {
        debug!(
            job_id = %job.id,
            status = updated.status.as_str(),
            "job terminal after failure"
        );
        let _ = updated.status; // silence if unused in release
    }

    match class {
        FailureClass::Cancelled => {
            emit(
                events,
                SyncProgressEvent::JobCancelled {
                    job_id: job.id,
                    repository_id: job.repository_id,
                },
            );
            Ok(ExecutionOutcome::Cancelled)
        }
        FailureClass::Permanent => {
            emit(
                events,
                SyncProgressEvent::JobFailed {
                    job_id: job.id,
                    repository_id: job.repository_id,
                    error_code: err.code.as_str().to_string(),
                    message: err.message.clone(),
                    will_retry: false,
                    next_attempt: None,
                },
            );
            warn!(job_id = %job.id, code = err.code.as_str(), "permanent job failure");
            Ok(ExecutionOutcome::FailedPermanent)
        }
        FailureClass::Transient => {
            let will = should_retry(class, job.attempt_count, config.max_attempts);
            if will {
                // `ThreadRng` is not `Send`; scope it so it is dropped
                // before the await below (the executor runs in a
                // spawned task, so its future must be `Send`).
                let delay = {
                    let mut rng = rand::rng();
                    compute_backoff(
                        job.attempt_count as u32,
                        config.base_backoff,
                        config.max_backoff,
                        &mut rng,
                    )
                };
                // The backoff is enforced on the retry row itself
                // (available_at), not by the failing worker's sleep:
                // any worker claiming it before then is refused by
                // the claim query, and the attempt count is carried
                // over so the budget binds across retries.
                let available_at = Utc::now()
                    + chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::seconds(0));
                match create_retry_job(pool, job, available_at).await {
                    Ok(retry) => {
                        emit(
                            events,
                            SyncProgressEvent::JobFailed {
                                job_id: job.id,
                                repository_id: job.repository_id,
                                error_code: err.code.as_str().to_string(),
                                message: err.message.clone(),
                                will_retry: true,
                                next_attempt: Some(retry.attempt_count + 1),
                            },
                        );
                        info!(
                            job_id = %job.id,
                            retry_job_id = %retry.id,
                            delay_ms = delay.as_millis() as u64,
                            "transient failure; retry scheduled"
                        );
                        Ok(ExecutionOutcome::FailedWillRetry { delay })
                    }
                    Err(e) if is_unique_violation(&e) => {
                        // Another writer already queued the same work
                        // (dedupe index); the retry is not lost, it
                        // is just owned by the other row.
                        warn!(
                            job_id = %job.id,
                            "transient failure; retry already queued by another writer"
                        );
                        emit(
                            events,
                            SyncProgressEvent::JobFailed {
                                job_id: job.id,
                                repository_id: job.repository_id,
                                error_code: err.code.as_str().to_string(),
                                message: err.message.clone(),
                                will_retry: true,
                                next_attempt: None,
                            },
                        );
                        Ok(ExecutionOutcome::FailedWillRetry { delay })
                    }
                    Err(e) => Err(e),
                }
            } else {
                emit(
                    events,
                    SyncProgressEvent::JobFailed {
                        job_id: job.id,
                        repository_id: job.repository_id,
                        error_code: err.code.as_str().to_string(),
                        message: err.message.clone(),
                        will_retry: false,
                        next_attempt: None,
                    },
                );
                warn!(job_id = %job.id, "transient failure but attempt budget exhausted");
                Ok(ExecutionOutcome::FailedPermanent)
            }
        }
    }
}

fn emit(tx: &Option<broadcast::Sender<SyncProgressEvent>>, event: SyncProgressEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(event);
    }
}

/// Ensure a job is still in `running` before acting (defensive).
pub fn assert_running(job: &SyncJob) -> Result<(), RepoNyxError> {
    if job.status != JobStatus::Running {
        return Err(RepoNyxError::new(
            reponyx_core::ErrorCode::InvalidInput,
            format!(
                "execute_claimed_job requires status=running, got {}",
                job.status.as_str()
            ),
        ));
    }
    Ok(())
}
