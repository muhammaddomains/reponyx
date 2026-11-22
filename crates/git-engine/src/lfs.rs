//! Git LFS sync + verification (Phase 9).
//!
//! A mirror holds LFS *pointers*, never objects: `git clone --mirror`
//! copies the pointer files but does not download the large-file
//! content. This module is the separate LFS step that runs after a
//! mirror sync: `git lfs fetch --all` downloads every object, then
//! verification checks each object file exists under
//! `<git-dir>/lfs/objects/` and that its SHA-256 matches the pointer
//! oid (the LFS oid *is* the SHA-256 of the content).
//!
//! Verification does not parse `git lfs fsck` output — that text is
//! version-dependent. It reads `git lfs ls-files --all` (stable
//! machine-oriented first column: the oid) and checks the objects
//! directly, so a missing or corrupt object is reported by oid.
//!
//! The fetch takes the same per-repository lock as mirror
//! clone/fetch, streams progress, honours the job timeout and
//! cancellation token, and installs the credential helper so HTTP
//! remotes authenticate the same way a normal fetch does.

use crate::commands::{build_lfs_fetch, build_lfs_ls_files, build_rev_parse_git_dir};
use crate::detect::detect_lfs;
use crate::locks::RepoLockManager;
use crate::options::SyncOptions;
use crate::process::{map_git_failure, run_git_command, run_git_command_streaming};
use crate::refs::REF_TIMEOUT;
use reponyx_core::{ErrorCode, RepoNyxError};
use sha2::{Digest, Sha256};
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// What an LFS fetch + verification produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsSyncSummary {
    /// LFS pointers found in the repository (all refs).
    pub pointers: usize,
    /// Objects that were downloaded by this fetch. Counted from the
    /// objects that exist after the fetch and did not exist before
    /// it; 0 when everything was already present.
    pub fetched: usize,
    /// Oids still missing after the fetch (empty on success).
    pub missing: Vec<String>,
    /// Oids whose stored content does not hash to the oid (empty on
    /// success).
    pub corrupt: Vec<String>,
}

/// Result of checking the stored LFS objects without fetching.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LfsVerifyReport {
    /// LFS pointers found in the repository (all refs).
    pub pointers: usize,
    /// Oids with no object file under `<git-dir>/lfs/objects/`.
    pub missing: Vec<String>,
    /// Oids whose object file exists but whose SHA-256 differs.
    pub corrupt: Vec<String>,
}

impl LfsVerifyReport {
    /// True when every pointer has a present, hash-matching object.
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty() && self.corrupt.is_empty()
    }
}

/// Parse `git lfs ls-files --all --long` output into pointer oids.
///
/// Each line starts with the full object oid (64 lowercase hex, the
/// SHA-256 of the content), e.g. `3f8a… * assets/big.bin`. Lines
/// whose first token is not a 64-hex oid are ignored, so format
/// drift in later git-lfs versions cannot turn a listing into a
/// false pointer. (Without `--long`, ls-files prints an abbreviated
/// 10-character oid, which cannot address the object file.)
pub fn parse_lfs_ls_files(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let oid = line.split_whitespace().next()?;
            let is_oid = oid.len() == 64
                && oid
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
            is_oid.then(|| oid.to_string())
        })
        .collect()
}

/// Path of one LFS object inside a git directory:
/// `<git-dir>/lfs/objects/<oid[0:2]>/<oid[2:4]>/<oid>`.
pub fn lfs_object_path(git_dir: &Path, oid: &str) -> PathBuf {
    git_dir
        .join("lfs")
        .join("objects")
        .join(&oid[0..2])
        .join(&oid[2..4])
        .join(oid)
}

/// SHA-256 hex of a file's contents, streamed so large objects are
/// never read fully into memory.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Resolve the git directory for a repository path (bare or
/// non-bare): `git -C <path> rev-parse --git-dir`, resolved
/// relative to the path when the answer is relative (`.` for a bare
/// repo, `.git` for a working clone).
async fn git_dir_of(git: &Path, repo_path: &Path) -> Result<PathBuf, RepoNyxError> {
    let cmd = build_rev_parse_git_dir(git, repo_path);
    let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel()).await?;
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    let rel = out.stdout.trim();
    let git_dir = PathBuf::from(rel);
    Ok(if git_dir.is_absolute() {
        git_dir
    } else {
        repo_path.join(git_dir)
    })
}

/// List the LFS pointer oids in a repository (all refs).
async fn list_pointer_oids(git: &Path, repo_path: &Path) -> Result<Vec<String>, RepoNyxError> {
    let cmd = build_lfs_ls_files(git, repo_path);
    let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel()).await?;
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    Ok(parse_lfs_ls_files(&out.stdout))
}

/// True when the repository has any LFS pointers (all refs).
/// Fails with `GitLfsFailed` when git-lfs is not installed, so
/// callers can distinguish "no LFS content" from "cannot tell".
pub async fn has_lfs_pointers(git: &Path, repo_path: &Path) -> Result<bool, RepoNyxError> {
    detect_lfs().await?;
    Ok(!list_pointer_oids(git, repo_path).await?.is_empty())
}

/// Verify the stored LFS objects for every pointer in the
/// repository, without fetching. Missing and corrupt oids are
/// reported, not hidden: a mirror that claims an archive it does
/// not hold is worse than one that reports the gap.
pub async fn verify_lfs_objects(
    git: &Path,
    repo_path: &Path,
) -> Result<LfsVerifyReport, RepoNyxError> {
    detect_lfs().await?;
    let oids = list_pointer_oids(git, repo_path).await?;
    if oids.is_empty() {
        return Ok(LfsVerifyReport::default());
    }
    let git_dir = git_dir_of(git, repo_path).await?;
    // Hashing reads file contents on blocking threads; never block
    // the async runtime on a large object.
    let paths: Vec<(String, PathBuf)> = oids
        .iter()
        .map(|oid| (oid.clone(), lfs_object_path(&git_dir, oid)))
        .collect();
    let checked = tokio::task::spawn_blocking(move || {
        let mut missing = Vec::new();
        let mut corrupt = Vec::new();
        for (oid, path) in &paths {
            if !path.is_file() {
                missing.push(oid.clone());
                continue;
            }
            match sha256_file(path) {
                Ok(hash) if hash == *oid => {}
                Ok(_) => corrupt.push(oid.clone()),
                Err(_) => corrupt.push(oid.clone()),
            }
        }
        (missing, corrupt)
    })
    .await
    .map_err(|e| {
        RepoNyxError::new(ErrorCode::InternalError, "LFS verification task failed")
            .with_details(e.to_string())
    })?;
    Ok(LfsVerifyReport {
        pointers: oids.len(),
        missing: checked.0,
        corrupt: checked.1,
    })
}

/// Download every LFS object for a repository, then verify them.
///
/// The per-repository lock is held for the whole operation (same
/// lock as mirror clone/fetch, so an LFS fetch never races a
/// fetch). On verification failure the error names the missing and
/// corrupt oids — the job fails rather than reporting an archive
/// that is not complete.
pub async fn lfs_fetch(
    git: &Path,
    repo_path: &Path,
    locks: &RepoLockManager,
    opts: SyncOptions,
) -> Result<LfsSyncSummary, RepoNyxError> {
    detect_lfs().await?;
    if !repo_path.is_dir() {
        return Err(RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            "LFS fetch target does not exist",
        )
        .with_details(format!("path={}", repo_path.display())));
    }
    let _lock = locks.acquire(repo_path, opts.lock_timeout).await?;

    let before = verify_lfs_objects(git, repo_path).await?;
    let present_before = before.pointers - before.missing.len();

    // A corrupt object is provably not the content its oid names —
    // the object store is a cache of remote content, so remove it
    // and let the fetch re-download the real bytes instead of
    // skipping a file that merely exists under the right name.
    if !before.corrupt.is_empty() {
        let git_dir = git_dir_of(git, repo_path).await?;
        for oid in &before.corrupt {
            let _ = std::fs::remove_file(lfs_object_path(&git_dir, oid));
        }
    }

    let mut cmd = build_lfs_fetch(git, repo_path);
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
        // `map_git_failure` classifies by stderr text; its generic
        // fallback is GitFetchFailed, but this is an LFS operation —
        // keep specific codes (auth, not-found, network) and re-code
        // only the generic one so the job history names LFS.
        let mut err = map_git_failure(&cmd, &out);
        if err.code == ErrorCode::GitFetchFailed {
            err.code = ErrorCode::GitLfsFailed;
        }
        return Err(err);
    }

    let after = verify_lfs_objects(git, repo_path).await?;
    if !after.is_ok() {
        return Err(RepoNyxError::new(
            ErrorCode::GitLfsFailed,
            "LFS verification failed after fetch",
        )
        .with_details(format!(
            "missing={} corrupt={}",
            after.missing.join(","),
            after.corrupt.join(",")
        )));
    }
    let present_after = after.pointers - after.missing.len();
    Ok(LfsSyncSummary {
        pointers: after.pointers,
        fetched: present_after.saturating_sub(present_before),
        missing: after.missing,
        corrupt: after.corrupt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_files_parses_oids_and_ignores_junk() {
        let out = "3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e * assets/big.bin\n\
                   0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef - data/model.bin\n\
                   not-an-oid line here\n\
                   \n";
        let oids = parse_lfs_ls_files(out);
        assert_eq!(oids.len(), 2);
        assert_eq!(
            oids[0],
            "3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e3f8a9c1e"
        );
        assert_eq!(
            oids[1],
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn ls_files_empty_is_empty() {
        assert!(parse_lfs_ls_files("").is_empty());
        assert!(parse_lfs_ls_files("\n\n").is_empty());
    }

    #[test]
    fn object_path_layout() {
        let p = lfs_object_path(
            Path::new("/data/repo.git"),
            "ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34",
        );
        assert_eq!(
            p,
            PathBuf::from(
                "/data/repo.git/lfs/objects/ab/12/ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34"
            )
        );
    }

    #[test]
    fn sha256_file_matches_known_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obj");
        std::fs::write(&path, b"hello world\n").unwrap();
        // sha256("hello world\n") — well-known value.
        assert_eq!(
            sha256_file(&path).unwrap(),
            "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447"
        );
    }

    #[test]
    fn verify_report_ok_only_when_clean() {
        assert!(LfsVerifyReport::default().is_ok());
        assert!(
            !LfsVerifyReport {
                pointers: 1,
                missing: vec!["ab".into()],
                corrupt: vec![],
            }
            .is_ok()
        );
    }
}
