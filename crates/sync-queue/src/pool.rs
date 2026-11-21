//! Bounded worker pool that claims and executes sync jobs.

use chrono::Utc;
use reponyx_core::{JobStatus, RepoNyxError, SyncResult};
use reponyx_database::jobs::{
    JobOutcome, claim_next_job, complete_job_with_history, count_jobs_by_status, defer_job,
    heartbeat_job,
};
use reponyx_database::repositories::get_repository;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::{Semaphore, broadcast};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::config::QueueConfig;
use crate::executor::{ExecutionOutcome, execute_claimed_job};
use crate::progress::{QueueStats, SyncProgressEvent};
use crate::rate_limit::RateLimiter;
use crate::recovery::recover_interrupted_jobs;
use crate::runner::JobRunner;

/// Live handle to a running pool.
pub struct QueueHandle {
    cancel: CancellationToken,
    joins: std::sync::Mutex<Vec<JoinHandle<()>>>,
    events: broadcast::Sender<SyncProgressEvent>,
    active_workers: Arc<AtomicUsize>,
    pool: SqlitePool,
    config: QueueConfig,
}

/// Snapshot of pool activity.
#[derive(Debug, Clone)]
pub struct WorkerPoolStats {
    pub configured_workers: usize,
    pub active_workers: usize,
    pub queue: QueueStats,
}

/// Spawns and supervises workers.
pub struct WorkerPool;

impl WorkerPool {
    /// Recover abandoned jobs, then spawn `config.max_concurrency` workers.
    pub async fn start(
        pool: SqlitePool,
        runner: Arc<dyn JobRunner>,
        config: QueueConfig,
    ) -> Result<QueueHandle, RepoNyxError> {
        let recovery =
            recover_interrupted_jobs(&pool, config.recovery_grace, config.max_attempts).await?;
        info!(
            requeued = recovery.requeued,
            failed_only = recovery.failed_only,
            examined = recovery.examined,
            "sync-queue recovery complete"
        );

        let (events, _) = broadcast::channel(256);
        if recovery.requeued > 0 || recovery.failed_only > 0 {
            let _ = events.send(SyncProgressEvent::Recovered {
                requeued: recovery.requeued,
                failed: recovery.failed_only,
            });
        }

        let limiter = Arc::new(RateLimiter::new(
            config.global_jobs_per_minute,
            config.per_account_jobs_per_minute,
        ));
        let cancel = CancellationToken::new();
        let active_workers = Arc::new(AtomicUsize::new(0));
        // Semaphore caps concurrent executions (same as worker count when
        // each worker holds one permit for the whole job).
        let permits = Arc::new(Semaphore::new(config.max_concurrency));

        let mut joins = Vec::with_capacity(config.max_concurrency);
        for worker_id in 0..config.max_concurrency {
            let args = WorkerArgs {
                pool: pool.clone(),
                runner: Arc::clone(&runner),
                config: config.clone(),
                cancel: cancel.child_token(),
                events: events.clone(),
                active: Arc::clone(&active_workers),
                permits: Arc::clone(&permits),
                limiter: Arc::clone(&limiter),
            };

            joins.push(tokio::spawn(async move {
                worker_loop(worker_id, args).await;
            }));
        }

        info!(
            workers = config.max_concurrency,
            "sync-queue workers started"
        );

        Ok(QueueHandle {
            cancel,
            joins: std::sync::Mutex::new(joins),
            events,
            active_workers,
            pool,
            config,
        })
    }
}

impl QueueHandle {
    /// Subscribe to progress events.
    pub fn subscribe(&self) -> broadcast::Receiver<SyncProgressEvent> {
        self.events.subscribe()
    }

    /// Broadcast sender for external instrumentation.
    pub fn event_sender(&self) -> broadcast::Sender<SyncProgressEvent> {
        self.events.clone()
    }

    /// Request graceful shutdown; workers finish the current job then exit.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Wait for all workers to exit after [`Self::shutdown`].
    /// A worker that panicked is logged instead of silently shrinking
    /// the pool; the job it owned was already failed by the worker
    /// loop's panic guard.
    pub async fn join(&self) {
        self.cancel.cancel();
        let joins = self
            .joins
            .lock()
            .map(|mut g| std::mem::take(&mut *g))
            .unwrap_or_default();
        for j in joins {
            if let Err(e) = j.await {
                error!(error = %e, "sync-queue worker task ended with an error");
            }
        }
    }

    pub async fn stats(&self) -> Result<WorkerPoolStats, RepoNyxError> {
        let counts = count_jobs_by_status(&self.pool).await?;
        let mut pending = 0i64;
        let mut running = 0i64;
        for c in counts {
            match c.status {
                JobStatus::Pending => pending = c.count,
                JobStatus::Running => running = c.count,
                _ => {}
            }
        }
        Ok(WorkerPoolStats {
            configured_workers: self.config.max_concurrency,
            active_workers: self.active_workers.load(Ordering::Relaxed),
            queue: QueueStats {
                pending,
                running,
                workers: self.config.max_concurrency,
            },
        })
    }
}

/// Everything a worker needs, bundled so the loop stays under the
/// argument-count lint.
struct WorkerArgs {
    pool: SqlitePool,
    runner: Arc<dyn JobRunner>,
    config: QueueConfig,
    cancel: CancellationToken,
    events: broadcast::Sender<SyncProgressEvent>,
    active: Arc<AtomicUsize>,
    permits: Arc<Semaphore>,
    limiter: Arc<RateLimiter>,
}

async fn worker_loop(worker_id: usize, args: WorkerArgs) {
    let WorkerArgs {
        pool,
        runner,
        config,
        cancel,
        events,
        active,
        permits,
        limiter,
    } = args;
    let worker_name = format!("worker-{worker_id}");
    debug!(worker_id, "worker started");
    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Acquire a concurrency permit before claiming so we never hold
        // more running jobs than configured.
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            p = permits.acquire() => match p {
                Ok(p) => p,
                Err(_) => break,
            },
        };

        let worker_name_claim = worker_name.clone();
        let lease = config.lease_duration;
        let claimed = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                drop(permit);
                break;
            }
            r = claim_next_job(&pool, &worker_name_claim, lease) => r,
        };

        let job = match claimed {
            Ok(Some(job)) => job,
            Ok(None) => {
                drop(permit);
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(config.claim_poll) => continue,
                }
            }
            Err(err) => {
                drop(permit);
                error!(worker_id, error = %err.message, "claim_next_job failed");
                tokio::time::sleep(config.claim_poll).await;
                continue;
            }
        };

        // Rate-limit gate: refuse the job (defer, do not fail it) when
        // the global or per-account budget is spent. The consumed
        // attempt is refunded by `defer_job`.
        let mut rate_limited: Option<Duration> = None;
        if let Ok(Some(repo)) = get_repository(&pool, job.repository_id).await
            && let Err(wait) = limiter.try_acquire(&repo.account_id.to_string())
        {
            rate_limited = Some(wait);
        }
        if let Some(wait) = rate_limited {
            let available_at = Utc::now()
                + chrono::Duration::from_std(wait).unwrap_or(chrono::Duration::seconds(0));
            if let Err(e) = defer_job(&pool, job.id, available_at).await {
                error!(worker_id, job_id = %job.id, error = %e.message, "defer_job failed");
            }
            drop(permit);
            // Sleep at most 5 s: the job is not claimable before
            // `available_at` anyway, and other work may be ready.
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(wait.min(Duration::from_secs(5))) => continue,
            }
        }

        active.fetch_add(1, Ordering::Relaxed);
        let job_cancel = cancel.child_token();

        // Heartbeat task: renew the lease and watch for a cooperative
        // cancel request on the job row.
        let heartbeat_handle = tokio::spawn(heartbeat_loop(
            pool.clone(),
            job.id,
            worker_name.clone(),
            config.lease_duration,
            job_cancel.clone(),
        ));

        // Execute in a child task so a runner panic cannot kill this
        // worker: on panic the job is failed (not lost) and the
        // worker keeps looping.
        let exec_pool = pool.clone();
        let exec_job = job.clone();
        let exec_runner = Arc::clone(&runner);
        let exec_config = config.clone();
        let exec_events = events.clone();
        let exec_cancel = job_cancel.child_token();
        let exec_task = tokio::spawn(async move {
            execute_claimed_job(
                exec_pool,
                exec_job,
                exec_runner,
                exec_config,
                exec_cancel,
                Some(exec_events),
            )
            .await
        });

        let outcome = exec_task.await;
        heartbeat_handle.abort();
        active.fetch_sub(1, Ordering::Relaxed);
        drop(permit);

        match outcome {
            Ok(Ok(ExecutionOutcome::FailedWillRetry { .. })) => {
                // No sleep here: the backoff is enforced on the retry
                // row (`available_at`), so the worker can immediately
                // claim other work.
            }
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                error!(
                    worker_id,
                    error = %err.message,
                    code = err.code.as_str(),
                    "execute_claimed_job returned error"
                );
            }
            Err(join_err) => {
                error!(
                    worker_id,
                    job_id = %job.id,
                    error = %join_err,
                    "job execution task panicked; failing the job"
                );
                let _ = complete_job_with_history(
                    &pool,
                    job.id,
                    JobOutcome {
                        result: SyncResult::Failed,
                        duration_ms: None,
                        bytes_received: None,
                        error_code: Some("WorkerPanic".to_string()),
                        error_message: Some("job execution task panicked".to_string()),
                    },
                )
                .await;
                let _ = events.send(SyncProgressEvent::JobFailed {
                    job_id: job.id,
                    repository_id: job.repository_id,
                    error_code: "WorkerPanic".to_string(),
                    message: "job execution task panicked".to_string(),
                    will_retry: false,
                    next_attempt: None,
                });
            }
        }

        // Publish stats occasionally.
        if let Ok(counts) = count_jobs_by_status(&pool).await {
            let mut pending = 0i64;
            let mut running = 0i64;
            for c in counts {
                match c.status {
                    JobStatus::Pending => pending = c.count,
                    JobStatus::Running => running = c.count,
                    _ => {}
                }
            }
            let _ = events.send(SyncProgressEvent::Stats(QueueStats {
                pending,
                running,
                workers: config.max_concurrency,
            }));
        }
    }
    debug!(worker_id, "worker stopped");
}

/// Renew the lease on a running job every `lease / 3` and watch for a
/// cooperative cancel request. Ends when the job token is cancelled
/// (shutdown / job finished) or the job is no longer owned by this
/// worker (heartbeat returns `None`).
async fn heartbeat_loop(
    pool: SqlitePool,
    job_id: Uuid,
    worker_id: String,
    lease: Duration,
    cancel: CancellationToken,
) {
    let interval = lease / 3;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {
                match heartbeat_job(&pool, job_id, &worker_id, lease).await {
                    Ok(Some(job)) => {
                        if job.cancel_requested_at.is_some() {
                            info!(job_id = %job_id, "cancel requested; cancelling job");
                            cancel.cancel();
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        debug!(job_id = %job_id, error = %e.message, "heartbeat failed; will retry next tick");
                    }
                }
            }
        }
    }
}
