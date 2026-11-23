//! RepoNyx background agent (Phase 7).
//!
//! Responsibilities (ADR-006 / architecture doc):
//! - Open the shared SQLite database (same path as the desktop).
//! - Recover interrupted sync jobs and start the Phase 6 worker pool.
//! - Periodically materialize due schedules (Phase 7 scheduler crate).
//! - Serve authenticated local IPC for the desktop.
//! - Graceful shutdown on SIGINT/SIGTERM or IPC `shutdown`.
//!
//! This process is intentionally independent of the UI. Start-at-logon
//! registration lives in the desktop (`register_agent_autostart_command`
//! in `apps/desktop/src-tauri/src/agent.rs`, exposed on the Agent page);
//! the agent itself never registers itself.

mod lifecycle;
mod paths;
mod runtime;

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
