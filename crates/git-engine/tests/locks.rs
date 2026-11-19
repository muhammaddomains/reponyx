//! Integration tests: repository lock contention with real operations.

#[path = "common.rs"]
mod common;

use common::seeded_bare_repo;
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::mirror::{mirror_clone, mirror_fetch};
use reponyx_git_engine::options::SyncOptions;
use std::path::Path;
use std::time::{Duration, Instant};

#[tokio::test]
async fn second_operation_waits_for_the_first() {
    let manager = RepoLockManager::new();
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo.git");
    let _guard = manager
        .acquire(&repo, Duration::from_secs(5))
        .await
        .unwrap();
    // The holder keeps the lock for 300ms; a second acquirer with a
    // 5s timeout must wait (not fail) and then succeed.
    let manager = std::sync::Arc::new(manager);
    let repo2 = repo.clone();
    let manager2 = manager.clone();
    let holder = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(_guard);
    });
    let started = Instant::now();
    let _guard2 = manager2
        .acquire(&repo2, Duration::from_secs(5))
        .await
        .expect("second acquirer should wait, not fail");
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "second acquirer did not wait for the lock"
    );
    holder.await.unwrap();
}

#[tokio::test]
async fn different_paths_run_concurrently() {
    let manager = RepoLockManager::new();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.git");
    let b = dir.path().join("b.git");
    let _guard_a = manager.acquire(&a, Duration::from_secs(5)).await.unwrap();
    let started = Instant::now();
    let _guard_b = manager
        .try_acquire(&b)
        .await
        .expect("a different path must lock immediately");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "different path was blocked by another path's lock"
    );
}

#[tokio::test]
async fn concurrent_fetches_on_one_mirror_serialize() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = std::sync::Arc::new(RepoLockManager::new());
    let mirror = dir.path().join("mirror.git");
    let url = fixture.bare.to_string_lossy().into_owned();
    mirror_clone(
        Path::new("git"),
        &url,
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .unwrap();

    // Two fetches at once on the same mirror: both must succeed
    // (serialized by the lock), never interleaved.
    let locks1 = locks.clone();
    let mirror1 = mirror.clone();
    let first = tokio::spawn(async move {
        mirror_fetch(Path::new("git"), &mirror1, &locks1, SyncOptions::default()).await
    });
    let locks2 = locks.clone();
    let mirror2 = mirror.clone();
    let second = tokio::spawn(async move {
        mirror_fetch(Path::new("git"), &mirror2, &locks2, SyncOptions::default()).await
    });
    first.await.unwrap().expect("first fetch failed");
    second.await.unwrap().expect("second fetch failed");
}
