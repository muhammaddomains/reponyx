//! Agent single-instance lock: `<data_dir>/agent.lock`.
//!
//! The agent writes its PID + generation after binding the IPC
//! endpoint; the desktop reads it to tell a live-but-unreachable
//! agent (restart it) from a dead one (spawn a fresh agent). A stale
//! lock (PID dead, or not our agent) is ignored and overwritten.
//! The lock is best-effort coordination — the named pipe / socket
//! remains the ultimate arbiter of who owns the endpoint.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the lock file holds. JSON, so it stays readable by hand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentLock {
    pub pid: u32,
    pub generation: u64,
    pub started_at: String,
}

pub fn lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join("agent.lock")
}

/// Read the lock file, if it exists and parses. A corrupt file is
/// treated as no lock (a stale/corrupt lock must never block a
/// fresh agent).
pub fn read_lock(data_dir: &Path) -> Option<AgentLock> {
    let text = std::fs::read_to_string(lock_path(data_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_lock(data_dir: &Path, lock: &AgentLock) -> std::io::Result<()> {
    let text = serde_json::to_string(lock)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(lock_path(data_dir), text)
}

/// Best-effort removal (a killed agent leaves a stale lock behind;
/// the next start overwrites it).
pub fn remove_lock(data_dir: &Path) {
    let _ = std::fs::remove_file(lock_path(data_dir));
}

/// A PID the OS could actually have assigned. Rejects 0 (which
/// `kill` reads as "my process group") and values above `i32::MAX` —
/// `u32::MAX` wraps to -1, and `kill(-1, 0)` means "every process",
/// so an unchecked call would report a dead PID as alive.
fn valid_pid(pid: u32) -> bool {
    pid > 0 && pid <= i32::MAX as u32
}

/// A `Command` for a Windows console program (`tasklist`), already
/// hidden: the desktop is a GUI app and the agent runs detached,
/// so a child console program would otherwise open a visible cmd
/// window on every liveness check. Windows-only — off Windows the
/// liveness checks use `kill` instead, so this helper does not
/// exist there (an ungated helper would be dead code on Linux and
/// fail Clippy with `-D warnings`).
#[cfg(windows)]
fn hidden_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new(program);
    cmd.creation_flags(0x0800_0000);
    cmd
}

/// Is this PID a live process? Same-user only — the agent and the
/// desktop always run as the same user.
pub fn pid_alive(pid: u32) -> bool {
    if !valid_pid(pid) {
        return false;
    }
    #[cfg(windows)]
    {
        // `tasklist` prints the row when the PID exists, and
        // "INFO: No tasks are running..." when it does not.
        let output = hidden_command("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                !text.contains("No tasks are running")
            }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        // `kill -0` succeeds exactly when the process exists and we
        // may signal it (same user).
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Is this PID a reponyx-agent process? Verifies the process image
/// name, so a recycled PID (now some other program) is never
/// mistaken for our agent — and never killed by the desktop's
/// restart path. Best-effort: when the check cannot run, returns
/// false and the caller must not act on the PID.
pub fn pid_is_agent(pid: u32) -> bool {
    if !valid_pid(pid) {
        return false;
    }
    #[cfg(windows)]
    {
        let output = hidden_command("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output();
        let Ok(out) = output else {
            return false;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        // First quoted field is the image name, e.g.
        // `"reponyx-agent.exe","1234",...`.
        let Some(name) = text.split('"').nth(1) else {
            return false;
        };
        name.eq_ignore_ascii_case("reponyx-agent.exe")
    }
    #[cfg(not(windows))]
    {
        // Linux: /proc/<pid>/cmdline holds the argv, NUL-separated.
        if let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) {
            let text = String::from_utf8_lossy(&cmdline);
            return text.split('\0').any(|part| part.contains("reponyx-agent"));
        }
        // macOS (no /proc): fall back to `ps`.
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains("reponyx-agent"),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_roundtrip_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let lock = AgentLock {
            pid: std::process::id(),
            generation: 42,
            started_at: "2026-11-25T00:00:00Z".into(),
        };
        write_lock(dir.path(), &lock).unwrap();
        assert_eq!(read_lock(dir.path()), Some(lock));
        remove_lock(dir.path());
        assert_eq!(read_lock(dir.path()), None);
    }

    #[test]
    fn corrupt_lock_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(lock_path(dir.path()), "not json").unwrap();
        assert_eq!(read_lock(dir.path()), None);
    }

    #[test]
    fn own_pid_is_alive_but_not_an_agent_binary() {
        // The test runner is alive, but it is not reponyx-agent, so
        // the image-name check must refuse it (this is what stops a
        // recycled PID from ever being killed).
        let me = std::process::id();
        assert!(pid_alive(me));
        assert!(!pid_is_agent(me));
    }

    #[test]
    fn dead_pid_is_not_alive_and_not_an_agent() {
        // A PID that can never exist.
        assert!(!pid_alive(u32::MAX));
        assert!(!pid_is_agent(u32::MAX));
    }
}
