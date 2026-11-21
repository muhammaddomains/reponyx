//! RepoNyx background agent (Phase 7).
//!
//! Responsibilities (ADR-006 / architecture doc):
//! - Open the shared SQLite database (same path as the desktop).
//! - Recover interrupted sync jobs and start the Phase 6 worker pool.
//! - Periodically materialize due schedules (Phase 7 scheduler crate).
//! - Serve authenticated local IPC for the desktop.
//! - Graceful shutdown on SIGINT/SIGTERM or IPC `shutdown`.
//!
//! This process is intentionally independent of the UI. Task Scheduler /
//! launchd registration is handled outside the hot path (see
//! `windows_task.rs` helpers and docs).

mod lifecycle;
mod paths;
mod runtime;
#[cfg(windows)]
mod windows_task;

use lifecycle::run_agent;
use tracing::error;

#[tokio::main]
async fn main() {
    if let Err(e) = run_agent().await {
        eprintln!("reponyx-agent failed: {e}");
        error!(error = %e, "agent exited with error");
        std::process::exit(1);
    }
}
