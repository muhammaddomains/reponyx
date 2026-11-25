//! Safe process execution: arg arrays, timeout, cancellation,
//! redacted output. No shell is ever involved.

use crate::commands::GitCommand;
use crate::progress::{ProgressEvent, parse_progress_line};
use reponyx_core::{ErrorCode, RepoNyxError};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// Outcome of a finished Git process.
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    /// stdout, redacted.
    pub stdout: String,
    /// stderr, redacted.
    pub stderr: String,
}

/// Kill a git child **and its descendants**.
///
/// `child.kill()` alone only kills the direct child: `git clone` /
/// `git fetch` spawn `git-remote-https`, `index-pack` and (for LFS
/// repos) `git-lfs` as separate processes, and those orphans keep
/// their locks and keep writing after the parent is gone — the
/// lock is released while an orphan still writes (audit G2).
///
/// Unix: every git child is spawned in its own process group
/// (`process_group(0)`, so the child is its own group leader)
/// and the whole group is killed with `kill(-pgid, SIGKILL)`.
/// The group kill only runs when the child really is a group
/// leader (`getpgid(pid) == pid`), so a failed `setpgid` can never
/// turn this into a kill of the agent's own process group.
/// Windows: `taskkill /PID <pid> /T /F` kills the process tree.
/// Either way the direct child is killed and reaped afterwards,
/// so a failed tree kill still leaves no zombie.
async fn kill_process_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        #[cfg(unix)]
        {
            // Safe wrappers from `nix` — no `unsafe` in this
            // crate's code, so Semgrep's unsafe-usage rule stays
            // quiet and the kill stays auditable in one place.
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::{Pid, getpgid};
            let pid = Pid::from_raw(pid as i32);
            let is_group_leader = getpgid(Some(pid)).is_ok_and(|pgid| pgid == pid);
            if is_group_leader {
                let _ = kill(Pid::from_raw(-pid.as_raw()), Signal::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

/// Spawn a git child in its own process group (Unix) so
/// [`kill_process_tree`] can kill the whole tree. On Windows the
/// group is provided by `taskkill /T` at kill time instead.
///
/// Uses tokio's built-in `process_group(0)` (`setpgid(0, 0)` in
/// the child): the child becomes its own group leader, so its
/// pgid equals its pid. (An earlier `pre_exec` + `setsid` version
/// of this did not take effect under tokio's spawn path, and the
/// orphan test caught it.)
fn spawn_in_own_group(cmd: &mut tokio::process::Command) {
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

/// Run a [`GitCommand`] with a timeout. `cancel` is a watch channel;
/// sending `true` kills the child process. Output is redacted via
/// `reponyx_logging::redact_secrets` before it is returned or logged.
pub async fn run_git_command(
    cmd: &GitCommand,
    timeout_duration: Duration,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<ProcessOutput, RepoNyxError> {
    let mut command = tokio::process::Command::new(&cmd.program);
    command.args(&cmd.args);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    // Controlled environment: no inherited credential-bearing variables
    // are added here; credential injection (Phase 3+) sets explicit env.
    command.env_remove("GIT_ASKPASS");
    for (key, value) in &cmd.env {
        command.env(key, value);
    }
    if let Some(dir) = &cmd.current_dir {
        command.current_dir(dir);
    }
    spawn_in_own_group(&mut command);

    let mut child = command.spawn().map_err(|e| {
        RepoNyxError::new(ErrorCode::GitCloneFailed, "Failed to start git process")
            .with_details(e.to_string())
    })?;

    // Read stdout/stderr on separate tasks so large output cannot
    // deadlock the child while we wait for it to exit. `wait()` borrows
    // the child (unlike `wait_with_output()`, which consumes it), so
    // cancellation can still kill the process afterwards.
    let stdout_reader = spawn_pipe_reader(child.stdout.take());
    let stderr_reader = spawn_pipe_reader(child.stderr.take());

    let mut cancel = cancel;
    // Run the child, watching for cancellation, under an overall
    // timeout. If the cancel sender is dropped, cancellation is no
    // longer possible and we simply wait for the child.
    let run = async {
        loop {
            tokio::select! {
                changed = cancel.changed() => {
                    match changed {
                        Ok(()) if *cancel.borrow() => {
                            kill_process_tree(&mut child).await;
                            stdout_reader.abort();
                            stderr_reader.abort();
                            return Err(RepoNyxError::new(
                                ErrorCode::OperationCancelled,
                                "Git operation cancelled",
                            ));
                        }
                        Ok(()) => { /* spurious change; keep waiting */ }
                        Err(_) => {
                            break child.wait().await.map_err(|e| {
                                RepoNyxError::new(
                                    ErrorCode::GitFetchFailed,
                                    "Failed to read git output",
                                )
                                .with_details(e.to_string())
                            });
                        }
                    }
                }
                status = child.wait() => {
                    break status.map_err(|e| {
                        RepoNyxError::new(
                            ErrorCode::GitFetchFailed,
                            "Failed to read git output",
                        )
                        .with_details(e.to_string())
                    });
                }
            }
        }
    };

    let status = match timeout(timeout_duration, run).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            kill_process_tree(&mut child).await;
            stdout_reader.abort();
            stderr_reader.abort();
            return Err(RepoNyxError::new(
                ErrorCode::OperationTimedOut,
                format!(
                    "Git operation timed out after {}s",
                    timeout_duration.as_secs()
                ),
            ));
        }
    };

    let stdout = stdout_reader.await.unwrap_or_default();
    let stderr = stderr_reader.await.unwrap_or_default();

    Ok(ProcessOutput {
        success: status.success(),
        exit_code: status.code(),
        stdout: reponyx_logging::redact_secrets(&String::from_utf8_lossy(&stdout)),
        stderr: reponyx_logging::redact_secrets(&String::from_utf8_lossy(&stderr)),
    })
}

/// Run a [`GitCommand`] with a timeout, cancellation token, and
/// streaming progress reporting.
///
/// stderr is read in chunks and split on `\r` / `\n` (git's
/// `--progress` output uses carriage returns, not newlines, so a
/// line reader would never see updates until the command finished).
/// Each complete segment is parsed into a [`ProgressEvent`] and sent
/// to `progress` as it arrives; the full stdout/stderr are still
/// collected and returned redacted in the [`ProcessOutput`].
/// Cancellation and timeout kill the child process, exactly like
/// [`run_git_command`].
pub async fn run_git_command_streaming(
    cmd: &GitCommand,
    timeout_duration: Duration,
    cancel: Option<&CancellationToken>,
    progress: Option<mpsc::Sender<ProgressEvent>>,
) -> Result<ProcessOutput, RepoNyxError> {
    let mut command = tokio::process::Command::new(&cmd.program);
    command.args(&cmd.args);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    command.env_remove("GIT_ASKPASS");
    for (key, value) in &cmd.env {
        command.env(key, value);
    }
    if let Some(dir) = &cmd.current_dir {
        command.current_dir(dir);
    }
    spawn_in_own_group(&mut command);

    let mut child = command.spawn().map_err(|e| {
        RepoNyxError::new(ErrorCode::GitCloneFailed, "Failed to start git process")
            .with_details(e.to_string())
    })?;

    let stdout_reader = spawn_pipe_reader(child.stdout.take());
    let stderr_reader = spawn_progress_reader(child.stderr.take(), progress);

    let cancelled = async {
        match cancel {
            Some(token) => token.cancelled().await,
            None => std::future::pending::<()>().await,
        }
    };

    let run = async {
        tokio::select! {
            _ = cancelled => {
                kill_process_tree(&mut child).await;
                stdout_reader.abort();
                stderr_reader.abort();
                Err(RepoNyxError::new(
                    ErrorCode::OperationCancelled,
                    "Git operation cancelled",
                ))
            }
            status = child.wait() => {
                status.map_err(|e| {
                    RepoNyxError::new(
                        ErrorCode::GitFetchFailed,
                        "Failed to read git output",
                    )
                    .with_details(e.to_string())
                })
            }
        }
    };

    let status = match timeout(timeout_duration, run).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            kill_process_tree(&mut child).await;
            stdout_reader.abort();
            stderr_reader.abort();
            return Err(RepoNyxError::new(
                ErrorCode::OperationTimedOut,
                format!(
                    "Git operation timed out after {}s",
                    timeout_duration.as_secs()
                ),
            ));
        }
    };

    let stdout = stdout_reader.await.unwrap_or_default();
    let stderr = stderr_reader.await.unwrap_or_default();

    Ok(ProcessOutput {
        success: status.success(),
        exit_code: status.code(),
        stdout: reponyx_logging::redact_secrets(&String::from_utf8_lossy(&stdout)),
        stderr: reponyx_logging::redact_secrets(&String::from_utf8_lossy(&stderr)),
    })
}

/// Read stderr in chunks, emitting a [`ProgressEvent`] for every
/// complete `\r`/`\n`-separated segment, while collecting the raw
/// bytes for the final [`ProcessOutput`].
fn spawn_progress_reader(
    mut pipe: Option<impl tokio::io::AsyncRead + Send + Unpin + 'static>,
    progress: Option<mpsc::Sender<ProgressEvent>>,
) -> tokio::task::JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut collected = Vec::new();
        let mut pending: Vec<u8> = Vec::new();
        let Some(pipe) = pipe.as_mut() else {
            return collected;
        };
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    collected.extend_from_slice(&buf[..n]);
                    pending.extend_from_slice(&buf[..n]);
                    let mut start = 0;
                    for (i, byte) in pending.iter().enumerate() {
                        if *byte == b'\r' || *byte == b'\n' {
                            emit_segment(&pending[start..i], &progress).await;
                            start = i + 1;
                        }
                    }
                    pending.drain(..start);
                }
                Err(_) => break,
            }
        }
        if !pending.is_empty() {
            emit_segment(&pending, &progress).await;
        }
        collected
    })
}

async fn emit_segment(segment: &[u8], progress: &Option<mpsc::Sender<ProgressEvent>>) {
    let Some(tx) = progress else {
        return;
    };
    let line = String::from_utf8_lossy(segment);
    if let Some(event) = parse_progress_line(&line) {
        // A dropped receiver means nobody listens anymore; progress
        // is best-effort and must never fail the operation.
        let _ = tx.send(event).await;
    }
}

fn spawn_pipe_reader(
    mut pipe: Option<impl tokio::io::AsyncRead + Send + Unpin + 'static>,
) -> tokio::task::JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(pipe) = pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buf).await;
        }
        buf
    })
}

/// A cancellation receiver that never fires, for operations that do
/// not support cancellation (short local reads).
pub fn never_cancel() -> tokio::sync::watch::Receiver<bool> {
    let (_tx, rx) = tokio::sync::watch::channel(false);
    rx
}

/// Map a failed process outcome to the most actionable error code.
/// Exit code 128 with "repository not found" hints is surfaced as
/// `RepositoryNotFound`; a missing remote / auth failure is not retried
/// as if it were a network blip.
pub fn map_git_failure(cmd: &GitCommand, output: &ProcessOutput) -> RepoNyxError {
    let combined = format!("{} {}", output.stdout, output.stderr).to_lowercase();
    let code = if combined.contains("repository not found")
        || combined.contains("not found")
        || combined.contains("does not exist")
    {
        ErrorCode::RepositoryNotFound
    } else if combined.contains("authentication failed")
        || combined.contains("permission denied")
        || combined.contains("403")
        || combined.contains("401")
    {
        ErrorCode::AuthenticationRevoked
    } else if combined.contains("could not resolve host")
        || combined.contains("network is unreachable")
        || combined.contains("connection refused")
        || combined.contains("timed out")
    {
        ErrorCode::NetworkUnavailable
    } else if combined.contains("no space left on device") {
        ErrorCode::DiskSpaceInsufficient
    } else if combined.contains("index.lock") || combined.contains("unable to lock") {
        ErrorCode::RepositoryLocked
    } else if combined.contains("not a git repository") || combined.contains("corrupt") {
        ErrorCode::RepositoryCorrupted
    } else {
        ErrorCode::GitFetchFailed
    };
    RepoNyxError::new(
        code,
        format!(
            "git {} failed (exit {:?})",
            cmd.args.first().map(String::as_str).unwrap_or(""),
            output.exit_code
        ),
    )
    .with_details(reponyx_logging::redact_secrets(&output.stderr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::build_ls_remote;
    use std::path::PathBuf;

    #[test]
    fn maps_repository_not_found() {
        let cmd = build_ls_remote(&PathBuf::from("git"), "https://example.invalid/r.git");
        let out = ProcessOutput {
            success: false,
            exit_code: Some(128),
            stdout: String::new(),
            stderr: "remote: Repository not found.".into(),
        };
        assert_eq!(
            map_git_failure(&cmd, &out).code,
            ErrorCode::RepositoryNotFound
        );
    }

    #[test]
    fn maps_auth_failure_to_revoked() {
        let cmd = build_ls_remote(&PathBuf::from("git"), "https://example.invalid/r.git");
        let out = ProcessOutput {
            success: false,
            exit_code: Some(128),
            stdout: String::new(),
            stderr: "fatal: Authentication failed for 'https://github.com/o/r.git/'".into(),
        };
        assert_eq!(
            map_git_failure(&cmd, &out).code,
            ErrorCode::AuthenticationRevoked
        );
    }

    #[test]
    fn maps_disk_full() {
        let cmd = build_ls_remote(&PathBuf::from("git"), "https://example.invalid/r.git");
        let out = ProcessOutput {
            success: false,
            exit_code: Some(128),
            stdout: String::new(),
            stderr: "fatal: write error: No space left on device".into(),
        };
        assert_eq!(
            map_git_failure(&cmd, &out).code,
            ErrorCode::DiskSpaceInsufficient
        );
    }

    #[tokio::test]
    async fn run_git_version_succeeds() {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let cmd = GitCommand::new("git", vec!["--version".into()]);
        let out = run_git_command(&cmd, Duration::from_secs(10), rx)
            .await
            .expect("git --version should run");
        assert!(out.success);
        assert!(out.stdout.contains("git version"));
    }

    /// Cancelling a shell that spawned its own child must kill the
    /// child too (audit G2): `sh` starts `sleep`, the run is
    /// cancelled, and the `sleep` process must be gone afterwards —
    /// not left holding locks and writing as an orphan.
    #[tokio::test]
    async fn cancel_kills_the_whole_process_tree() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("sleep.pid");
        // The shell backgrounds `sleep`, records its pid, then waits
        // for it — so the sleep outlives nothing but the kill.
        let script = format!("sleep 30 & echo $! > {}; wait", pid_file.to_string_lossy());
        let (tx, rx) = tokio::sync::watch::channel(false);
        let cmd = GitCommand::new("sh", vec!["-c".into(), script]);
        // Cancel from the same task: after 500 ms the shell and its
        // sleep are running, so the cancel has a live tree to kill.
        let err = tokio::select! {
            r = run_git_command(&cmd, Duration::from_secs(60), rx) => r,
            _ = async {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let _ = tx.send(true);
                std::future::pending::<()>().await
            } => unreachable!("the run always finishes first after cancel"),
        }
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::OperationCancelled);
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // A killed child of a dead shell is reparented to init and
        // may linger briefly as a zombie: `kill(pid, 0)` reports a
        // zombie as existing, so read the state from /proc instead.
        // `Z` is dead (awaiting reap); any other state is alive.
        // A missing /proc entry is gone entirely.
        //
        // SIGKILL cannot be caught or ignored, so the sleep WILL
        // die — but the kernel may need a scheduling slice to act
        // on it, and a single read races that window: CI run #84
        // caught it still `R` on a loaded runner. Poll until it
        // is gone or a zombie instead of asserting on one sample;
        // the loop only fails if the group kill genuinely missed
        // the process (a real bug), never on scheduling timing.
        let proc_state = |pid: i32| {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|stat| {
                    stat.rfind(')')
                        .and_then(|close| stat[close + 2..].chars().next())
                })
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut state = proc_state(pid);
        while state.is_some() && state != Some('Z') && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
            state = proc_state(pid);
        }
        assert!(
            state.is_none() || state == Some('Z'),
            "orphaned sleep (pid {pid}) survived the cancel (state {state:?})"
        );
    }
}
