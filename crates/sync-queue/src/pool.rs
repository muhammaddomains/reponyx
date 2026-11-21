//! Bounded worker pool that claims and executes sync jobs.

use reponyx_core::{JobStatus, RepoNyxError};
use reponyx_database::jobs::{claim_next_job, count_jobs_by_status};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Semaphore, broadcast};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::config::QueueConfig;
use crate::executor::{ExecutionOutcome, execute_claimed_job};
use crate::progress::{QueueStats, SyncProgressEvent};
use crate::recovery::recover_interrupted_jobs;
use crate::runner::JobRunner;

/// Live handle to a running pool.
pub struct QueueHandle {
    cancel: CancellationToken,
    joins: Vec<JoinHandle<()>>,
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
        let recovery = recover_interrupted_jobs(&pool, config.recovery_grace).await?;
        info!(
            requeued = recovery.requeued,
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

        let cancel = CancellationToken::new();
        let active_workers = Arc::new(AtomicUsize::new(0));
        // Semaphore caps concurrent executions (same as worker count when
        // each worker holds one permit for the whole job).
        let permits = Arc::new(Semaphore::new(config.max_concurrency));

        let mut joins = Vec::with_capacity(config.max_concurrency);
        for worker_id in 0..config.max_concurrency {
            let pool = pool.clone();
            let runner = Arc::clone(&runner);
            let config = config.clone();
            let cancel = cancel.child_token();
            let events = events.clone();
            let active = Arc::clone(&active_workers);
            let permits = Arc::clone(&permits);

            joins.push(tokio::spawn(async move {
                worker_loop(
                    worker_id, pool, runner, config, cancel, events, active, permits,
                )
                .await;
            }));
        }

        info!(workers = config.max_concurrency, "sync-queue workers started");

        Ok(QueueHandle {
            cancel,
            joins,
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
    pub async fn join(self) {
        self.cancel.cancel();
        for j in self.joins {
            let _ = j.await;
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

async fn worker_loop(
    worker_id: usize,
    pool: SqlitePool,
    runner: Arc<dyn JobRunner>,
    config: QueueConfig,
    cancel: CancellationToken,
    events: broadcast::Sender<SyncProgressEvent>,
    active: Arc<AtomicUsize>,
    permits: Arc<Semaphore>,
) {
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

        let claimed = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                drop(permit);
                break;
            }
            r = claim_next_job(&pool) => r,
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

        active.fetch_add(1, Ordering::Relaxed);
        let outcome = execute_claimed_job(
            &pool,
            job,
            Arc::clone(&runner),
            &config,
            cancel.child_token(),
            Some(events.clone()),
        )
        .await;
        active.fetch_sub(1, Ordering::Relaxed);
        drop(permit);

        match outcome {
            Ok(ExecutionOutcome::FailedWillRetry { delay }) => {
                // Brief pause so a tight retry storm does not spin the pool.
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(delay.min(config.base_backoff)) => {}
                }
            }
            Ok(_) => {}
            Err(err) => {
                error!(
                    worker_id,
                    error = %err.message,
                    code = err.code.as_str(),
                    "execute_claimed_job returned error"
                );
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
