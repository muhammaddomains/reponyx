//! Agent process lifecycle: init → run → graceful shutdown.

use crate::paths::{data_dir, db_path, log_dir};
use crate::runtime::{AgentState, bind_ipc, command_loop, scheduler_loop, start_queue};
use chrono::Utc;
use reponyx_database::open_database;
use reponyx_database::settings::{get_setting, set_setting};
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
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);

    info!(
        data_dir = %data.display(),
        generation,
        "reponyx-agent starting"
    );

    let database = open_database(&db_path(&data)).await?;
    let pool = database.pool.clone();

    // A real sync cannot run without a storage root (the V1 seed is
    // empty and nothing sets it yet). Default to
    // `<data_dir>/repositories` on first run and say so loudly, so
    // the agent never fails a job with "storage.root is not
    // configured" before touching git. The desktop Settings page
    // (Phase 8) can change it later.
    ensure_storage_root(&pool, &data).await?;

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

/// Set `storage.root` to `<data_dir>/repositories` when it is
/// missing or empty. Returns the effective root.
async fn ensure_storage_root(
    pool: &sqlx::SqlitePool,
    data_dir: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let configured = get_setting(pool, "storage.root").await?;
    if let Some(setting) = configured
        && !setting.value.trim().is_empty()
    {
        return Ok(std::path::PathBuf::from(setting.value));
    }
    // `storage.root` is the product root (e.g. `D:\RepoNyx`): the
    // repository layout appends `repositories/github/...` under it.
    // Defaulting to `<data_dir>/repositories` would double the
    // segment (`.../repositories/repositories/github/...`) — found by
    // the first live agent run on 2026-11-21.
    let root = data_dir.to_path_buf();
    set_setting(pool, "storage.root", root.to_string_lossy().as_ref()).await?;
    warn!(
        root = %root.display(),
        "storage.root was not configured; defaulting to <data_dir>"
    );
    Ok(root)
}
