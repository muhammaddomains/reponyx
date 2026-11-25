//! Desktop-side Auth Broker lifecycle: resolve + spawn the broker
//! binary, track the child the desktop owns, stop it on quit.
//!
//! The broker is a separate process the desktop starts on demand —
//! at startup, and (best-effort) before any broker command — so the
//! Accounts page never fails just because the broker was not
//! running. The desktop only ever stops a broker it spawned itself;
//! a broker the user started by hand (or via `dev:all`) is left
//! alone on quit.
//!
//! Broker configuration: in dev the spawned broker runs with the
//! repo root as its working directory, so it reads the same `.env`
//! and the same relative `broker-data` dir as `pnpm broker`. For an
//! installed app there is no repo root: the broker runs with the
//! data dir as its working directory and reads its config from
//! `<data_dir>/.env` (via `REPONYX_BROKER_ENV_FILE`), with its
//! state in `<data_dir>/broker-data` — so the user puts the broker
//! variables there once and the installed app just works.

use crate::auth::AuthState;
use crate::exe;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The broker child this desktop process spawned (if any), held so
/// quit can stop exactly what it started — and nothing else.
pub struct BrokerProcess(pub Mutex<Option<Child>>);

fn broker_base_url(app: &AppHandle) -> String {
    app.state::<AuthState>().inner().broker_base_url.clone()
}

fn socket_addr(base_url: &str) -> Option<SocketAddr> {
    let host = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))?;
    let host = host.split('/').next()?;
    host.parse().ok()
}

/// Is the broker answering `/health`? A bare TCP connect is not
/// enough — another process could hold the port — so this sends a
/// minimal HTTP GET and requires a 200 status line. Short timeout:
/// this runs on the Accounts page's refresh path.
pub async fn broker_reachable(app: &AppHandle) -> bool {
    let Some(addr) = socket_addr(&broker_base_url(app)) else {
        return false;
    };
    let connect =
        tokio::time::timeout(Duration::from_secs(1), tokio::net::TcpStream::connect(addr)).await;
    let Ok(Ok(mut stream)) = connect else {
        return false;
    };
    let request = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
    let Ok(Ok(n)) = read else {
        return false;
    };
    std::str::from_utf8(&buf[..n])
        .map(|text| text.contains(" 200 "))
        .unwrap_or(false)
}

fn resolve_broker_exe(app: Option<&AppHandle>) -> Option<PathBuf> {
    exe::resolve_exe(
        app,
        "REPONYX_BROKER_EXE",
        &exe::exe_name("reponyx-auth-broker"),
    )
}

fn spawn_broker_process(app: Option<&AppHandle>) -> Result<Child, String> {
    let exe = resolve_broker_exe(app).ok_or_else(|| {
        "reponyx-auth-broker executable not found (set REPONYX_BROKER_EXE or install the broker binary next to the desktop app)".to_string()
    })?;
    let mut cmd = Command::new(&exe);
    // Working directory + env: dev parity first. When the desktop
    // runs from a repo checkout, the broker inherits the repo root
    // as its CWD, so it reads the repo `.env` and the relative
    // `broker-data` dir — exactly like `pnpm broker`. Otherwise
    // (installed app) the data dir is the CWD, the broker state
    // lives in `<data_dir>/broker-data`, and the config comes from
    // `<data_dir>/.env`.
    match exe::find_repo_root_with_env() {
        Some(root) => {
            cmd.current_dir(root);
        }
        None => {
            let data_dir = crate::agent::data_dir();
            cmd.current_dir(&data_dir);
            cmd.env("REPONYX_BROKER_DATA_DIR", data_dir.join("broker-data"));
            cmd.env("REPONYX_BROKER_ENV_FILE", data_dir.join(".env"));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    #[cfg(unix)]
    {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }
    cmd.spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", exe.display()))
}

/// Start the broker if it is not already reachable. Returns `true`
/// when the broker answers `/health` afterwards. A missing binary
/// is not an error here — the user may run the broker by hand — it
/// just means "not started by the desktop".
pub async fn ensure_broker_running(app: &AppHandle) -> Result<bool, String> {
    if broker_reachable(app).await {
        return Ok(true);
    }
    let child = spawn_broker_process(Some(app))?;
    // Keep the child so quit can stop it. A spawn that died at once
    // (e.g. missing env vars) is detected below by the health poll.
    let state = app.state::<BrokerProcess>();
    *state
        .0
        .lock()
        .map_err(|e| format!("broker state poisoned: {e}"))? = Some(child);
    // Wait briefly for /health to come up.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if broker_reachable(app).await {
            return Ok(true);
        }
        // If the spawned broker died on startup, say so instead of
        // waiting out the clock.
        let exited = {
            let mut guard = state
                .0
                .lock()
                .map_err(|e| format!("broker state poisoned: {e}"))?;
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        *guard = None;
                        Some(status)
                    }
                    _ => None,
                },
                None => None,
            }
        };
        if let Some(status) = exited {
            return Err(format!(
                "broker process exited during startup ({status}) — check the broker configuration (REPONYX_BROKER_APP_ID / private key / setup token)"
            ));
        }
    }
    Ok(false)
}

/// Best-effort ensure for startup and broker commands: failures
/// are logged, never raised — the caller reports the broker as
/// unreachable instead of breaking the page.
pub async fn ensure_broker_best_effort(app: &AppHandle) {
    match ensure_broker_running(app).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("auth broker is not running and could not be started");
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not start the auth broker");
        }
    }
}

/// Stop the broker this desktop process spawned, if any. Returns
/// `true` when a broker child was stopped. A broker started by hand
/// (or by dev:all) is never touched.
pub async fn stop_broker_if_spawned(app: &AppHandle) -> bool {
    let mut child = {
        let state = app.state::<BrokerProcess>();
        let Ok(mut guard) = state.0.lock() else {
            return false;
        };
        match guard.take() {
            Some(child) => child,
            None => return false,
        }
    };
    if let Err(e) = child.kill() {
        tracing::warn!(error = %e, "could not stop the spawned auth broker");
        return false;
    }
    // The child was just killed, so this returns as soon as the OS
    // reaps it.
    match child.wait() {
        Ok(_) => {
            tracing::info!("auth broker stopped");
            true
        }
        Err(e) => {
            tracing::warn!(error = %e, "spawned auth broker did not exit after kill");
            false
        }
    }
}
