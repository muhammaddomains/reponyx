//! Safe process execution: arg arrays, timeout, cancellation,
//! redacted output. No shell is ever involved.

use crate::commands::GitCommand;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::time::Duration;
use tokio::time::timeout;

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
    if let Some(dir) = &cmd.current_dir {
        command.current_dir(dir);
    }

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
                            let _ = child.kill().await;
                            let _ = child.wait().await;
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
            let _ = child.kill().await;
            let _ = child.wait().await;
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
}
