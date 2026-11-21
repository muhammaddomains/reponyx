//! Pluggable job execution.
//!
//! The queue owns claim / finish / retry. How a job actually syncs a
//! repository is injected so production can use git-engine + credentials
//! while unit tests use a deterministic mock.

use async_trait::async_trait;
use reponyx_core::{JobType, RepoNyxError, SyncJob};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::progress::SyncProgressEvent;

/// Context passed to every runner invocation.
pub struct JobContext {
    pub pool: SqlitePool,
    pub job: SyncJob,
    pub timeout: Duration,
    pub cancel: CancellationToken,
    /// Optional sink for fine-grained progress (stage lines, etc.).
    /// The queue passes its broadcast sender here; runners forward
    /// git progress events through it.
    pub progress: Option<broadcast::Sender<SyncProgressEvent>>,
}

/// Successful or failed outcome from a runner (before queue-level
/// classification / history write).
#[derive(Debug)]
pub struct JobRunResult {
    pub duration_ms: Option<i64>,
    pub bytes_received: Option<i64>,
}

/// Executes one claimed job. Implementations must honour `cancel` and
/// `timeout` and must not leave the job row in a terminal state — the
/// queue writes history itself.
#[async_trait]
pub trait JobRunner: Send + Sync + 'static {
    async fn run(&self, ctx: JobContext) -> Result<JobRunResult, RepoNyxError>;
}

/// Configurable mock for tests. Returns scripted outcomes in order;
/// when the script is exhausted it succeeds.
pub struct MockJobRunner {
    script: Mutex<Vec<MockOutcome>>,
    /// Observed job ids, for assertions.
    pub seen: Mutex<Vec<Uuid>>,
}

#[derive(Debug, Clone)]
pub enum MockOutcome {
    Success {
        duration_ms: i64,
    },
    Fail {
        code: reponyx_core::ErrorCode,
        message: String,
    },
    /// Block until the context cancel token fires, then return cancelled.
    HangUntilCancel,
    /// Sleep then succeed (for concurrency / timing tests).
    Delay {
        ms: u64,
    },
}

impl MockJobRunner {
    pub fn new(script: Vec<MockOutcome>) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script),
            seen: Mutex::new(Vec::new()),
        })
    }

    pub fn always_ok() -> Arc<Self> {
        Self::new(Vec::new())
    }
}

#[async_trait]
impl JobRunner for MockJobRunner {
    async fn run(&self, ctx: JobContext) -> Result<JobRunResult, RepoNyxError> {
        self.seen.lock().await.push(ctx.job.id);

        let outcome = {
            let mut script = self.script.lock().await;
            if script.is_empty() {
                MockOutcome::Success { duration_ms: 1 }
            } else {
                script.remove(0)
            }
        };

        match outcome {
            MockOutcome::Success { duration_ms } => Ok(JobRunResult {
                duration_ms: Some(duration_ms),
                bytes_received: None,
            }),
            MockOutcome::Fail { code, message } => Err(RepoNyxError::new(code, message)),
            MockOutcome::HangUntilCancel => {
                ctx.cancel.cancelled().await;
                Err(RepoNyxError::new(
                    reponyx_core::ErrorCode::OperationCancelled,
                    "cancelled while hanging",
                ))
            }
            MockOutcome::Delay { ms } => {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(ms)) => {
                        Ok(JobRunResult {
                            duration_ms: Some(ms as i64),
                            bytes_received: None,
                        })
                    }
                    _ = ctx.cancel.cancelled() => {
                        Err(RepoNyxError::new(
                            reponyx_core::ErrorCode::OperationCancelled,
                            "cancelled during delay",
                        ))
                    }
                }
            }
        }
    }
}

/// Helper: map job type to a short operation label for events / logs.
pub fn operation_label(job_type: JobType) -> &'static str {
    job_type.as_str()
}
