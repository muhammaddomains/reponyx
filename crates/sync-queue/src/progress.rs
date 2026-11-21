//! Progress events for the UI and background agent.
//!
//! Events are pure data. The queue broadcasts them; the desktop and
//! agent decide how to surface them. No secrets are ever included.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Live counts exposed on the progress channel and via [`crate::WorkerPoolStats`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: i64,
    pub running: i64,
    pub workers: usize,
}

/// Events emitted while the queue is running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncProgressEvent {
    JobStarted {
        job_id: Uuid,
        repository_id: Uuid,
        attempt: i32,
        operation: String,
    },
    JobProgress {
        job_id: Uuid,
        /// Free-form stage label from the runner / git progress parser.
        stage: String,
        detail: Option<String>,
    },
    JobCompleted {
        job_id: Uuid,
        repository_id: Uuid,
        duration_ms: Option<i64>,
    },
    JobFailed {
        job_id: Uuid,
        repository_id: Uuid,
        error_code: String,
        message: String,
        will_retry: bool,
        next_attempt: Option<i32>,
    },
    JobCancelled {
        job_id: Uuid,
        repository_id: Uuid,
    },
    Stats(QueueStats),
    Recovered {
        requeued: usize,
        failed: usize,
    },
}
