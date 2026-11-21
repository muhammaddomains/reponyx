//! Agent process lifecycle: init → run → graceful shutdown.

use crate::paths::{data_dir, db_path, log_dir};
use crate::runtime::{AgentState, bind_ipc, command_loop, scheduler_loop, start_queue};
use chrono::Utc;
use reponyx_database::open_database;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub async fn run_agent() -> Result<(), Box<dyn std::error::Error>> {
    let data = data_dir();
    std::fs::create_dir_all(data.join("data"))?;
    std::fs::create_dir_all(log_dir(&data))?;
    let _ = reponyx_logging::init_logging(&log_dir(&data));

    let generation = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1);

    info!(
        data_dir = %data.display(),
        generation,
        "reponyx-agent starting"
    );

    let database = open_database(&db_path(&data)).await?;
    let pool = database.pool.clone();

    let (queue, store) = start_queue(pool.clone()).await?;
    let state = Arc::new(AgentState {
        pool: pool.clone(),
        queue,
        paused: AtomicBool::new(false),
        generation,
        started_at: Utc::now(),
        last_materialize_at: tokio::sync::Mutex::new(None),
        last_materialize_created: tokio::sync::Mutex::new(None),
    });

    let cancel = CancellationToken::new();
    let (ipc_server, cmd_rx) = bind_ipc(&data, store, generation).await?;

    // OS signals.
    let signal_cancel = cancel.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("signal received — shutting down");
        signal_cancel.cancel();
    });

    let sched_state = Arc::clone(&state);
    let sched_cancel = cancel.clone();
    let sched_handle = tokio::spawn(async move {
        scheduler_loop(sched_state.pool.clone(), sched_cancel, sched_state).await;
    });

    let cmd_state = Arc::clone(&state);
    let cmd_cancel = cancel.clone();
    let cmd_handle = tokio::spawn(async move {
        command_loop(cmd_rx, cmd_state, cmd_cancel).await;
    });

    let ipc_cancel = cancel.clone();
    let ipc_handle = tokio::spawn(async move {
        ipc_server.run(ipc_cancel).await;
    });

    // Wait until cancel, then drain.
    cancel.cancelled().await;
    info!("agent shutting down — stopping queue workers");
    state.queue.shutdown();
    state.queue.join().await;

    ipc_handle.abort();
    sched_handle.abort();
    cmd_handle.abort();
    info!("reponyx-agent stopped");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).ok();
        let mut sigint = signal(SignalKind::interrupt()).ok();
        tokio::select! {
            _ = async {
                if let Some(s) = sigterm.as_mut() {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
            _ = async {
                if let Some(s) = sigint.as_mut() {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[allow(dead_code)]
fn warn_if_no_storage_root() {
    warn!("set storage.root in settings before real git sync");
}
