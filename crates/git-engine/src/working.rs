//! Working-clone operations (Mode B).
//!
//! A working clone is a checked-out repository with local changes.
//! The rules here are absolute:
//!
//! - Fetch only: `git fetch --all --prune --tags`. Never checkout,
//!   reset, or clean — local work is never touched.
//! - Status and divergence are read-only observations.
//!
//! `working_status` classifies the worktree with this precedence:
//! `Conflicted` > `Staged` > `Modified` > `UntrackedOnly` > `Clean`.
//! A worktree with both staged and unstaged changes reports `Staged`
//! (the staged entries are listed too, so nothing is hidden).

use crate::commands::{
    build_fetch_all_progress, build_rev_list_count, build_rev_parse_upstream,
    build_status_porcelain,
};
use crate::locks::RepoLockManager;
use crate::options::SyncOptions;
use crate::process::{map_git_failure, never_cancel, run_git_command, run_git_command_streaming};
use crate::refs::REF_TIMEOUT;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::path::Path;

/// Dirty state of a working clone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingState {
    /// No staged, modified, untracked, or conflicted files.
    Clean,
    /// Only untracked files (no staged or modified tracked files).
    UntrackedOnly,
    /// Tracked files modified in the worktree, nothing staged.
    Modified,
    /// At least one file staged in the index.
    Staged,
    /// At least one unmerged/conflicted file.
    Conflicted,
}

/// One line of `git status --porcelain`: the two-letter XY code and
/// the path, exactly as Git reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// Two-letter status code, e.g. `M `, ` M`, `??`, `UU`.
    pub code: String,
    pub path: String,
}

/// The dirty state of a working clone plus every status entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingStatus {
    pub state: WorkingState,
    pub entries: Vec<StatusEntry>,
}

/// Ahead/behind of the current branch (or `branch`) against its
/// upstream. `upstream` is `None` when the branch has no upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
}

/// Fetch `--all --prune --tags` into a working clone. Only the
/// remote-tracking refs move; the checkout and the index are never
/// touched.
pub async fn working_fetch(
    git: &Path,
    work_path: &Path,
    locks: &RepoLockManager,
    opts: SyncOptions,
) -> Result<(), RepoNyxError> {
    let git_dir = require_worktree(work_path)?;
    let _lock = locks.acquire(work_path, opts.lock_timeout).await?;
    let mut cmd = build_fetch_all_progress(git, &git_dir);
    if let Some(helper) = &opts.credential {
        helper.install(&mut cmd);
    }
    let out = run_git_command_streaming(
        &cmd,
        opts.timeout,
        opts.cancel.as_ref(),
        opts.progress.clone(),
    )
    .await?;
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    Ok(())
}

/// Dirty state of a working clone. Read-only; acquires no lock.
pub async fn working_status(git: &Path, work_path: &Path) -> Result<WorkingStatus, RepoNyxError> {
    require_worktree(work_path)?;
    let cmd = build_status_porcelain(git, work_path);
    let out = run_git_command(&cmd, REF_TIMEOUT, never_cancel()).await?;
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    Ok(classify_status(&out.stdout))
}

/// Ahead/behind of `branch` (default: `HEAD`) against its upstream.
/// Read-only; acquires no lock. A branch without an upstream reports
/// `upstream: None` with zero counts — that is not an error.
pub async fn divergence(
    git: &Path,
    work_path: &Path,
    branch: Option<&str>,
) -> Result<Divergence, RepoNyxError> {
    require_worktree(work_path)?;
    let reference = branch
        .map(|b| format!("refs/heads/{b}"))
        .unwrap_or_else(|| "HEAD".to_string());
    let upstream_cmd = build_rev_parse_upstream(git, work_path, &reference);
    let upstream_out = run_git_command(&upstream_cmd, REF_TIMEOUT, never_cancel()).await?;
    if !upstream_out.success {
        return Ok(Divergence {
            upstream: None,
            ahead: 0,
            behind: 0,
        });
    }
    let upstream = upstream_out.stdout.trim().to_string();
    if upstream.is_empty() {
        return Ok(Divergence {
            upstream: None,
            ahead: 0,
            behind: 0,
        });
    }
    let range = format!("{reference}...{upstream}");
    let count_cmd = build_rev_list_count(git, work_path, &range);
    let count_out = run_git_command(&count_cmd, REF_TIMEOUT, never_cancel()).await?;
    if !count_out.success {
        return Err(map_git_failure(&count_cmd, &count_out));
    }
    let mut parts = count_out.stdout.split_whitespace();
    let ahead: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let behind: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok(Divergence {
        upstream: Some(upstream),
        ahead,
        behind,
    })
}

/// Require `path` to be a working clone (a `.git` directory or file).
/// Returns the git directory for commands that need `--git-dir`.
fn require_worktree(path: &Path) -> Result<std::path::PathBuf, RepoNyxError> {
    crate::refs::git_dir_of(path)
        .filter(|gd| gd != path)
        .ok_or_else(|| {
            RepoNyxError::new(ErrorCode::RepositoryNotFound, "Path is not a working clone")
                .with_details(format!("path={}", path.display()))
        })
}

/// Classify `git status --porcelain` output. Precedence:
/// conflicted > staged > modified > untracked-only > clean.
fn classify_status(porcelain: &str) -> WorkingStatus {
    let mut entries = Vec::new();
    let mut conflicted = false;
    let mut staged = false;
    let mut modified = false;
    let mut only_untracked = true;
    for line in porcelain.lines() {
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].to_string();
        let (x, y) = (code.as_bytes()[0], code.as_bytes()[1]);
        entries.push(StatusEntry {
            code: code.to_string(),
            path,
        });
        if code == "??" {
            continue;
        }
        only_untracked = false;
        // Unmerged states: both sides unmerged, or one side deleted.
        if x == b'U' || y == b'U' || code == "AA" || code == "DD" {
            conflicted = true;
        }
        // Staged: index differs from HEAD (M/A/D/R/C in the X slot).
        if matches!(x, b'M' | b'A' | b'D' | b'R' | b'C') {
            staged = true;
        }
        // Modified: worktree differs from the index (M/D in the Y slot).
        if matches!(y, b'M' | b'D') {
            modified = true;
        }
    }
    let state = if entries.is_empty() {
        WorkingState::Clean
    } else if conflicted {
        WorkingState::Conflicted
    } else if staged {
        WorkingState::Staged
    } else if modified {
        WorkingState::Modified
    } else if only_untracked {
        WorkingState::UntrackedOnly
    } else {
        // Defensive: entries exist but matched no class (e.g. `!!`
        // ignored files, which porcelain does not emit by default).
        WorkingState::Modified
    };
    WorkingStatus { state, entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_of(porcelain: &str) -> WorkingState {
        classify_status(porcelain).state
    }

    #[test]
    fn clean_and_untracked() {
        assert_eq!(status_of(""), WorkingState::Clean);
        assert_eq!(status_of("?? notes.txt\n"), WorkingState::UntrackedOnly);
        assert_eq!(
            status_of("?? a.txt\n?? dir/\n"),
            WorkingState::UntrackedOnly
        );
    }

    #[test]
    fn modified_and_staged() {
        assert_eq!(status_of(" M src/lib.rs\n"), WorkingState::Modified);
        assert_eq!(status_of("M  src/lib.rs\n"), WorkingState::Staged);
        assert_eq!(status_of("A  new.rs\n"), WorkingState::Staged);
        // Both staged and modified: staged wins, entries are kept.
        let s = classify_status("MM src/lib.rs\n");
        assert_eq!(s.state, WorkingState::Staged);
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].code, "MM");
    }

    #[test]
    fn conflicted_beats_everything() {
        assert_eq!(status_of("UU src/lib.rs\n"), WorkingState::Conflicted);
        assert_eq!(
            status_of("M  staged.rs\nUU conflict.rs\n"),
            WorkingState::Conflicted
        );
        assert_eq!(status_of("AA both-added.rs\n"), WorkingState::Conflicted);
        assert_eq!(status_of("DU deleted-by-us.rs\n"), WorkingState::Conflicted);
    }

    #[test]
    fn deleted_and_renamed() {
        assert_eq!(status_of(" D gone.rs\n"), WorkingState::Modified);
        assert_eq!(status_of("D  gone.rs\n"), WorkingState::Staged);
        assert_eq!(status_of("R  old.rs -> new.rs\n"), WorkingState::Staged);
    }

    #[test]
    fn require_worktree_rejects_bare_and_plain_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("bare.git");
        std::fs::create_dir_all(bare.join("objects")).unwrap();
        std::fs::create_dir_all(bare.join("refs")).unwrap();
        std::fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert!(require_worktree(&bare).is_err());
        assert!(require_worktree(dir.path()).is_err());
    }
}
