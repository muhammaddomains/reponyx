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

    // Single instance: refuse to start when another agent already
    // holds the lock (its PID is alive and is a reponyx-agent).
    // Without this, the second agent only fails later at bind_ipc
    // with a cryptic "Access is denied" from the named pipe. A stale
    // lock (PID dead, or not our agent) is ignored and overwritten
    // after binding below.
    if let Some(lock) = reponyx_ipc::read_lock(&data)
        && reponyx_ipc::pid_alive(lock.pid)
        && reponyx_ipc::pid_is_agent(lock.pid)
    {
        return Err(format!(
            "another reponyx-agent is already running (pid {})",
            lock.pid
        )
        .into());
    }

    // Bind the IPC endpoint FIRST — before the database opens and
    // before the queue runs recovery. The named pipe / socket is the
    // single-instance arbiter: if another agent already holds it,
    // bind fails here and this process exits before two agents can
    // both recover and claim the same jobs (the old order ran the
    // queue first and only failed later at bind_ipc).
    let cancel = CancellationToken::new();
    let store = reponyx_credential_store::default_store();
    let (ipc_server, cmd_rx) = bind_ipc(&data, store, generation).await?;

    // Record this process in the single-instance lock now that the
    // endpoint is bound, so a second agent refuses to start and the
    // desktop can tell a live agent from a dead one.
    let _ = reponyx_ipc::write_lock(
        &data,
        &reponyx_ipc::AgentLock {
            pid: std::process::id(),
            generation,
            started_at: Utc::now().to_rfc3339(),
        },
    );

    let database = open_database(&db_path(&data)).await?;
    tracing::info!(path = %db_path(&data).display(), "database opened");
    let pool = database.pool.clone();

    // A real sync cannot run without a storage root (the V1 seed is
    // empty and nothing sets it yet). Default to
    // `<data_dir>/repositories` on first run and say so loudly, so
    // the agent never fails a job with "storage.root is not
    // configured" before touching git. The desktop Settings page
    // (Phase 8) can change it later.
    ensure_storage_root(&pool, &data).await?;

    // Startup self-check (audit AG-F): git, storage-root
    // writability, and the credential store are the three things a
    // running agent needs that opening the database does not prove.
    // Warnings only — the agent still starts, because a missing git
    // today could be a PATH problem fixed while the agent runs —
    // but the reason a later job fails is in the log from the
    // first minute, not discovered job by job.
    self_check(&data, &pool).await;

    let (queue, _) = start_queue(pool.clone()).await?;
    let state = Arc::new(AgentState {
        pool: pool.clone(),
        queue,
        paused: AtomicBool::new(false),
        generation,
        started_at: Utc::now(),
        last_materialize_at: tokio::sync::Mutex::new(None),
        last_materialize_created: tokio::sync::Mutex::new(None),
    });

    // Forward the queue's progress events to IPC subscribers. The
    // queue broadcasts typed events; the IPC server forwards JSON
    // values, so a small adapter serializes each event into a JSON
    // broadcast channel whose receiver the server hands to every
    // connection (see `AgentIpcServer::set_progress_source`). The
    // adapter ends on its own when the queue shuts down.
    {
        let (json_tx, json_rx) = tokio::sync::broadcast::channel::<serde_json::Value>(256);
        ipc_server.set_progress_source(json_rx).await;
        let mut queue_rx = state.queue.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = queue_rx.recv().await {
                match serde_json::to_value(event) {
                    Ok(value) => {
                        let _ = json_tx.send(value);
                    }
                    Err(e) => {
                        warn!(error = %e, "could not serialize progress event");
                    }
                }
            }
        });
    }

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
    // Best-effort: a killed agent leaves a stale lock behind, and
    // the next start overwrites it once its PID is dead.
    reponyx_ipc::remove_lock(&data);
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
    #[cfg(windows)]
    {
        // Audit AG-F: Ctrl+C alone missed logoff and shutdown —
        // Windows sends ctrl_close on console close, ctrl_shutdown
        // on system shutdown, and ctrl_logoff on logoff. Any one of
        // them must stop the agent gracefully instead of the
        // process being killed mid-job. A listener that fails to
        // register waits forever instead of ending the select.
        use tokio::signal::windows::{ctrl_c, ctrl_close, ctrl_logoff, ctrl_shutdown};
        tokio::select! {
            _ = async {
                match ctrl_c() {
                    Ok(mut s) => { let _ = s.recv().await; }
                    Err(_) => std::future::pending::<()>().await,
                }
            } => {}
            _ = async {
                match ctrl_close() {
                    Ok(mut s) => { let _ = s.recv().await; }
                    Err(_) => std::future::pending::<()>().await,
                }
            } => {}
            _ = async {
                match ctrl_shutdown() {
                    Ok(mut s) => { let _ = s.recv().await; }
                    Err(_) => std::future::pending::<()>().await,
                }
            } => {}
            _ = async {
                match ctrl_logoff() {
                    Ok(mut s) => { let _ = s.recv().await; }
                    Err(_) => std::future::pending::<()>().await,
                }
            } => {}
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Startup self-check — see the call site in [`run_agent`].
async fn self_check(data: &std::path::Path, pool: &sqlx::SqlitePool) {
    match reponyx_git_engine::detect_git().await {
        Ok(info) => info!(
            git = %info.path.display(),
            version = %info.version_string,
            "git found"
        ),
        Err(e) => warn!(
            error = %e.message,
            "git not found — sync jobs will fail until git is on PATH"
        ),
    }
    let root = match get_setting(pool, "storage.root").await {
        Ok(Some(setting)) if !setting.value.trim().is_empty() => {
            std::path::PathBuf::from(setting.value)
        }
        _ => data.to_path_buf(),
    };
    let probe = root.join(format!(".reponyx-write-test-{}", std::process::id()));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            info!(root = %root.display(), "storage root is writable");
        }
        Err(e) => warn!(
            error = %e,
            root = %root.display(),
            "storage root is not writable — sync jobs will fail"
        ),
    }
    let store = reponyx_credential_store::default_store();
    match store.get(reponyx_ipc::SERVICE_IPC, reponyx_ipc::ACCOUNT_SHARED_SECRET) {
        Ok(Some(_)) => info!("credential store: ipc shared secret present"),
        Ok(None) => warn!(
            "credential store: ipc shared secret missing — the desktop cannot talk to this agent"
        ),
        Err(e) => warn!(
            error = %e.message,
            "credential store is not readable — sync jobs will fail"
        ),
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
