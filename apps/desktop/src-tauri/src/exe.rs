//! Resolving the helper binaries the desktop owns (the background
//! agent and the Auth Broker) to an executable path.
//!
//! The lookup order is the same for both binaries, and the result
//! is only a path hint for locating our own bundled binary — never
//! an identity or trust decision: anyone able to replace that binary
//! already controls this machine.
//!
//! 1. An explicit environment override (`REPONYX_AGENT_EXE` /
//!    `REPONYX_BROKER_EXE`) — wins unconditionally, so a stale value
//!    fails loudly at spawn instead of silently picking another
//!    binary.
//! 2. Tauri's resource directory (the installer layout —
//!    `bundle.resources` in `tauri.conf.json`).
//! 3. `<exe dir>/resources/` (the same layout, without an AppHandle).
//! 4. Next to the desktop executable (portable ZIP layout).
//! 5. The workspace `target/{debug,release}/` — dev parity: `cargo
//!    run -p` builds into the workspace target dir while `tauri dev`
//!    runs the desktop from `apps/desktop/src-tauri/target/debug`.
//! 6. `PATH`.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Platform file name for a helper binary (`reponyx-agent.exe` on
/// Windows, `reponyx-agent` elsewhere).
pub fn exe_name(base: &str) -> String {
    #[cfg(windows)]
    {
        format!("{base}.exe")
    }
    #[cfg(not(windows))]
    {
        base.to_string()
    }
}

pub fn resolve_exe(app: Option<&AppHandle>, env_var: &str, file_name: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env_var) {
        return Some(PathBuf::from(p));
    }
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let me = std::env::current_exe().ok()?;
    let exe_dir = me.parent()?.to_path_buf();
    if let Some(app) = app
        && let Ok(dir) = app.path().resource_dir()
    {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let candidate = exe_dir.join("resources").join(file_name);
    if candidate.is_file() {
        return Some(candidate);
    }
    let candidate = exe_dir.join(file_name);
    if candidate.is_file() {
        return Some(candidate);
    }
    if let Some(found) = find_workspace_target_exe(file_name) {
        return Some(found);
    }
    which(file_name)
}

/// Walk up from the desktop executable looking for a workspace
/// `target/{debug,release}/<file_name>` binary — the layout
/// `cargo run -p` produces in dev.
fn find_workspace_target_exe(file_name: &str) -> Option<PathBuf> {
    // Locating a sibling dev binary, not a security decision: the
    // resolved path is only ever executed as the user's own agent
    // or broker binary.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let mut dir = std::env::current_exe().ok()?;
    dir.pop();
    for _ in 0..8 {
        for profile in ["debug", "release"] {
            let candidate = dir.join("target").join(profile).join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn which(file_name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Find the repo root (a directory holding both `.env` and
/// `Cargo.toml`) by walking up from the desktop executable. The
/// broker's working directory in dev is that root: its `.env` holds
/// the broker config and its relative `broker-data` dir holds the
/// same pairing/installation state `pnpm broker` uses. Requiring
/// both files guards against a stray `.env` in an ancestor of an
/// installed app.
pub fn find_repo_root_with_env() -> Option<PathBuf> {
    // Only finds a directory to run the broker from in dev; the
    // broker config itself still comes from env vars / the `.env`
    // file, never from the exe path.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let mut dir = std::env::current_exe().ok()?;
    dir.pop();
    for _ in 0..8 {
        if dir.join(".env").is_file() && dir.join("Cargo.toml").is_file() {
            return Some(dir.clone());
        }
        if !dir.pop() {
            break;
        }
    }
    None
}
