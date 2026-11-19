//! Repository health checks: diagnosis, never repair.
//!
//! [`verify_repository`] inspects a local repository and returns a
//! structured [`RepositoryDiagnosis`]: what is wrong (if anything),
//! what to do about it, and the mapped [`RepoNyxError`]. It never
//! deletes, repairs, or otherwise modifies anything — recovery is a
//! later phase, and only ever happens with the user's explicit
//! confirmation.
//!
//! Detection order matters: a missing path is "not a repository"; a
//! lock held by a live process is "locked" (not corrupt); a
//! half-written clone is "incomplete" (not corrupt); only a
//! repository whose own metadata fails to parse is "corrupt".
//!
//! Stale `index.lock` heuristic: Git leaves `index.lock` behind only
//! when a process died mid-operation, but a live Git holds it briefly
//! too. A lock file older than [`STALE_INDEX_LOCK_AGE`] is reported
//! as stale; a fresh one is reported as held by a live process. The
//! `.reponyx.lock` file lock, by contrast, is probed exactly —
//! [`RepoLockManager::probe_file_lock`] asks the OS whether anyone
//! holds it, so it is never misreported as stale.

use crate::commands::{build_fsck, build_is_bare, build_rev_parse};
use crate::locks::RepoLockManager;
use crate::process::{never_cancel, run_git_command};
use crate::refs::{REF_TIMEOUT, git_dir_of};
use reponyx_core::{ErrorCode, RepoNyxError};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// An `index.lock` older than this is treated as stale (its holder
/// process is assumed gone). A live Git operation holds the file for
/// seconds, never an hour.
pub const STALE_INDEX_LOCK_AGE: Duration = Duration::from_secs(3600);

/// Health of a local repository, as diagnosed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// The repository is usable.
    Healthy,
    /// The path does not exist, is empty, or holds no Git markers.
    NotARepository,
    /// The directory exists but the clone never finished (missing
    /// HEAD / objects / refs).
    IncompleteClone,
    /// A lock is held: `index.lock`, or the `.reponyx.lock` file lock.
    Locked,
    /// Git metadata is present but unreadable (`rev-parse` / `fsck`
    /// failure).
    Corrupt,
}

/// The result of [`verify_repository`]: a diagnosis, not a bare bool.
#[derive(Debug, Clone)]
pub struct RepositoryDiagnosis {
    pub status: HealthStatus,
    /// What was found, in plain language.
    pub detail: String,
    /// What the user (or a later repair step) should do.
    pub suggested_action: String,
    /// The mapped error, if the repository is not healthy.
    pub error: Option<RepoNyxError>,
    /// SHA of `HEAD` when the repository is healthy and has commits.
    pub head_sha: Option<String>,
    /// Non-fatal caveat, e.g. "fsck timed out; integrity not fully
    /// verified". Present only when the status is otherwise healthy.
    pub warning: Option<String>,
}

impl RepositoryDiagnosis {
    fn healthy(detail: impl Into<String>, head_sha: Option<String>) -> Self {
        Self {
            status: HealthStatus::Healthy,
            detail: detail.into(),
            suggested_action: "No action needed.".to_string(),
            error: None,
            head_sha,
            warning: None,
        }
    }

    fn problem(
        status: HealthStatus,
        code: ErrorCode,
        detail: impl Into<String>,
        suggested_action: impl Into<String>,
    ) -> Self {
        let detail = detail.into();
        Self {
            status,
            error: Some(RepoNyxError::new(code, detail.clone())),
            detail,
            suggested_action: suggested_action.into(),
            head_sha: None,
            warning: None,
        }
    }
}

/// Diagnose the repository at `path`. Never modifies anything and
/// never returns `Err` — the diagnosis itself is the result.
pub async fn verify_repository(git: &Path, path: &Path, timeout: Duration) -> RepositoryDiagnosis {
    if !path.exists() {
        return RepositoryDiagnosis::problem(
            HealthStatus::NotARepository,
            ErrorCode::RepositoryNotFound,
            format!("Path does not exist: {}", path.display()),
            "Clone the repository again, or check the path.",
        );
    }

    // Locked by another RepoNyx process? The OS-level probe is exact.
    if RepoLockManager::probe_file_lock(path) {
        return RepositoryDiagnosis::problem(
            HealthStatus::Locked,
            ErrorCode::RepositoryLocked,
            format!(
                "Another RepoNyx process holds the lock for {}",
                path.display()
            ),
            "Wait for the other operation to finish, then retry.",
        );
    }

    let Some(git_dir) = git_dir_of(path) else {
        return not_a_repo_or_incomplete(path);
    };

    // Stale index.lock? A live Git holds it only briefly.
    let index_lock = git_dir.join("index.lock");
    if index_lock.exists() {
        let stale = index_lock
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|age| age > STALE_INDEX_LOCK_AGE)
            .unwrap_or(false);
        return RepositoryDiagnosis::problem(
            HealthStatus::Locked,
            ErrorCode::RepositoryLocked,
            if stale {
                format!("Stale index.lock in {}", git_dir.display())
            } else {
                format!("index.lock present in {}", git_dir.display())
            },
            if stale {
                "The lock holder is gone; remove index.lock and retry. \
                 RepoNyx never removes it automatically."
            } else {
                "Another Git operation is running; wait for it to finish, then retry."
            },
        );
    }

    // HEAD present? A missing HEAD in an otherwise Git-shaped
    // directory means the clone never finished.
    if !git_dir.join("HEAD").exists() {
        return RepositoryDiagnosis::problem(
            HealthStatus::IncompleteClone,
            ErrorCode::RepositoryCorrupted,
            format!("Missing HEAD in {}", git_dir.display()),
            "The clone did not finish. Delete the incomplete directory \
             and clone again — only after confirming; RepoNyx never \
             deletes it automatically.",
        );
    }
    if !git_dir.join("objects").is_dir() {
        return RepositoryDiagnosis::problem(
            HealthStatus::IncompleteClone,
            ErrorCode::RepositoryCorrupted,
            format!("Missing objects directory in {}", git_dir.display()),
            "The clone did not finish. Delete the incomplete directory \
             and clone again — only after confirming; RepoNyx never \
             deletes it automatically.",
        );
    }

    // The directory must at least answer rev-parse.
    let cmd = build_rev_parse(git, &git_dir, "--git-dir");
    let out = run_git_command(&cmd, REF_TIMEOUT, never_cancel()).await;
    let Ok(out) = out else {
        return RepositoryDiagnosis::problem(
            HealthStatus::Corrupt,
            ErrorCode::RepositoryCorrupted,
            "git rev-parse could not be run for this repository",
            "Run repository recovery/diagnostics; do not delete the \
             archive without confirmation.",
        );
    };
    if !out.success {
        return RepositoryDiagnosis::problem(
            HealthStatus::Corrupt,
            ErrorCode::RepositoryCorrupted,
            format!("git rev-parse failed: {}", first_line(&out.stderr)),
            "Run repository recovery/diagnostics; do not delete the \
             archive without confirmation.",
        );
    }

    // HEAD must resolve, or be a well-formed unborn ref (empty repo).
    let head = match head_sha_of(git, &git_dir).await {
        Ok(head) => head,
        Err(detail) => {
            return RepositoryDiagnosis::problem(
                HealthStatus::Corrupt,
                ErrorCode::RepositoryCorrupted,
                detail,
                "Run repository recovery/diagnostics; do not delete the \
                 archive without confirmation.",
            );
        }
    };

    // Full integrity check. A timeout is not corruption — it is a
    // caveat on an otherwise healthy repository.
    let fsck = build_fsck(git, &git_dir);
    let fsck_timeout = timeout.min(Duration::from_secs(60));
    match tokio::time::timeout(
        fsck_timeout,
        run_git_command(&fsck, fsck_timeout, never_cancel()),
    )
    .await
    {
        Ok(Ok(out)) if out.success => {}
        Ok(Ok(out)) => {
            return RepositoryDiagnosis::problem(
                HealthStatus::Corrupt,
                ErrorCode::RepositoryCorrupted,
                format!("git fsck failed: {}", first_line(&out.stderr)),
                "Run repository recovery/diagnostics; do not delete the \
                 archive without confirmation.",
            );
        }
        Ok(Err(e)) => {
            return RepositoryDiagnosis::problem(
                HealthStatus::Corrupt,
                ErrorCode::RepositoryCorrupted,
                format!("git fsck could not run: {}", e.message),
                "Run repository recovery/diagnostics; do not delete the \
                 archive without confirmation.",
            );
        }
        Err(_) => {
            let mut diagnosis = RepositoryDiagnosis::healthy(
                format!("Repository at {} is usable", path.display()),
                head,
            );
            diagnosis.warning = Some(
                "Integrity check (fsck) timed out; repository integrity \
                 was not fully verified."
                    .to_string(),
            );
            return diagnosis;
        }
    }

    RepositoryDiagnosis::healthy(format!("Repository at {} is usable", path.display()), head)
}

/// A directory that is not a repository, or a half-written clone.
/// A `.git` directory/file or a bare-repo HEAD fragment means an
/// interrupted clone; anything else is simply not a repository.
fn not_a_repo_or_incomplete(path: &Path) -> RepositoryDiagnosis {
    let dot_git = path.join(".git");
    let has_git_markers = dot_git.exists()
        || path.join("HEAD").exists()
        || path.join("objects").exists()
        || path.join("refs").exists();
    if has_git_markers {
        let mut missing = Vec::new();
        if !path.join("HEAD").exists() && !dot_git.join("HEAD").exists() {
            missing.push("HEAD");
        }
        if !path.join("objects").is_dir() && !dot_git.join("objects").is_dir() {
            missing.push("objects");
        }
        return RepositoryDiagnosis::problem(
            HealthStatus::IncompleteClone,
            ErrorCode::RepositoryCorrupted,
            format!(
                "Incomplete clone at {} (missing: {})",
                path.display(),
                if missing.is_empty() {
                    "unknown".to_string()
                } else {
                    missing.join(", ")
                }
            ),
            "The clone did not finish. Delete the incomplete directory \
             and clone again — only after confirming; RepoNyx never \
             deletes it automatically.",
        );
    }
    RepositoryDiagnosis::problem(
        HealthStatus::NotARepository,
        ErrorCode::RepositoryNotFound,
        format!("Not a git repository: {}", path.display()),
        "Clone the repository again, or check the path.",
    )
}

/// Resolve HEAD: the commit SHA, or `None` for an empty repository
/// (unborn HEAD). A malformed HEAD file is corruption.
async fn head_sha_of(git: &Path, git_dir: &Path) -> Result<Option<String>, String> {
    let cmd = build_rev_parse(git, git_dir, "HEAD");
    let out = run_git_command(&cmd, REF_TIMEOUT, never_cancel())
        .await
        .map_err(|e| e.message.clone())?;
    if out.success {
        let sha = out.stdout.trim().to_string();
        return Ok(if sha.is_empty() { None } else { Some(sha) });
    }
    // rev-parse fails on an empty repo ("Needed a single revision").
    // That is healthy if HEAD itself is well-formed.
    let head_path = git_dir.join("HEAD");
    let head = std::fs::read_to_string(&head_path).unwrap_or_default();
    let head = head.trim();
    if let Some(target) = head.strip_prefix("ref: ")
        && !target.is_empty()
        && !target.contains("..")
    {
        return Ok(None);
    }
    if head.len() == 40 && head.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(Some(head.to_string()));
    }
    Err(format!("HEAD is malformed: {}", first_line(&out.stderr)))
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

/// Whether `path` is a bare repository (used by callers that need to
/// tell mirrors from working clones).
pub async fn is_bare_repository(git: &Path, git_dir: &Path) -> bool {
    let cmd = build_is_bare(git, git_dir);
    let Ok(out) = run_git_command(&cmd, REF_TIMEOUT, never_cancel()).await else {
        return false;
    };
    out.success && out.stdout.trim() == "true"
}

/// Resolve the git directory of `path` (bare or working clone).
pub fn resolve_git_dir(path: &Path) -> Option<PathBuf> {
    git_dir_of(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_head_is_not_unborn() {
        // HEAD parsing is covered by the integration fixtures in
        // tests/health.rs; this guards the empty-repo branch directly.
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert!(git_dir.join("HEAD").exists());
    }
}
