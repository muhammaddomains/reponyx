//! Windows Task Scheduler registration helpers (Phase 7).
//!
//! The agent itself does not require elevation. Registration is a one-shot
//! admin-or-user action that creates a logon trigger for `reponyx-agent.exe`.
//!
//! Limitations (documented in ADR-006): a user-session task does **not**
//! survive logout. A true Windows Service needs a credential-store redesign
//! and is explicitly out of scope here.

#![cfg(windows)]

use std::path::Path;
use std::process::Command;

pub const TASK_NAME: &str = "RepoNyxBackgroundAgent";

/// Build the `schtasks` arguments that register the agent at user logon.
pub fn register_logon_task_args(agent_exe: &Path) -> Vec<String> {
    vec![
        "/Create".into(),
        "/TN".into(),
        TASK_NAME.into(),
        "/TR".into(),
        format!("\"{}\"", agent_exe.display()),
        "/SC".into(),
        "ONLOGON".into(),
        "/RL".into(),
        "LIMITED".into(),
        "/F".into(), // force overwrite
    ]
}

/// Invoke `schtasks` to register the agent. Returns stderr on failure.
pub fn register_logon_task(agent_exe: &Path) -> Result<(), String> {
    let args = register_logon_task_args(agent_exe);
    let output = Command::new("schtasks")
        .args(&args)
        .output()
        .map_err(|e| format!("failed to spawn schtasks: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

pub fn unregister_logon_task() -> Result<(), String> {
    let output = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map_err(|e| format!("failed to spawn schtasks: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}
