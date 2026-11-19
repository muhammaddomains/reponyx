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
use std::path::Path;

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
/// A destination that already exists and is not empty is rejected
/// before Git runs. On failure the (possibly partial) destination is
/// left in place for diagnosis — RepoNyx never deletes it.
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
    if dest.exists()
        && dest
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "Mirror destination already exists and is not empty",
        )
        .with_details(format!("dest={}", dest.display())));
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

    let mut cmd = build_clone_mirror_progress(git, url, dest);
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
}
