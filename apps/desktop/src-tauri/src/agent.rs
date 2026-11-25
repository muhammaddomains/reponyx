//! Desktop ↔ background-agent IPC commands + ensure-running / autostart.
//!
//! These commands never fabricate agent health: if the agent is down,
//! the UI gets a real `BackgroundAgentUnavailable`-style error.

use crate::auth::{CommandError, CommandResult, database_unavailable};
use crate::exe;
use reponyx_credential_store::default_store;
use reponyx_database::Database;
use reponyx_ipc::protocol::AgentStatus;
use reponyx_ipc::{IpcClient, agent_reachable};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealthView {
    pub reachable: bool,
    pub status: Option<AgentStatusView>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusView {
    pub generation: u64,
    pub started_at: String,
    pub protocol_version: u32,
    pub paused: bool,
    pub pending_jobs: i64,
    pub running_jobs: i64,
    pub workers: usize,
    pub last_materialize_at: Option<String>,
    pub last_materialize_created: Option<usize>,
}

impl From<AgentStatus> for AgentStatusView {
    fn from(s: AgentStatus) -> Self {
        Self {
            generation: s.generation,
            started_at: s.started_at.to_rfc3339(),
            protocol_version: s.protocol_version,
            paused: s.paused,
            pending_jobs: s.pending_jobs,
            running_jobs: s.running_jobs,
            workers: s.workers,
            last_materialize_at: s.last_materialize_at.map(|t| t.to_rfc3339()),
            last_materialize_created: s.last_materialize_created,
        }
    }
}

pub(crate) fn data_dir() -> PathBuf {
    // Single source of truth: `reponyx_database::paths` (audit
    // DB-M3). The desktop, the agent, and the broker must all
    // resolve the same directory or they open different databases.
    reponyx_database::data_dir()
}

fn client() -> Result<IpcClient, CommandError> {
    let store = default_store();
    IpcClient::open(data_dir(), &store).map_err(CommandError::from)
}

/// Forward the agent's live progress stream to the frontend as
/// `sync-progress` Tauri events, for as long as the app runs.
///
/// The subscription is a single long-lived IPC connection; if the
/// agent is down (or goes away) the forwarder retries every 10 s,
/// so progress resumes on its own the next time the agent starts.
/// Events carry no secrets (job id, stage label, optional
/// percentage) and are emitted as-is — the Synchronization page
/// validates them before rendering.
pub fn start_progress_forwarder(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = forward_progress_stream(&handle).await;
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

async fn forward_progress_stream(app: &AppHandle) -> Result<(), ()> {
    let client = client().map_err(|_| ())?;
    let mut rx = client.subscribe_progress().await.map_err(|_| ())?;
    loop {
        match rx.recv().await {
            Ok(event) => {
                let _ = app.emit("sync-progress", event);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

/// Stop the background agent if it is running, so a database
/// restore can replace the file it holds open. Also backs the
/// Agent page's "Stop agent" button. Returns `true` when an agent
/// was running and has stopped. The agent is re-spawned on demand
/// (the Agent page's "Start agent", or the next sync enqueue), so
/// stopping it here loses nothing.
///
/// The graceful path is an IPC shutdown; if the agent is wedged
/// (pipe open, no reply) and still reachable after the wait, the
/// last resort is killing the process named by the single-instance
/// lock — only after verifying the PID is alive and is a
/// reponyx-agent, so a recycled PID is never killed by mistake.
pub(crate) async fn stop_agent_if_running() -> CommandResult<bool> {
    let dir = data_dir();
    if !agent_reachable(&dir).await {
        return Ok(false);
    }
    client()?
        .shutdown_agent()
        .await
        .map_err(CommandError::from)?;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if !agent_reachable(&dir).await {
            return Ok(true);
        }
    }
    if kill_agent_by_lock() {
        for _ in 0..8 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if !agent_reachable(&dir).await {
                return Ok(true);
            }
        }
    }
    Err(CommandError {
        code: "InternalError".into(),
        message: "background agent did not stop; the database restore was not staged".into(),
    })
}

/// Kill the agent process named by the single-instance lock, after
/// verifying the PID is alive and is a reponyx-agent. Returns
/// `true` when a process was killed.
fn kill_agent_by_lock() -> bool {
    let Some(lock) = reponyx_ipc::read_lock(&data_dir()) else {
        return false;
    };
    if !reponyx_ipc::pid_alive(lock.pid) || !reponyx_ipc::pid_is_agent(lock.pid) {
        return false;
    }
    #[cfg(windows)]
    {
        let killed = Command::new("taskkill")
            .args(["/PID", &lock.pid.to_string(), "/F"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if killed {
            tracing::warn!(pid = lock.pid, "killed wedged agent by PID");
        }
        killed
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-9", &lock.pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Stop the agent on purpose (Agent page "Stop agent" button and
/// the tray menu). The agent is re-spawned on demand, so this is
/// safe to offer next to "Start agent".
#[tauri::command]
pub async fn stop_agent_command() -> CommandResult<String> {
    if stop_agent_if_running().await? {
        Ok("Background agent stopped.".into())
    } else {
        Ok("Background agent was not running.".into())
    }
}

/// Whether start-at-login is currently enabled (Task Scheduler
/// task or Registry Run key on Windows, systemd unit on Linux,
/// LaunchAgent on macOS), so the Agent page can show the state
/// instead of two blind Enable/Disable buttons.
#[tauri::command]
pub async fn autostart_status_command() -> CommandResult<bool> {
    Ok(autostart_enabled())
}

#[tauri::command]
pub async fn agent_health_command() -> CommandResult<AgentHealthView> {
    let dir = data_dir();
    if !agent_reachable(&dir).await {
        return Ok(AgentHealthView {
            reachable: false,
            status: None,
            message: "Background agent is not running".into(),
        });
    }
    match client()?.status().await {
        Ok(status) => Ok(AgentHealthView {
            reachable: true,
            status: Some(status.into()),
            message: "ok".into(),
        }),
        // The pipe accepted the connection but the agent never
        // answered (or answered with an error): it is wedged, not
        // healthy. Reporting reachable:true here made the UI claim
        // a dead agent was "connected".
        Err(e) => Ok(AgentHealthView {
            reachable: false,
            status: None,
            message: format!("background agent is not responding: {}", e.message),
        }),
    }
}

#[tauri::command]
pub async fn agent_ping_command() -> CommandResult<(u64, u32)> {
    client()?.ping().await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn agent_pause_command() -> CommandResult<()> {
    client()?.pause().await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn agent_resume_command() -> CommandResult<()> {
    client()?.resume().await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn agent_enqueue_command(repository_id: String) -> CommandResult<Option<String>> {
    let id = Uuid::parse_str(&repository_id).map_err(|_| CommandError {
        code: "InvalidInput".into(),
        message: "invalid repository id".into(),
    })?;
    let job = client()?.enqueue(id).await.map_err(CommandError::from)?;
    Ok(job.map(|j| j.to_string()))
}

#[tauri::command]
pub async fn agent_cancel_command(job_id: String) -> CommandResult<()> {
    let id = Uuid::parse_str(&job_id).map_err(|_| CommandError {
        code: "InvalidInput".into(),
        message: "invalid job id".into(),
    })?;
    client()?.cancel_job(id).await.map_err(CommandError::from)
}

/// One live status probe, retried a few times: a single probe can
/// fail when it lands in the microsecond gap where the named pipe
/// has no listening instance, or when the agent goes away between
/// the reachability check and the status call. Neither means the
/// agent is down — `ensure_agent_running_command` must not report
/// "not running" (and skip the spawn) on one such probe.
async fn live_health(dir: &Path) -> Option<AgentHealthView> {
    for attempt in 0..3 {
        if agent_reachable(dir).await {
            if let Ok(health) = agent_health_command().await {
                if health.reachable {
                    return Some(health);
                }
            }
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    None
}

/// Start the agent process if it is not already reachable.
#[tauri::command]
pub async fn ensure_agent_running_command(app: AppHandle) -> CommandResult<AgentHealthView> {
    ensure_agent_running(Some(&app)).await
}

/// Start the agent process if it is not already reachable. The
/// AppHandle lets the lookup check Tauri's resource directory (the
/// installer layout); without it the exe-dir and workspace-target
/// fallbacks still apply.
pub(crate) async fn ensure_agent_running(
    app: Option<&AppHandle>,
) -> CommandResult<AgentHealthView> {
    let dir = data_dir();
    if let Some(health) = live_health(&dir).await {
        return Ok(health);
    }
    let mut child = spawn_agent_process(app)?;
    // Wait briefly for the socket to come up.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        // If the spawned agent died on startup, say so instead of
        // waiting out the clock: the usual cause is that another
        // agent already owns the named pipe (Windows allows only one
        // agent on it — the second one's bind fails with Access
        // denied and it exits at once). In that case the other agent
        // is the one to use, so probe it once more before giving up:
        // a flapping probe can fail here and succeed a moment later.
        if let Ok(Some(status)) = child.try_wait() {
            if let Some(health) = live_health(&dir).await {
                return Ok(health);
            }
            return Ok(AgentHealthView {
                reachable: false,
                status: None,
                message: format!(
                    "agent process exited during startup ({status}) — another agent is probably already running; stop it and try again"
                ),
            });
        }
        if agent_reachable(&dir).await {
            if let Ok(health) = agent_health_command().await {
                if health.reachable {
                    return Ok(health);
                }
            }
            // Reachable a moment ago but the status probe missed:
            // the agent is coming up — keep waiting instead of
            // reporting it down.
        }
    }
    Ok(AgentHealthView {
        reachable: false,
        status: None,
        message: "spawned agent process but it did not become reachable in time".into(),
    })
}

fn spawn_agent_process(app: Option<&AppHandle>) -> Result<Child, CommandError> {
    let exe = resolve_agent_exe(app).ok_or_else(|| CommandError {
        code: "InternalError".into(),
        message: "reponyx-agent executable not found (set REPONYX_AGENT_EXE or install the agent binary on PATH)".into(),
    })?;
    let mut cmd = Command::new(&exe);
    // Detach: no inherited console wait.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    #[cfg(unix)]
    {
        // Best-effort detach: redirect stdio and run without waiting.
        // Full session leadership can be added with `libc` if needed.
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }
    // Keep the handle so the caller can tell a spawn that died on
    // startup (e.g. the pipe is already held) from one that is still
    // coming up. Dropping it does not kill the process.
    cmd.spawn().map_err(|e| CommandError {
        code: "InternalError".into(),
        message: format!("failed to spawn {}: {e}", exe.display()),
    })
}

fn resolve_agent_exe(app: Option<&AppHandle>) -> Option<PathBuf> {
    exe::resolve_exe(app, "REPONYX_AGENT_EXE", &exe::exe_name("reponyx-agent"))
}

/// Register the app to start at user logon (Windows Task Scheduler /
/// Linux systemd user unit / macOS LaunchAgent). On Windows the
/// entry points at the desktop app itself: the desktop starts the
/// agent and the broker when it launches, so "start at login" means
/// the whole system comes up together — not the agent alone, which
/// used to leave the desktop and the broker behind on the next
/// logon. On Linux/macOS the entry still points at the agent
/// (a GUI desktop app is not a systemd/launchd service).
#[tauri::command]
pub async fn register_agent_autostart_command(app: AppHandle) -> CommandResult<String> {
    #[cfg(windows)]
    {
        // The desktop's own executable: autostart launches the app,
        // and the app starts its services.
        // nosemgrep: rust.lang.security.current-exe.current-exe
        let exe = std::env::current_exe().map_err(|e| CommandError {
            code: "InternalError".into(),
            message: format!("could not locate the desktop executable: {e}"),
        })?;
        register_autostart(&exe)
    }
    #[cfg(not(windows))]
    {
        let exe = resolve_agent_exe(Some(&app)).ok_or_else(|| CommandError {
            code: "InternalError".into(),
            message: "reponyx-agent executable not found".into(),
        })?;
        register_autostart(&exe)
    }
}

#[tauri::command]
pub async fn unregister_agent_autostart_command() -> CommandResult<String> {
    unregister_autostart()
}

/// Per-user autostart, Registry Run key
/// (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
/// This is the fallback when Task Scheduler refuses the task: it
/// needs no elevation, no service and no password, and the desktop
/// is a user-session process, so the Run key gives it exactly the
/// same logon behaviour.
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
/// Current autostart entry: the desktop app itself.
const RUN_VALUE_NAME: &str = "RepoNyx";
const TASK_NAME: &str = "RepoNyx";
/// Legacy entries (2026-11-24 and earlier) pointed at the agent
/// binary only. Enabling autostart now removes them so the agent
/// does not also start alone at logon; unregistering removes both.
const LEGACY_RUN_VALUE_NAME: &str = "RepoNyxBackgroundAgent";
const LEGACY_TASK_NAME: &str = "RepoNyxBackgroundAgent";

#[cfg(windows)]
fn task_scheduler_task_present(task: &str) -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", task])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn run_key_present(value: &str) -> bool {
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", value])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn register_run_key(exe: &Path, value: &str) -> Result<(), String> {
    let value_arg = format!("\"{}\"", exe.display());
    let output = Command::new("reg")
        .args([
            "add", RUN_KEY, "/v", value, "/t", "REG_SZ", "/d", &value_arg, "/f",
        ])
        .output()
        .map_err(|e| format!("reg add failed: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(windows)]
fn unregister_run_key(value: &str) -> bool {
    Command::new("reg")
        .args(["delete", RUN_KEY, "/v", value, "/f"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn remove_legacy_autostart() {
    let _ = Command::new("schtasks")
        .args(["/Delete", "/TN", LEGACY_TASK_NAME, "/F"])
        .output();
    unregister_run_key(LEGACY_RUN_VALUE_NAME);
}

#[cfg(windows)]
fn register_autostart(desktop_exe: &Path) -> CommandResult<String> {
    if task_scheduler_task_present(TASK_NAME) {
        return Ok("Start at login is already enabled (Task Scheduler).".into());
    }
    if run_key_present(RUN_VALUE_NAME) {
        return Ok("Start at login is already enabled (Registry Run key).".into());
    }
    // Upgrade: drop the legacy agent-only entries so the agent does
    // not also start alone at logon next to the desktop.
    remove_legacy_autostart();
    // No /RL flag: the task runs at logon with the user's normal
    // token by default, which is all this user-session app needs.
    // The previous version passed /RL LIMITED and schtasks refused
    // the task with "ERROR: Access is denied." when the app ran
    // non-elevated.
    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &format!("\"{}\"", desktop_exe.display()),
            "/SC",
            "ONLOGON",
            "/F",
        ])
        .output()
        .map_err(|e| CommandError {
            code: "InternalError".into(),
            message: format!("schtasks failed: {e}"),
        })?;
    if output.status.success() {
        return Ok("Registered RepoNyx (ONLOGON, Task Scheduler)".into());
    }
    let scheduler_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // Fall back to the per-user Registry Run key so Enable still
    // works on machines where Task Scheduler refuses the task.
    match register_run_key(desktop_exe, RUN_VALUE_NAME) {
        Ok(()) => Ok(format!(
            "Task Scheduler refused the task ({scheduler_error}); start at login was enabled via the Registry Run key instead."
        )),
        Err(reg_error) => Err(CommandError {
            code: "InternalError".into(),
            message: format!(
                "could not enable start at login: Task Scheduler said '{scheduler_error}', Registry Run key said '{reg_error}'"
            ),
        }),
    }
}

#[cfg(windows)]
fn unregister_autostart() -> CommandResult<String> {
    let mut removed = Vec::new();
    let task_gone = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if task_gone {
        removed.push("Task Scheduler");
    }
    if unregister_run_key(RUN_VALUE_NAME) {
        removed.push("Registry Run key");
    }
    // Also drop the legacy agent-only entries.
    let legacy_task_gone = Command::new("schtasks")
        .args(["/Delete", "/TN", LEGACY_TASK_NAME, "/F"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if legacy_task_gone {
        removed.push("legacy Task Scheduler task");
    }
    if unregister_run_key(LEGACY_RUN_VALUE_NAME) {
        removed.push("legacy Registry Run key");
    }
    if removed.is_empty() {
        Ok("Start at login was not enabled.".into())
    } else {
        Ok(format!(
            "Disabled start at login (removed from {}).",
            removed.join(" and ")
        ))
    }
}

#[cfg(windows)]
fn autostart_enabled() -> bool {
    task_scheduler_task_present(TASK_NAME)
        || run_key_present(RUN_VALUE_NAME)
        || task_scheduler_task_present(LEGACY_TASK_NAME)
        || run_key_present(LEGACY_RUN_VALUE_NAME)
}

#[cfg(target_os = "linux")]
fn register_autostart(agent_exe: &Path) -> CommandResult<String> {
    let home = std::env::var_os("HOME").ok_or_else(|| CommandError {
        code: "InternalError".into(),
        message: "HOME not set".into(),
    })?;
    let dir = PathBuf::from(home).join(".config/systemd/user");
    std::fs::create_dir_all(&dir).map_err(|e| CommandError {
        code: "InternalError".into(),
        message: e.to_string(),
    })?;
    let unit = dir.join("reponyx-agent.service");
    let body = format!(
        "[Unit]\nDescription=RepoNyx Background Agent\n\n[Service]\nExecStart={}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n",
        agent_exe.display()
    );
    std::fs::write(&unit, body).map_err(|e| CommandError {
        code: "InternalError".into(),
        message: e.to_string(),
    })?;
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "enable", "--now", "reponyx-agent.service"])
        .status();
    Ok(format!("Wrote {} and enabled user service", unit.display()))
}

#[cfg(target_os = "linux")]
fn unregister_autostart() -> CommandResult<String> {
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "reponyx-agent.service"])
        .status();
    if let Some(home) = std::env::var_os("HOME") {
        let unit = PathBuf::from(home).join(".config/systemd/user/reponyx-agent.service");
        let _ = std::fs::remove_file(unit);
    }
    Ok("Disabled reponyx-agent user service".into())
}

#[cfg(target_os = "linux")]
fn autostart_enabled() -> bool {
    std::env::var_os("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join(".config/systemd/user/reponyx-agent.service")
                .is_file()
        })
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn register_autostart(agent_exe: &Path) -> CommandResult<String> {
    let home = std::env::var_os("HOME").ok_or_else(|| CommandError {
        code: "InternalError".into(),
        message: "HOME not set".into(),
    })?;
    let dir = PathBuf::from(&home).join("Library/LaunchAgents");
    std::fs::create_dir_all(&dir).map_err(|e| CommandError {
        code: "InternalError".into(),
        message: e.to_string(),
    })?;
    let plist = dir.join("com.reponyx.agent.plist");
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.reponyx.agent</string>
  <key>ProgramArguments</key>
  <array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
"#,
        agent_exe.display()
    );
    std::fs::write(&plist, body).map_err(|e| CommandError {
        code: "InternalError".into(),
        message: e.to_string(),
    })?;
    let _ = Command::new("launchctl")
        .args(["load", "-w", &plist.to_string_lossy()])
        .status();
    Ok(format!("Installed {}", plist.display()))
}

#[cfg(target_os = "macos")]
fn unregister_autostart() -> CommandResult<String> {
    if let Some(home) = std::env::var_os("HOME") {
        let plist = PathBuf::from(home).join("Library/LaunchAgents/com.reponyx.agent.plist");
        let _ = Command::new("launchctl")
            .args(["unload", "-w", &plist.to_string_lossy()])
            .status();
        let _ = std::fs::remove_file(plist);
    }
    Ok("Removed com.reponyx.agent LaunchAgent".into())
}

#[cfg(target_os = "macos")]
fn autostart_enabled() -> bool {
    std::env::var_os("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join("Library/LaunchAgents/com.reponyx.agent.plist")
                .is_file()
        })
        .unwrap_or(false)
}

// Silence unused when DB state is present for future expansion.
#[allow(dead_code)]
fn _need_db(db: State<'_, Option<Database>>) -> CommandResult<()> {
    if db.inner().is_none() {
        return Err(database_unavailable());
    }
    Ok(())
}
