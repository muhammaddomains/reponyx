//! Sync queue (Phase 6).
//!
//! This crate **runs** sync jobs. The scheduler (Phase 7 partial) only
//! creates `pending` rows in `sync_jobs`; this crate claims them,
//! executes them through a [`JobRunner`], classifies failures, retries
//! with exponential backoff + jitter, recovers interrupted work on
//! startup, and emits progress events.
//!
//! Hard boundaries:
//! - Job status transitions stay on the Phase 2 table. A failed job is
//!   never moved back to `pending` — a **new** job is created for a
//!   retry (see [`reponyx_core::JobStatus::allowed_transitions`]).
//! - Execution is injected via [`JobRunner`]. Production uses
//!   [`GitJobRunner`]; tests use [`MockJobRunner`].
//! - The scheduler crate must never depend on this crate.

pub mod config;
pub mod credentials_resolve;
pub mod executor;
pub mod git_runner;
pub mod pool;
pub mod progress;
pub mod rate_limit;
pub mod recovery;
pub mod retry;
pub mod runner;

pub use config::QueueConfig;
pub use credentials_resolve::{
    CredentialResolver, DefaultCredentialResolver, StaticCredentialResolver,
    default_resolver_from_settings, device_identifier_from_settings, storage_root_from_settings,
};
pub use executor::{ExecutionOutcome, execute_claimed_job};
pub use git_runner::{GitJobRunner, GitRunnerConfig};
pub use pool::{QueueHandle, WorkerPool, WorkerPoolStats};
pub use progress::{QueueStats, SyncProgressEvent};
pub use rate_limit::RateLimiter;
pub use recovery::{RecoveryReport, recover_interrupted_jobs};
pub use retry::{FailureClass, classify_error, compute_backoff, should_retry};
pub use runner::{JobContext, JobRunResult, JobRunner, MockJobRunner, MockOutcome};
