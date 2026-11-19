//! Branch and tag discovery from a mirror (or any git directory).
//!
//! Discovery reads refs with `git for-each-ref` — it never contacts a
//! remote. In a mirror clone, `refs/heads/*` are the remote's branches
//! (a mirror maps `refs/*` to `refs/*`); in a working clone,
//! `refs/remotes/<remote>/*` are the remote-tracking branches.

use crate::commands::build_for_each_ref;
use crate::process::{ProcessOutput, map_git_failure, never_cancel, run_git_command};
use reponyx_core::RepoNyxError;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default timeout for local ref reads.
pub const REF_TIMEOUT: Duration = Duration::from_secs(30);

/// What kind of branch a ref is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// `refs/heads/*` — in a mirror, the remote's branches.
    LocalBranch,
    /// `refs/remotes/<remote>/*` — a remote-tracking branch.
    RemoteTracking,
}

/// One branch ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    /// Short name, e.g. `main` or `origin/main`.
    pub name: String,
    /// Full ref, e.g. `refs/heads/main`.
    pub full_ref: String,
    /// Commit SHA the branch points at.
    pub sha: String,
    pub kind: RefKind,
    /// Remote name for tracking branches (`origin`), else `None`.
    pub remote: Option<String>,
}

/// One tag ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagInfo {
    /// Short name, e.g. `v1.0.0`.
    pub name: String,
    /// Commit SHA the tag points at (peeled for annotated tags).
    pub sha: String,
    /// SHA of the tag object itself (same as `sha` for lightweight tags).
    pub tag_sha: String,
    /// `true` for annotated tags.
    pub annotated: bool,
}

/// List branches in `git_dir` (a mirror or a working clone's `.git`).
pub async fn list_branches(git: &Path, git_dir: &Path) -> Result<Vec<BranchInfo>, RepoNyxError> {
    let cmd = build_for_each_ref(
        git,
        git_dir,
        "%(refname)%00%(objectname)",
        &["refs/heads/", "refs/remotes/"],
    );
    let out = run_ref_command(&cmd).await?;
    let mut branches = Vec::new();
    for line in out.stdout.lines() {
        let mut fields = line.split('\0');
        let Some(full_ref) = fields.next() else {
            continue;
        };
        let Some(sha) = fields.next() else {
            continue;
        };
        if full_ref.is_empty() || sha.is_empty() {
            continue;
        }
        let (name, kind, remote) = if let Some(rest) = full_ref.strip_prefix("refs/heads/") {
            (rest.to_string(), RefKind::LocalBranch, None)
        } else if let Some(rest) = full_ref.strip_prefix("refs/remotes/") {
            let (remote, name) = match rest.split_once('/') {
                Some((remote, name)) => (Some(remote.to_string()), format!("{remote}/{name}")),
                None => (None, rest.to_string()),
            };
            (name, RefKind::RemoteTracking, remote)
        } else {
            continue;
        };
        branches.push(BranchInfo {
            name,
            full_ref: full_ref.to_string(),
            sha: sha.to_string(),
            kind,
            remote,
        });
    }
    Ok(branches)
}

/// List tags in `git_dir`.
pub async fn list_tags(git: &Path, git_dir: &Path) -> Result<Vec<TagInfo>, RepoNyxError> {
    let cmd = build_for_each_ref(
        git,
        git_dir,
        "%(refname)%00%(objectname)%00%(*objectname)",
        &["refs/tags/"],
    );
    let out = run_ref_command(&cmd).await?;
    let mut tags = Vec::new();
    for line in out.stdout.lines() {
        let mut fields = line.split('\0');
        let Some(full_ref) = fields.next() else {
            continue;
        };
        let Some(objectname) = fields.next() else {
            continue;
        };
        let peeled = fields.next().unwrap_or("");
        if full_ref.is_empty() || objectname.is_empty() {
            continue;
        }
        let Some(name) = full_ref.strip_prefix("refs/tags/") else {
            continue;
        };
        let annotated = !peeled.is_empty();
        tags.push(TagInfo {
            name: name.to_string(),
            sha: if annotated {
                peeled.to_string()
            } else {
                objectname.to_string()
            },
            tag_sha: objectname.to_string(),
            annotated,
        });
    }
    Ok(tags)
}

/// SHA of `HEAD` in `git_dir`, or `None` for a repository with no
/// commits yet (unborn HEAD).
pub async fn head_sha(git: &Path, git_dir: &Path) -> Result<Option<String>, RepoNyxError> {
    let cmd = crate::commands::build_rev_parse(git, git_dir, "HEAD");
    let out = run_git_command(&cmd, REF_TIMEOUT, never_cancel()).await?;
    if !out.success {
        return Ok(None);
    }
    let sha = out.stdout.trim().to_string();
    if sha.is_empty() {
        Ok(None)
    } else {
        Ok(Some(sha))
    }
}

/// Every ref name in `git_dir` (`refs/heads/*`, `refs/tags/*`, ...).
pub async fn list_ref_names(git: &Path, git_dir: &Path) -> Result<Vec<String>, RepoNyxError> {
    let cmd = build_for_each_ref(git, git_dir, "%(refname)", &[]);
    let out = run_ref_command(&cmd).await?;
    Ok(out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

async fn run_ref_command(cmd: &crate::commands::GitCommand) -> Result<ProcessOutput, RepoNyxError> {
    let out = run_git_command(cmd, REF_TIMEOUT, never_cancel()).await?;
    if !out.success {
        return Err(map_git_failure(cmd, &out));
    }
    Ok(out)
}

/// Short timeout for the `rev-parse --git-dir` probe used by
/// callers that only need to know whether a directory is a repo.
pub fn git_dir_of(path: &Path) -> Option<PathBuf> {
    // Mirrors (bare): HEAD + objects/ + refs/ at the path itself.
    if path.join("HEAD").is_file() && path.join("objects").is_dir() && path.join("refs").is_dir() {
        return Some(path.to_path_buf());
    }
    // Working clone: .git is a directory, or a file saying `gitdir: <x>`.
    let dot_git = path.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    if dot_git.is_file()
        && let Ok(content) = std::fs::read_to_string(&dot_git)
        && let Some(target) = content.trim().strip_prefix("gitdir: ")
    {
        let target = target.trim();
        let resolved = if Path::new(target).is_absolute() {
            PathBuf::from(target)
        } else {
            path.join(target)
        };
        return Some(resolved);
    }
    None
}

/// Whether `path` looks like a git directory (bare or working clone)
/// by filesystem markers alone. `git rev-parse` is the authority;
/// this is only a cheap pre-check.
pub fn looks_like_repo(path: &Path) -> bool {
    git_dir_of(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reponyx_core::ErrorCode;

    #[test]
    fn git_dir_of_detects_bare_and_working() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("bare.git");
        std::fs::create_dir_all(bare.join("objects")).unwrap();
        std::fs::create_dir_all(bare.join("refs")).unwrap();
        std::fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(git_dir_of(&bare), Some(bare.clone()));

        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join(".git")).unwrap();
        assert_eq!(git_dir_of(&work), Some(work.join(".git")));

        let plain = dir.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        assert_eq!(git_dir_of(&plain), None);
    }

    #[test]
    fn git_dir_of_follows_gitdir_files() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.git");
        std::fs::create_dir_all(real.join("objects")).unwrap();
        std::fs::create_dir_all(real.join("refs")).unwrap();
        std::fs::write(real.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        std::fs::write(work.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        assert_eq!(git_dir_of(&work), Some(real));
    }

    #[tokio::test]
    async fn head_sha_none_for_empty_repo() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("empty.git");
        let out = tokio::process::Command::new("git")
            .args(["init", "--bare", bare.to_str().unwrap()])
            .output()
            .await
            .unwrap();
        assert!(out.status.success());
        assert_eq!(head_sha(Path::new("git"), &bare).await.unwrap(), None);
        assert!(
            list_branches(Path::new("git"), &bare)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(list_tags(Path::new("git"), &bare).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn error_code_for_missing_git_dir() {
        let err = list_branches(Path::new("git"), Path::new("/nonexistent/repo.git"))
            .await
            .unwrap_err();
        // "fatal: not a git repository" maps to RepositoryCorrupted.
        assert_eq!(err.code, ErrorCode::RepositoryCorrupted);
    }
}
