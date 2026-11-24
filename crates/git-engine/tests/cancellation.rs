//! Integration tests: cancellation + timeouts end to end.
//! Operations run against a fake HTTP server that never responds,
//! so they hang until cancelled or timed out.

#[path = "common.rs"]
mod common;

use common::{run_git, seeded_bare_repo, start_hanging_server};
use reponyx_core::ErrorCode;
use reponyx_git_engine::health::{HealthStatus, verify_repository};
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::mirror::{mirror_clone, mirror_fetch};
use reponyx_git_engine::options::SyncOptions;
use std::path::Path;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn git() -> &'static Path {
    Path::new("git")
}

/// Point a mirror's origin at the hanging server.
async fn point_origin_at(mirror: &Path, url: &str) {
    let out = run_git(
        &[
            "--git-dir",
            mirror.to_str().unwrap(),
            "remote",
            "set-url",
            "origin",
            url,
        ],
        None,
    )
    .await;
    assert!(out.status.success(), "remote set-url failed");
}

#[tokio::test]
async fn cancel_mirror_fetch_mid_flight() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    mirror_clone(
        git(),
        &fixture.bare.to_string_lossy(),
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .unwrap();

    let server = start_hanging_server().await;
    let url = format!("http://127.0.0.1:{}/repo.git", server.addr.port());
    point_origin_at(&mirror, &url).await;

    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        canceller.cancel();
    });
    let opts = SyncOptions::default()
        .with_timeout(Duration::from_secs(30))
        .with_cancel(token);
    let err = mirror_fetch(git(), &mirror, &locks, opts)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::OperationCancelled);

    // The lock is released: a fresh acquire succeeds immediately.
    locks.try_acquire(&mirror).await.unwrap();

    // And no git fetch process is left running.
    #[cfg(target_os = "linux")]
    assert!(
        git_processes_with_arg(&mirror.to_string_lossy()).is_empty(),
        "a git process survived cancellation"
    );
}

#[tokio::test]
async fn fetch_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    mirror_clone(
        git(),
        &fixture.bare.to_string_lossy(),
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .unwrap();

    let server = start_hanging_server().await;
    let url = format!("http://127.0.0.1:{}/repo.git", server.addr.port());
    point_origin_at(&mirror, &url).await;

    let opts = SyncOptions::default().with_timeout(Duration::from_millis(200));
    let err = mirror_fetch(git(), &mirror, &locks, opts)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::OperationTimedOut);
    locks.try_acquire(&mirror).await.unwrap();
}

#[tokio::test]
async fn cancel_mirror_clone_leaves_diagnosable_destination() {
    let dir = tempfile::tempdir().unwrap();
    let locks = RepoLockManager::new();
    let dest = dir.path().join("mirror.git");
    let server = start_hanging_server().await;
    let url = format!("http://127.0.0.1:{}/repo.git", server.addr.port());

    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        canceller.cancel();
    });
    let opts = SyncOptions::default()
        .with_timeout(Duration::from_secs(30))
        .with_cancel(token);
    let err = mirror_clone(git(), &url, &dest, &locks, opts)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::OperationCancelled);

    // The destination is left for diagnosis: either an incomplete
    // clone (missing HEAD/objects), a structurally valid empty
    // repository (git created the skeleton before connecting) — which
    // is indistinguishable from an empty repo and reports Healthy
    // with no HEAD — or a torn skeleton (git was killed mid-write
    // of its metadata, so rev-parse / fsck fail and the diagnosis
    // is Corrupt). All three are diagnosable, non-healthy states;
    // which one appears depends on where the 300 ms cancel lands,
    // and on a loaded runner it is Corrupt. It must never claim to
    // hold the remote's commits.
    //
    // It must also never report Locked: a git killed mid-operation
    // can leave index.lock behind, and the engine removes that
    // leftover on cancel/timeout (see remove_leftover_index_lock),
    // so the diagnosis reaches one of the states above instead of
    // staying locked forever.
    let diagnosis = verify_repository(git(), &dest, Duration::from_secs(30)).await;
    assert!(
        !dest.join("index.lock").exists(),
        "cancelled clone left index.lock behind"
    );
    match diagnosis.status {
        HealthStatus::IncompleteClone | HealthStatus::NotARepository | HealthStatus::Corrupt => {}
        HealthStatus::Healthy => {
            assert!(
                diagnosis.head_sha.is_none(),
                "cancelled clone must not report a HEAD"
            );
        }
        other => panic!("unexpected diagnosis after cancelled clone: {other:?}"),
    }
}

/// PIDs of `git` processes whose command line mentions `needle`
/// (Linux only). Used to prove a cancelled operation leaves no child.
#[cfg(target_os = "linux")]
fn git_processes_with_arg(needle: &str) -> Vec<u32> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let Ok(pid): Result<u32, _> = entry.file_name().to_string_lossy().parse() else {
            continue;
        };
        let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let text = String::from_utf8_lossy(&cmdline);
        if text.contains("git") && text.contains(needle) {
            found.push(pid);
        }
    }
    found
}
