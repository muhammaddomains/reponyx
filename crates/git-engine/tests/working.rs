//! Integration tests: working-clone fetch, status, divergence.
//! Local fixtures only. The key safety property under test: a fetch
//! never touches local work.

#[path = "common.rs"]
mod common;

use common::{commit_file, run_git, seeded_bare_repo, working_clone};
use reponyx_core::ErrorCode;
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::options::SyncOptions;
use reponyx_git_engine::working::{WorkingState, divergence, working_fetch, working_status};
use std::path::Path;

fn git() -> &'static Path {
    Path::new("git")
}

#[tokio::test]
async fn fetch_never_touches_local_work() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;

    // Local uncommitted change + an untracked file.
    std::fs::write(work.join("README.md"), "# locally modified\n").unwrap();
    std::fs::write(work.join("scratch.txt"), "do not touch\n").unwrap();
    let status = working_status(git(), &work).await.unwrap();
    assert_eq!(status.state, WorkingState::Modified);

    // A new commit lands upstream.
    let work2 = dir.path().join("work2");
    working_clone(&fixture.bare, &work2).await;
    commit_file(&work2, "upstream.txt", b"upstream\n", "upstream").await;
    run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work2)).await;

    working_fetch(git(), &work, &locks, SyncOptions::default())
        .await
        .expect("working fetch failed");

    // Local work is untouched.
    assert_eq!(
        std::fs::read_to_string(work.join("README.md")).unwrap(),
        "# locally modified\n"
    );
    assert!(work.join("scratch.txt").exists());
    // The checkout did not move (still the old commit).
    let out = run_git(&["rev-parse", "HEAD"], Some(&work)).await;
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        fixture.head_sha
    );
    // But the remote-tracking ref advanced: 1 behind, 0 ahead.
    let div = divergence(git(), &work, None).await.unwrap();
    assert_eq!(div.upstream.as_deref(), Some("refs/remotes/origin/main"));
    assert_eq!(div.ahead, 0);
    assert_eq!(div.behind, 1);
    let status = working_status(git(), &work).await.unwrap();
    assert_eq!(status.state, WorkingState::Modified);
}

#[tokio::test]
async fn status_states() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;

    assert_eq!(
        working_status(git(), &work).await.unwrap().state,
        WorkingState::Clean
    );

    std::fs::write(work.join("untracked.txt"), "new\n").unwrap();
    assert_eq!(
        working_status(git(), &work).await.unwrap().state,
        WorkingState::UntrackedOnly
    );

    std::fs::write(work.join("README.md"), "# changed\n").unwrap();
    assert_eq!(
        working_status(git(), &work).await.unwrap().state,
        WorkingState::Modified
    );

    run_git(&["add", "README.md"], Some(&work)).await;
    let status = working_status(git(), &work).await.unwrap();
    assert_eq!(status.state, WorkingState::Staged);
    assert_eq!(status.entries[0].code, "M ");

    // A merge conflict.
    run_git(&["checkout", "-b", "side"], Some(&work)).await;
    std::fs::write(work.join("README.md"), "# side\n").unwrap();
    run_git(&["add", "."], Some(&work)).await;
    run_git(&["commit", "-m", "side"], Some(&work)).await;
    run_git(&["checkout", "main"], Some(&work)).await;
    std::fs::write(work.join("README.md"), "# main\n").unwrap();
    run_git(&["add", "."], Some(&work)).await;
    run_git(&["commit", "-m", "main"], Some(&work)).await;
    let out = run_git(&["merge", "side"], Some(&work)).await;
    assert!(!out.status.success(), "merge should conflict");
    assert_eq!(
        working_status(git(), &work).await.unwrap().state,
        WorkingState::Conflicted
    );
}

#[tokio::test]
async fn divergence_ahead_and_behind() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;

    // One local commit: ahead 1, behind 0.
    commit_file(&work, "local.txt", b"local\n", "local").await;
    let div = divergence(git(), &work, None).await.unwrap();
    assert_eq!(div.ahead, 1);
    assert_eq!(div.behind, 0);

    // One upstream commit: ahead 1, behind 1.
    let work2 = dir.path().join("work2");
    working_clone(&fixture.bare, &work2).await;
    commit_file(&work2, "upstream.txt", b"upstream\n", "upstream").await;
    run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work2)).await;
    run_git(&["fetch", "origin"], Some(&work)).await;
    let div = divergence(git(), &work, None).await.unwrap();
    assert_eq!(div.ahead, 1);
    assert_eq!(div.behind, 1);

    // A branch with no upstream reports None, not an error.
    run_git(&["checkout", "-b", "lonely"], Some(&work)).await;
    let div = divergence(git(), &work, Some("lonely")).await.unwrap();
    assert_eq!(div.upstream, None);
    assert_eq!(div.ahead, 0);
    assert_eq!(div.behind, 0);
}

#[tokio::test]
async fn fetch_requires_a_working_clone() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let err = working_fetch(git(), &fixture.bare, &locks, SyncOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RepositoryNotFound);
}
