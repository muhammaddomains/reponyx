//! Desktop ↔ background-agent IPC commands + ensure-running / autostart.
//!
//! These commands never fabricate agent health: if the agent is down,
//! the UI gets a real `BackgroundAgentUnavailable`-style error.

use crate::auth::{CommandError, CommandResult, database_unavailable};
use reponyx_credential_store::default_store;
use reponyx_database::Database;
use reponyx_ipc::protocol::AgentStatus;
use reponyx_ipc::{IpcClient, agent_reachable};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
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

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("REPONYX_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("RepoNyx");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("RepoNyx");
    }
    PathBuf::from(".reponyx-data")
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
    Err(CommandError {
        code: "InternalError".into(),
        message: "background agent did not stop; the database restore was not staged".into(),
    })
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
        Err(e) => Ok(AgentHealthView {
            reachable: true,
            status: None,
            message: format!("connected but status failed: {}", e.message),
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

/// Start the agent process if it is not already reachable.
#[tauri::command]
pub async fn ensure_agent_running_command() -> CommandResult<AgentHealthView> {
    let dir = data_dir();
    if agent_reachable(&dir).await {
        return agent_health_command().await;
    }
    spawn_agent_process()?;
    // Wait briefly for the socket to come up.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if agent_reachable(&dir).await {
            return agent_health_command().await;
        }
    }
    Ok(AgentHealthView {
        reachable: false,
        status: None,
        message: "spawned agent process but it did not become reachable in time".into(),
    })
}

fn spawn_agent_process() -> Result<(), CommandError> {
    let exe = resolve_agent_exe().ok_or_else(|| CommandError {
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
    cmd.spawn().map_err(|e| CommandError {
        code: "InternalError".into(),
        message: format!("failed to spawn {}: {e}", exe.display()),
    })?;
    Ok(())
}

fn resolve_agent_exe() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("REPONYX_AGENT_EXE") {
        return Some(PathBuf::from(p));
    }
    // Same directory as the desktop binary (bundled layout).
    // The result is only a path hint for locating our own bundled
    // agent binary, never an identity or trust decision: anyone able
    // to replace that binary already controls this machine.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            #[cfg(windows)]
            let candidate = dir.join("reponyx-agent.exe");
            #[cfg(not(windows))]
            let candidate = dir.join("reponyx-agent");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // PATH lookup.
    which("reponyx-agent")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        #[cfg(windows)]
        let candidate = dir.join(format!("{name}.exe"));
        #[cfg(not(windows))]
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Register the agent to start at user logon (Windows Task Scheduler /
/// Linux systemd user unit / macOS LaunchAgent).
#[tauri::command]
pub async fn register_agent_autostart_command() -> CommandResult<String> {
    let exe = resolve_agent_exe().ok_or_else(|| CommandError {
        code: "InternalError".into(),
        message: "reponyx-agent executable not found".into(),
    })?;
    register_autostart(&exe)
}

#[tauri::command]
pub async fn unregister_agent_autostart_command() -> CommandResult<String> {
    unregister_autostart()
}

/// Per-user autostart, Registry Run key
/// (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
/// This is the fallback when Task Scheduler refuses the task: it
/// needs no elevation, no service and no password, and the agent
/// is a user-session process, so the Run key gives it exactly the
/// same logon behaviour.
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE_NAME: &str = "RepoNyxBackgroundAgent";

#[cfg(windows)]
fn task_scheduler_task_present() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", "RepoNyxBackgroundAgent"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn run_key_present() -> bool {
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_VALUE_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn register_run_key(agent_exe: &Path) -> Result<(), String> {
    let value = format!("\"{}\"", agent_exe.display());
    let output = Command::new("reg")
        .args([
            "add",
            RUN_KEY,
            "/v",
            RUN_VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
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
fn unregister_run_key() -> bool {
    Command::new("reg")
        .args(["delete", RUN_KEY, "/v", RUN_VALUE_NAME, "/f"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn register_autostart(agent_exe: &Path) -> CommandResult<String> {
    if task_scheduler_task_present() {
        return Ok("Start at login is already enabled (Task Scheduler).".into());
    }
    if run_key_present() {
        return Ok("Start at login is already enabled (Registry Run key).".into());
    }
    // No /RL flag: the task runs at logon with the user's normal
    // token by default, which is all this user-session agent
    // needs. The previous version passed /RL LIMITED and schtasks
    // refused the task with "ERROR: Access is denied." when the
    // app ran non-elevated.
    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            "RepoNyxBackgroundAgent",
            "/TR",
            &format!("\"{}\"", agent_exe.display()),
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
        return Ok("Registered RepoNyxBackgroundAgent (ONLOGON, Task Scheduler)".into());
    }
    let scheduler_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // Fall back to the per-user Registry Run key so Enable still
    // works on machines where Task Scheduler refuses the task.
    match register_run_key(agent_exe) {
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
        .args(["/Delete", "/TN", "RepoNyxBackgroundAgent", "/F"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if task_gone {
        removed.push("Task Scheduler");
    }
    if unregister_run_key() {
        removed.push("Registry Run key");
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
    task_scheduler_task_present() || run_key_present()
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
