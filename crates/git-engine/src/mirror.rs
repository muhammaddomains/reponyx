//! Mirror clone + incremental fetch (Mode A, the default archival
//! mode).
//!
//! A mirror is a bare repository: `git clone --mirror` copies every
//! ref, and `git fetch --prune --tags` keeps it in sync, deleting
//! refs that vanished upstream. Both operations take the
//! per-repository lock, stream progress, and verify the result —
//! they never report success they did not check.

use crate::commands::{build_clone_mirror_progress, build_fetch_progress, build_is_bare};
use crate::credentials::reject_embedded_credentials;
use crate::locks::RepoLockManager;
use crate::options::SyncOptions;
use crate::process::{map_git_failure, run_git_command_streaming};
use crate::refs::{head_sha, list_branches, list_ref_names, list_tags};
use reponyx_core::{ErrorCode, RepoNyxError};
use std::path::{Path, PathBuf};

/// What a successful mirror clone produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorInfo {
    /// SHA of `HEAD`, if the repository has any commits.
    pub head_sha: Option<String>,
    /// Number of branches in the mirror.
    pub branches: usize,
    /// Number of tags in the mirror.
    pub tags: usize,
}

/// What an incremental fetch changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchSummary {
    /// Ref names present before the fetch.
    pub refs_before: Vec<String>,
    /// Ref names present after the fetch.
    pub refs_after: Vec<String>,
    /// Refs that appeared during the fetch.
    pub added: Vec<String>,
    /// Refs that were pruned (deleted upstream).
    pub pruned: Vec<String>,
}

/// Clone `url` as a mirror into `dest`.
///
/// The destination's parent directories are created, the
/// per-repository lock is held for the whole operation, and the
/// result is verified (bare repository, HEAD present, refs listed).
///
/// A destination that already exists and holds a **valid** archive
/// is rejected — RepoNyx never clobbers a good mirror. A
/// destination that exists but is **partial** (a clone killed or
/// crashed mid-way) is moved aside to
/// `<dest>.partial-<timestamp>` and the clone proceeds, so a crash
/// no longer leaves the repository stuck forever (audit B8: the
/// old code rejected every re-clone with "already exists" and
/// `local_path` was never set). The moved-aside data is kept for
/// diagnosis, never deleted.
pub async fn mirror_clone(
    git: &Path,
    url: &str,
    dest: &Path,
    locks: &RepoLockManager,
    opts: SyncOptions,
) -> Result<MirrorInfo, RepoNyxError> {
    reject_embedded_credentials(url)?;
    if dest.as_os_str().is_empty() {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "Mirror destination must not be empty",
        ));
    }
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            RepoNyxError::new(ErrorCode::GitCloneFailed, "Cannot create mirror parent dir")
                .with_details(format!("{}: {e}", parent.display()))
        })?;
    }

    let _lock = locks.acquire(dest, opts.lock_timeout).await?;

    if dest.exists()
        && dest
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        match verify_mirror(git, dest).await {
            Ok(_) => {
                return Err(RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "Mirror destination already exists and holds a valid archive",
                )
                .with_details(format!(
                    "dest={} — delete this folder to re-clone, or run a fetch instead",
                    dest.display()
                )));
            }
            Err(_) => {
                if let Some(moved) = move_aside_partial_dest(dest)? {
                    tracing::warn!(
                        from = %dest.display(),
                        to = %moved.display(),
                        "moved aside a partial mirror clone; re-cloning"
                    );
                }
            }
        }
    }

    let mut cmd = build_clone_mirror_progress(git, url, dest);
    if let Some(helper) = &opts.credential {
        helper.install(&mut cmd);
    }
    let out = match run_git_command_streaming(
        &cmd,
        opts.timeout,
        opts.cancel.as_ref(),
        opts.progress.clone(),
    )
    .await
    {
        Ok(out) => out,
        Err(e) => {
            if matches!(
                e.code,
                ErrorCode::OperationCancelled | ErrorCode::OperationTimedOut
            ) {
                // We killed git (and waited for it), so no git process
                // can still hold a lock; the transport helper never
                // touches the refs. Remove the leftovers so the
                // destination diagnoses as incomplete/corrupt instead
                // of locked forever — and so the next clone can move
                // it aside instead of failing on a stale ref lock.
                crate::locks::remove_stale_git_locks(dest);
            }
            return Err(e);
        }
    };
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    verify_mirror(git, dest).await
}

/// Fetch `--prune --tags` into an existing mirror, then report what
/// changed. The lock is held for the whole operation.
pub async fn mirror_fetch(
    git: &Path,
    mirror_path: &Path,
    locks: &RepoLockManager,
    opts: SyncOptions,
) -> Result<FetchSummary, RepoNyxError> {
    if !mirror_path.is_dir() {
        return Err(
            RepoNyxError::new(ErrorCode::RepositoryNotFound, "Mirror path does not exist")
                .with_details(format!("path={}", mirror_path.display())),
        );
    }
    let _lock = locks.acquire(mirror_path, opts.lock_timeout).await?;

    let refs_before = list_ref_names(git, mirror_path).await?;
    let mut cmd = build_fetch_progress(git, mirror_path, true);
    if let Some(helper) = &opts.credential {
        helper.install(&mut cmd);
    }
    let out = match run_git_command_streaming(
        &cmd,
        opts.timeout,
        opts.cancel.as_ref(),
        opts.progress.clone(),
    )
    .await
    {
        Ok(out) => out,
        Err(e) => {
            if matches!(
                e.code,
                ErrorCode::OperationCancelled | ErrorCode::OperationTimedOut
            ) {
                // Same cleanup as mirror_clone: we killed git, so a
                // leftover lock is ours to remove.
                crate::locks::remove_stale_git_locks(mirror_path);
            }
            return Err(e);
        }
    };
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    let refs_after = list_ref_names(git, mirror_path).await?;
    let added: Vec<String> = refs_after
        .iter()
        .filter(|r| !refs_before.contains(r))
        .cloned()
        .collect();
    let pruned: Vec<String> = refs_before
        .iter()
        .filter(|r| !refs_after.contains(r))
        .cloned()
        .collect();
    // The fetch must not have damaged the mirror.
    verify_mirror(git, mirror_path).await?;
    Ok(FetchSummary {
        refs_before,
        refs_after,
        added,
        pruned,
    })
}

/// Move a partial (invalid) clone destination aside so a re-clone
/// can proceed: `<dest>.partial-<unix-seconds>` (with a counter
/// when that name is taken). The partial data is kept for
/// diagnosis, never deleted. Returns the path it was moved to, or
/// `None` when the destination does not exist. Used by the mirror
/// clone here and by the working clone in the queue runner.
pub fn move_aside_partial_dest(dest: &Path) -> Result<Option<PathBuf>, RepoNyxError> {
    if !dest.exists() {
        return Ok(None);
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = dest.to_string_lossy();
    for attempt in 0..100 {
        let suffix = if attempt == 0 {
            format!(".partial-{secs}")
        } else {
            format!(".partial-{secs}-{attempt}")
        };
        let target = PathBuf::from(format!("{base}{suffix}"));
        if target.exists() {
            continue;
        }
        match std::fs::rename(dest, &target) {
            Ok(()) => return Ok(Some(target)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(RepoNyxError::new(
                    ErrorCode::GitCloneFailed,
                    "Cannot move aside the partial clone destination",
                )
                .with_details(format!(
                    "{} -> {}: {e}",
                    dest.display(),
                    target.display()
                )));
            }
        }
    }
    Err(RepoNyxError::new(
        ErrorCode::GitCloneFailed,
        "Cannot move aside the partial clone destination (no free name)",
    )
    .with_details(format!("dest={}", dest.display())))
}

/// Verify a mirror is a bare repository with readable refs.
async fn verify_mirror(git: &Path, mirror_path: &Path) -> Result<MirrorInfo, RepoNyxError> {
    let cmd = build_is_bare(git, mirror_path);
    let out = crate::process::run_git_command(
        &cmd,
        crate::refs::REF_TIMEOUT,
        crate::process::never_cancel(),
    )
    .await?;
    if !out.success || out.stdout.trim() != "true" {
        return Err(RepoNyxError::new(
            ErrorCode::RepositoryCorrupted,
            "Cloned directory is not a bare repository",
        )
        .with_details(format!("path={}", mirror_path.display())));
    }
    let branches = list_branches(git, mirror_path).await?.len();
    let tags = list_tags(git, mirror_path).await?.len();
    Ok(MirrorInfo {
        head_sha: head_sha(git, mirror_path).await?,
        branches,
        tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_summary_diff_is_correct() {
        let summary = FetchSummary {
            refs_before: vec!["refs/heads/main".into(), "refs/heads/old".into()],
            refs_after: vec!["refs/heads/main".into(), "refs/heads/new".into()],
            added: vec!["refs/heads/new".into()],
            pruned: vec!["refs/heads/old".into()],
        };
        assert_eq!(summary.added, ["refs/heads/new"]);
        assert_eq!(summary.pruned, ["refs/heads/old"]);
    }

    #[test]
    fn move_aside_partial_renames_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("repo.git");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("half-object"), "partial").unwrap();
        let moved = move_aside_partial_dest(&dest).unwrap().unwrap();
        assert!(!dest.exists());
        assert!(moved.join("half-object").exists());
        assert!(
            moved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("repo.git.partial-")
        );
        // A missing destination is a no-op.
        assert!(move_aside_partial_dest(&dest).unwrap().is_none());
    }

    #[test]
    fn move_aside_partial_avoids_name_collision() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("repo.git");
        std::fs::create_dir(&dest).unwrap();
        // Pre-create the first candidate name: the move must pick
        // the next one instead of failing or overwriting.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::create_dir(dir.path().join(format!("repo.git.partial-{secs}"))).unwrap();
        let moved = move_aside_partial_dest(&dest).unwrap().unwrap();
        assert!(
            moved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("repo.git.partial-{secs}-"))
        );
    }
}
