//! Per-repository locking: one mutating operation at a time per
//! local path.
//!
//! Two layers, both required for a lock to be held:
//!
//! 1. **In-process**: an async mutex per canonical path, so tasks in
//!    the same process serialize. This is the fast path and covers the
//!    common case (UI + scheduler in one process).
//! 2. **Cross-process**: an advisory file lock ([`fs2`]) on a lock
//!    file that lives *next to* the repository,
//!    `<parent>/.reponyx-<name>.lock`. The operating system releases
//!    an advisory lock when its holder process dies, so a lock file
//!    left behind by a crashed process is **never stale** — it is just
//!    an unlocked rendezvous file, and the next acquirer locks it
//!    immediately. The lock file is therefore never deleted (deleting
//!    it would reintroduce the classic unlink race between two
//!    processes), and no other process's lock is ever removed.
//!
//! The lock file is placed next to the repository — not inside it —
//! so it never shows up as an untracked file in a working clone and
//! never pollutes a bare mirror. The same file is used whether the
//! repository already exists or is about to be cloned, so a clone and
//! a fetch of the same path can never run concurrently.
//!
//! Only mutating operations (clone, fetch) acquire locks. Read-only
//! operations (status, ref listing, health checks) do not lock.

use fs2::FileExt;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::OwnedMutexGuard;

/// How often to retry the cross-process file lock while waiting.
const FILE_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Manages per-repository locks. Cheap to construct; one instance is
/// normally shared for the whole process (see `RepoLockManager::new`).
pub struct RepoLockManager {
    in_process: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
}

/// An acquired repository lock. The lock is held until this guard is
/// dropped: dropping releases the in-process mutex and closes the
/// lock file, which releases the advisory file lock.
#[derive(Debug)]
pub struct RepoLockGuard {
    _in_process: OwnedMutexGuard<()>,
    _file: Option<File>,
    /// The repository path this guard locks.
    pub path: PathBuf,
}

impl RepoLockManager {
    pub fn new() -> Self {
        Self {
            in_process: Mutex::new(HashMap::new()),
        }
    }

    /// Acquire the lock for `path`, waiting up to `timeout`. Fails
    /// with `RepositoryLocked` when the timeout expires.
    pub async fn acquire(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> Result<RepoLockGuard, RepoNyxError> {
        let deadline = Instant::now() + timeout;
        let key = lock_key(path);

        let slot = {
            let mut map = self.in_process.lock().expect("lock registry poisoned");
            map.entry(key).or_default().clone()
        };
        let in_process = match tokio::time::timeout_at(deadline.into(), slot.lock_owned()).await {
            Ok(guard) => guard,
            Err(_) => return Err(locked(path, "another operation holds the lock")),
        };

        // The in-process mutex is held; now the cross-process file lock.
        // If the file lock cannot be acquired in time, returning early
        // drops the in-process guard — the lock is released.
        let file = match self.acquire_file_lock(path, deadline).await {
            Ok(file) => file,
            Err(e) => return Err(e),
        };

        Ok(RepoLockGuard {
            _in_process: in_process,
            _file: file,
            path: path.to_path_buf(),
        })
    }

    /// Try to acquire the lock for `path` immediately, without waiting.
    /// Fails with `RepositoryLocked` when it is already held.
    pub async fn try_acquire(&self, path: &Path) -> Result<RepoLockGuard, RepoNyxError> {
        let key = lock_key(path);
        let slot = {
            let mut map = self.in_process.lock().expect("lock registry poisoned");
            map.entry(key).or_default().clone()
        };
        let in_process = match slot.try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => return Err(locked(path, "another operation holds the lock")),
        };
        let file = match self.try_file_lock(path) {
            Ok(Some(file)) => Some(file),
            Ok(None) => {
                drop(in_process);
                return Err(locked(path, "another process holds the lock"));
            }
            Err(e) => {
                drop(in_process);
                return Err(e);
            }
        };
        Ok(RepoLockGuard {
            _in_process: in_process,
            _file: file,
            path: path.to_path_buf(),
        })
    }

    /// Probe whether another process currently holds the file lock for
    /// `path`. Best-effort: returns `false` when the lock file does
    /// not exist or cannot be probed. Used by health checks; it never
    /// takes the in-process lock.
    pub fn probe_file_lock(path: &Path) -> bool {
        let Some(lock_path) = lock_file_path(path) else {
            return false;
        };
        let Ok(file) = OpenOptions::new().read(true).write(true).open(&lock_path) else {
            return false;
        };
        // Succeeding means nobody holds it (we drop the file at once,
        // releasing the probe lock immediately).
        file.try_lock_exclusive().is_err()
    }

    /// The path of the lock file for a repository. The file lives in
    /// the repository's parent directory so the same file applies
    /// before and after the repository exists.
    pub fn lock_file_path(path: &Path) -> Option<PathBuf> {
        lock_file_path(path)
    }

    /// Acquire the cross-process file lock, polling until `deadline`.
    async fn acquire_file_lock(
        &self,
        path: &Path,
        deadline: Instant,
    ) -> Result<Option<File>, RepoNyxError> {
        let Some(lock_path) = lock_file_path(path) else {
            // No parent to put the file in: in-process lock only.
            return Ok(None);
        };
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RepoNyxError::new(ErrorCode::InternalError, "Cannot create lock directory")
                    .with_details(format!("{}: {e}", parent.display()))
            })?;
        }
        loop {
            match self.try_file_lock(path)? {
                file @ Some(_) => return Ok(file),
                None => {
                    if Instant::now() >= deadline {
                        return Err(locked(path, "another process holds the lock"));
                    }
                    tokio::time::sleep(FILE_LOCK_POLL_INTERVAL).await;
                }
            }
        }
    }

    /// One attempt at the file lock. `Ok(Some(file))` means locked;
    /// `Ok(None)` means another process holds it; `Err` is an
    /// unexpected I/O failure.
    fn try_file_lock(&self, path: &Path) -> Result<Option<File>, RepoNyxError> {
        let Some(lock_path) = lock_file_path(path) else {
            return Ok(None);
        };
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| {
                RepoNyxError::new(ErrorCode::InternalError, "Cannot open repository lock file")
                    .with_details(format!("{}: {e}", lock_path.display()))
            })?;
        match file.try_lock_exclusive() {
            Ok(()) => {
                write_lock_content(&file, path);
                Ok(Some(file))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(RepoNyxError::new(
                ErrorCode::InternalError,
                "Cannot lock repository lock file",
            )
            .with_details(format!("{}: {e}", lock_path.display()))),
        }
    }
}

impl Default for RepoLockManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Remove stale git lock files left by a killed git process in a
/// git directory: `index.lock`, `packed-refs.lock`, and any
/// `refs/**/*.lock` (a killed fetch leaves those behind, and the
/// next fetch fails with "cannot lock ref" until they are gone).
///
/// Only call after the git child was killed and reaped, while the
/// per-repository lock is held — otherwise a live git process
/// could be mid-write and its lock would be stolen. Best-effort:
/// failures are ignored so the original error is never masked.
/// The lock files themselves are never removed by health checks —
/// only here, after a kill this process performed.
pub fn remove_stale_git_locks(git_dir: &Path) {
    let _ = std::fs::remove_file(git_dir.join("index.lock"));
    let _ = std::fs::remove_file(git_dir.join("packed-refs.lock"));
    // Walk refs/ for *.lock files (e.g. refs/heads/main.lock).
    let mut stack = vec![git_dir.join("refs")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "lock") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

fn locked(path: &Path, reason: &str) -> RepoNyxError {
    RepoNyxError::new(
        ErrorCode::RepositoryLocked,
        format!("Repository is locked: {}", path.display()),
    )
    .with_details(reason.to_string())
}

/// Lock file for a repository: `<parent>/.reponyx-<name>.lock`.
/// `None` when the path has no parent to place the file in.
///
/// Audit G9: the file path used to be built from the raw spelling,
/// so two spellings of the same repository (different `..` segments,
/// a symlinked parent, a different case on Windows) produced two
/// different lock files and two processes could hold "the same"
/// lock. The path is normalized exactly like [`lock_key`] first —
/// canonical when the repository exists, lexical normalization
/// otherwise — so both layers agree on one file per repository.
pub fn lock_file_path(repo_path: &Path) -> Option<PathBuf> {
    let normalized = lock_key(repo_path);
    let parent = normalized.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    let name = normalized.file_name()?.to_string_lossy();
    Some(parent.join(format!(".reponyx-{name}.lock")))
}

/// Registry key: the canonical path when the repository exists,
/// otherwise the absolute path with `.` / `..` resolved lexically.
/// Two spellings of the same repository must map to one lock.
fn lock_key(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Best-effort diagnostic content for the lock file: who holds it and
/// since when. The advisory lock itself — not this content — is the
/// authority; failures to write are ignored.
fn write_lock_content(file: &File, path: &Path) {
    use std::io::Write;
    let mut file = file;
    let pid = std::process::id();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = file.set_len(0);
    let _ = writeln!(file, "pid={pid}");
    let _ = writeln!(file, "created_unix={now}");
    let _ = writeln!(file, "path={}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn second_acquire_while_held_fails_fast() {
        let manager = RepoLockManager::new();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo.git");
        let _guard = manager
            .acquire(&repo, Duration::from_secs(5))
            .await
            .unwrap();
        let err = manager.try_acquire(&repo).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::RepositoryLocked);
    }

    #[tokio::test]
    async fn acquire_times_out_instead_of_waiting_forever() {
        let manager = RepoLockManager::new();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo.git");
        let _guard = manager
            .acquire(&repo, Duration::from_secs(5))
            .await
            .unwrap();
        let started = Instant::now();
        let err = manager
            .acquire(&repo, Duration::from_millis(200))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::RepositoryLocked);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn lock_is_released_on_drop() {
        let manager = RepoLockManager::new();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo.git");
        {
            let _guard = manager
                .acquire(&repo, Duration::from_secs(5))
                .await
                .unwrap();
        }
        manager.try_acquire(&repo).await.unwrap();
    }

    #[tokio::test]
    async fn different_paths_lock_independently() {
        let manager = RepoLockManager::new();
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.git");
        let b = dir.path().join("b.git");
        let _guard_a = manager.acquire(&a, Duration::from_secs(5)).await.unwrap();
        // b is unaffected by a's lock.
        let _guard_b = manager.try_acquire(&b).await.unwrap();
    }

    #[tokio::test]
    async fn lock_file_is_shared_across_managers() {
        // Two managers in one process model two processes: the
        // in-process mutexes differ, but the file lock still excludes.
        let first = RepoLockManager::new();
        let second = RepoLockManager::new();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo.git");
        let _guard = first.acquire(&repo, Duration::from_secs(5)).await.unwrap();
        let err = second.try_acquire(&repo).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::RepositoryLocked);
    }

    /// Audit G9: two spellings of the same repository — a `..`
    /// segment, a symlinked parent — must map to one lock file, so
    /// two processes cannot hold "the same" lock at once.
    #[tokio::test]
    async fn same_repo_different_spelling_shares_the_lock() {
        let first = RepoLockManager::new();
        let second = RepoLockManager::new();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("sub").join("repo.git");
        std::fs::create_dir_all(&repo).unwrap();
        let spelled = dir
            .path()
            .join("sub")
            .join("..")
            .join("sub")
            .join("repo.git");
        assert_eq!(
            RepoLockManager::lock_file_path(&repo),
            RepoLockManager::lock_file_path(&spelled),
            "lock file must not depend on the path spelling"
        );
        let _guard = first.acquire(&repo, Duration::from_secs(5)).await.unwrap();
        let err = second.try_acquire(&spelled).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::RepositoryLocked);
    }

    #[test]
    fn stale_git_locks_removed_only_lock_files() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join("repo.git");
        std::fs::create_dir_all(git_dir.join("refs").join("heads")).unwrap();
        std::fs::write(git_dir.join("index.lock"), "stale").unwrap();
        std::fs::write(git_dir.join("packed-refs.lock"), "stale").unwrap();
        std::fs::write(
            git_dir.join("refs").join("heads").join("main.lock"),
            "stale",
        )
        .unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(git_dir.join("packed-refs"), "refs\n").unwrap();
        remove_stale_git_locks(&git_dir);
        assert!(!git_dir.join("index.lock").exists());
        assert!(!git_dir.join("packed-refs.lock").exists());
        assert!(
            !git_dir
                .join("refs")
                .join("heads")
                .join("main.lock")
                .exists()
        );
        assert!(git_dir.join("HEAD").exists());
        assert!(git_dir.join("packed-refs").exists());
        // Missing locks are not an error.
        remove_stale_git_locks(&git_dir);
    }

    #[test]
    fn lock_file_lives_next_to_the_repo() {
        assert_eq!(
            lock_file_path(Path::new("/data/github/owner/repo.git")),
            Some(PathBuf::from("/data/github/owner/.reponyx-repo.git.lock"))
        );
        assert_eq!(
            lock_file_path(Path::new("/data/work/owner/repo")),
            Some(PathBuf::from("/data/work/owner/.reponyx-repo.lock"))
        );
    }

    #[test]
    fn lock_key_normalizes_spellings() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo.git");
        std::fs::create_dir(&repo).unwrap();
        let via_dot = dir.path().join(".").join("repo.git");
        assert_eq!(lock_key(&repo), lock_key(&via_dot));
        let missing = dir.path().join("sub").join("..").join("repo.git");
        assert_eq!(lock_key(&repo), lock_key(&missing));
    }
}
