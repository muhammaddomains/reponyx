//! Agent runtime: queue + scheduler ticks + IPC command handling.

use chrono::Utc;
use reponyx_core::{ErrorCode, JobType, RepoNyxError, SyncMode};
use reponyx_credential_store::{CredentialStore, default_store};
use reponyx_database::jobs::{JobFilter, NewJob, create_job, get_job, list_jobs};
use reponyx_database::repositories::get_repository;
use reponyx_ipc::protocol::{AgentStatus, PROTOCOL_VERSION, ServerMessage};
use reponyx_ipc::{AgentCommand, AgentIpcServer};
use reponyx_scheduler::materialize_due;
use reponyx_sync_queue::{
    DefaultCredentialResolver, GitJobRunner, QueueConfig, QueueHandle, WorkerPool,
};
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct AgentState {
    pub pool: SqlitePool,
    pub queue: QueueHandle,
    pub paused: AtomicBool,
    pub generation: u64,
    pub started_at: chrono::DateTime<Utc>,
    pub last_materialize_at: tokio::sync::Mutex<Option<chrono::DateTime<Utc>>>,
    pub last_materialize_created: tokio::sync::Mutex<Option<usize>>,
}

pub async fn start_queue(pool: SqlitePool) -> Result<(QueueHandle, Arc<dyn CredentialStore>), RepoNyxError> {
    let store = default_store();
    let resolver = Arc::new(DefaultCredentialResolver::pat_only(Arc::clone(&store)));
    // Full App support: use default_resolver_from_settings when device.identifier is set.
    let runner = GitJobRunner::new(resolver).into_arc();
    let config = QueueConfig::from_settings(&pool).await;
    let handle = WorkerPool::start(pool, runner, config).await?;
    Ok((handle, store))
}

pub async fn scheduler_loop(pool: SqlitePool, cancel: CancellationToken, state: Arc<AgentState>) {
    let interval = Duration::from_secs(
        std::env::var("REPONYX_SCHEDULER_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30),
    );
    let mut ticker = tokio::time::interval(interval);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = ticker.tick() => {
                if state.paused.load(Ordering::SeqCst) {
                    continue;
                }
                match materialize_due(&pool).await {
                    Ok(report) => {
                        if report.created.len() > 0 || report.checked > 0 {
                            info!(
                                checked = report.checked,
                                created = report.created.len(),
                                skipped_dup = report.skipped_duplicate,
                                errors = report.errors.len(),
                                "scheduler materialize"
                            );
                        }
                        *state.last_materialize_at.lock().await = Some(Utc::now());
                        *state.last_materialize_created.lock().await = Some(report.created.len());
                    }
                    Err(e) => {
                        error!(error = %e, "materialize_due failed");
                    }
                }
            }
        }
    }
}

pub async fn command_loop(
    mut rx: mpsc::Receiver<AgentCommand>,
    state: Arc<AgentState>,
    cancel: CancellationToken,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            AgentCommand::Status { request_id, reply } => {
                let status = build_status(&state).await;
                let _ = reply.send(ServerMessage::Status { request_id, status });
            }
            AgentCommand::Pause { request_id, reply } => {
                state.paused.store(true, Ordering::SeqCst);
                info!("agent paused via ipc");
                let _ = reply.send(ServerMessage::Ack {
                    request_id,
                    ok: true,
                    message: Some("paused".into()),
                    job_id: None,
                });
            }
            AgentCommand::Resume { request_id, reply } => {
                state.paused.store(false, Ordering::SeqCst);
                info!("agent resumed via ipc");
                let _ = reply.send(ServerMessage::Ack {
                    request_id,
                    ok: true,
                    message: Some("resumed".into()),
                    job_id: None,
                });
            }
            AgentCommand::Shutdown { request_id, reply } => {
                let _ = reply.send(ServerMessage::Ack {
                    request_id,
                    ok: true,
                    message: Some("shutting down".into()),
                    job_id: None,
                });
                cancel.cancel();
            }
            AgentCommand::Enqueue {
                request_id,
                repository_id,
                reply,
            } => {
                let result = enqueue_now(&state.pool, repository_id).await;
                let msg = match result {
                    Ok(job_id) => ServerMessage::Ack {
                        request_id,
                        ok: true,
                        message: Some("enqueued".into()),
                        job_id: Some(job_id),
                    },
                    Err(e) => ServerMessage::Ack {
                        request_id,
                        ok: false,
                        message: Some(e.to_string()),
                        job_id: None,
                    },
                };
                let _ = reply.send(msg);
            }
            AgentCommand::CancelJob {
                request_id,
                job_id,
                reply,
            } => {
                // Soft cancel: mark intent in logs; full cooperative cancel
                // is owned by the queue worker cancellation token (future).
                let msg = match get_job(&state.pool, job_id).await {
                    Ok(Some(job)) => {
                        warn!(%job_id, status = ?job.status, "cancel requested (best-effort)");
                        ServerMessage::Ack {
                            request_id,
                            ok: true,
                            message: Some(format!("cancel noted for job in status {:?}", job.status)),
                            job_id: Some(job_id),
                        }
                    }
                    Ok(None) => ServerMessage::Ack {
                        request_id,
                        ok: false,
                        message: Some("job not found".into()),
                        job_id: None,
                    },
                    Err(e) => ServerMessage::Ack {
                        request_id,
                        ok: false,
                        message: Some(e.to_string()),
                        job_id: None,
                    },
                };
                let _ = reply.send(msg);
            }
        }
    }
}

async fn build_status(state: &AgentState) -> AgentStatus {
    let (pending, running, workers) = match state.queue.stats().await {
        Ok(s) => (s.queue.pending, s.queue.running, s.configured_workers),
        Err(_) => (0, 0, 0),
    };
    AgentStatus {
        generation: state.generation,
        started_at: state.started_at,
        protocol_version: PROTOCOL_VERSION,
        paused: state.paused.load(Ordering::SeqCst),
        pending_jobs: pending,
        running_jobs: running,
        workers,
        last_materialize_at: *state.last_materialize_at.lock().await,
        last_materialize_created: *state.last_materialize_created.lock().await,
    }
}

async fn enqueue_now(pool: &SqlitePool, repository_id: Uuid) -> Result<Uuid, RepoNyxError> {
    let repo = get_repository(pool, repository_id)
        .await?
        .ok_or_else(|| {
            RepoNyxError::new(
                ErrorCode::RepositoryNotFound,
                format!("repository {repository_id} not found"),
            )
        })?;
    if !repo.sync_enabled {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "sync is disabled for this repository",
        ));
    }
    // Skip if pending/running already exists.
    for status in [
        reponyx_core::JobStatus::Pending,
        reponyx_core::JobStatus::Running,
    ] {
        let existing = list_jobs(
            pool,
            JobFilter {
                repository_id: Some(repository_id),
                status: Some(status),
            },
        )
        .await?;
        if let Some(j) = existing.into_iter().next() {
            return Ok(j.id);
        }
    }
    let job_type = match repo.local_path {
        Some(_) => JobType::Fetch,
        None => match repo.sync_mode {
            SyncMode::Mirror | SyncMode::Both => JobType::MirrorClone,
            SyncMode::Working => JobType::WorkingClone,
        },
    };
    let job = create_job(
        pool,
        NewJob {
            repository_id,
            job_type,
            priority: 100, // UI "sync now" beats scheduled work
        },
    )
    .await?;
    Ok(job.id)
}

pub async fn bind_ipc(
    data_dir: &Path,
    store: Arc<dyn CredentialStore>,
    generation: u64,
) -> Result<(AgentIpcServer, mpsc::Receiver<AgentCommand>), reponyx_ipc::TransportError> {
    let (tx, rx) = mpsc::channel(64);
    let server = AgentIpcServer::bind(data_dir, &store, generation, tx).await?;
    Ok((server, rx))
}
