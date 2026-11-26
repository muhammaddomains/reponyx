//! Failure injection: the disk cannot grow.
//!
//! Unix-only: `RLIMIT_FSIZE` does not exist on Windows (the `libc`
//! crate has no `rlimit`/`getrlimit`/`setrlimit` there), so this
//! whole file is gated out on non-Unix targets — see the `cfg`
//! below. The release workflow's Windows `test` job failed to
//! compile this file before the gate was added (2026-11-26). The
//! error-mapping half is covered by the `maps_disk_full` unit test
//! in `process.rs`, which runs on every platform; only the live
//! rlimit simulation is Unix-only.
//!
//! What was tried for a true ENOSPC ("No space left on device"):
//! a tiny tmpfs needs mount privileges this VM does not grant, and
//! filling the real filesystem would endanger the machine. The
//! closest feasible simulation is `RLIMIT_FSIZE`: the limit is
//! inherited by the git child, so its pack write fails mid-clone
//! exactly like a disk that cannot grow. A process-wide rlimit is
//! unsafe with parallel tests in one binary, so **this file contains
//! exactly one test** and the limit is set only around the clone
//! (restored by an RAII guard). The error-mapping half is covered by
//! the `maps_disk_full` unit test in `process.rs`.

#![cfg(unix)]

#[path = "common.rs"]
mod common;

use common::{commit_file, pseudo_random_bytes, run_git, set_identity};
use reponyx_git_engine::health::{HealthStatus, verify_repository};
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::mirror::mirror_clone;
use reponyx_git_engine::options::SyncOptions;
use std::path::Path;
use std::time::Duration;

/// RAII guard: cap this process's `RLIMIT_FSIZE` (and therefore every
/// child it spawns) at `limit_bytes`, restoring the original limit on
/// drop. Only `rlim_cur` is lowered — `rlim_max` is left alone so the
/// restore is always permitted.
struct FileSizeLimit {
    original_cur: libc::rlim_t,
    original_max: libc::rlim_t,
}

impl FileSizeLimit {
    fn set(limit_bytes: u64) -> Self {
        let mut current = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut current) };
        assert_eq!(rc, 0, "getrlimit failed");
        let limited = libc::rlimit {
            rlim_cur: limit_bytes as libc::rlim_t,
            rlim_max: current.rlim_max,
        };
        let rc = unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &limited) };
        assert_eq!(rc, 0, "setrlimit failed");
        Self {
            original_cur: current.rlim_cur,
            original_max: current.rlim_max,
        }
    }
}

impl Drop for FileSizeLimit {
    fn drop(&mut self) {
        let restored = libc::rlimit {
            rlim_cur: self.original_cur,
            rlim_max: self.original_max,
        };
        unsafe {
            libc::setrlimit(libc::RLIMIT_FSIZE, &restored);
        }
    }
}

#[tokio::test]
async fn mirror_clone_fails_when_disk_cannot_grow() {
    let dir = tempfile::tempdir().unwrap();
    // Seed a bare repo whose pack is ~3 MiB of incompressible data.
    let bare = dir.path().join("source.git");
    run_git(&["init", "--bare", bare.to_str().unwrap()], None).await;
    let work = dir.path().join("work");
    run_git(
        &["clone", bare.to_str().unwrap(), work.to_str().unwrap()],
        None,
    )
    .await;
    set_identity(&work).await;
    for i in 0..3 {
        commit_file(
            &work,
            &format!("blob{i}.bin"),
            &pseudo_random_bytes(1024 * 1024, 1000 + i as u64),
            &format!("blob {i}"),
        )
        .await;
    }
    run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work)).await;
    run_git(&["symbolic-ref", "HEAD", "refs/heads/main"], Some(&bare)).await;

    let locks = RepoLockManager::new();
    let dest = dir.path().join("mirror.git");
    // file:// forces the transport (a pack transfer); a plain local
    // path clone would hardlink objects and never write pack data.
    let url = format!("file://{}", bare.display());

    // 1 MiB limit: the ~3 MiB pack cannot fit.
    let _limit = FileSizeLimit::set(1024 * 1024);
    let result = mirror_clone(
        Path::new("git"),
        &url,
        &dest,
        &locks,
        SyncOptions::default(),
    )
    .await;
    drop(_limit);

    let err = result.expect_err("clone must fail when the disk cannot grow");
    // A write failure must surface as a Git failure, never as success.
    assert!(
        matches!(
            err.code,
            reponyx_core::ErrorCode::GitCloneFailed
                | reponyx_core::ErrorCode::GitFetchFailed
                | reponyx_core::ErrorCode::DiskSpaceInsufficient
        ),
        "unexpected error code: {:?}",
        err.code
    );
    // The destination must not be reported as a healthy mirror.
    let diagnosis = verify_repository(Path::new("git"), &dest, Duration::from_secs(30)).await;
    assert_ne!(
        diagnosis.status,
        HealthStatus::Healthy,
        "a failed clone must not verify as healthy"
    );
}
