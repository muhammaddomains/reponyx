//! Integration tests: mirror clone + fetch (local fixtures only).
//! No network, no GitHub credentials.

#[path = "common.rs"]
mod common;

use common::{commit_file, run_git, seeded_bare_repo, working_clone};
use reponyx_core::ErrorCode;
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::mirror::{mirror_clone, mirror_fetch};
use reponyx_git_engine::options::SyncOptions;
use reponyx_git_engine::progress::ProgressEvent;
use reponyx_git_engine::refs::{head_sha, list_branches, list_tags};
use std::path::Path;
use tokio::sync::mpsc;

fn git() -> &'static Path {
    Path::new("git")
}

#[tokio::test]
async fn mirror_clone_then_fetch_prunes_and_preserves_tags() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    let url = fixture.bare.to_string_lossy().into_owned();

    let info = mirror_clone(git(), &url, &mirror, &locks, SyncOptions::default())
        .await
        .expect("mirror clone failed");
    assert_eq!(info.head_sha.as_deref(), Some(fixture.head_sha.as_str()));
    assert_eq!(info.branches, 3); // main, feature, doomed
    assert_eq!(info.tags, 2); // v1.0.0, v0.9

    // New commit on main, a new tag, and the doomed branch deleted.
    let work2 = dir.path().join("work2");
    working_clone(&fixture.bare, &work2).await;
    commit_file(&work2, "CHANGELOG.md", b"v2\n", "second").await;
    run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work2)).await;
    run_git(
        &["tag", "-a", "v1.1.0", "-m", "release 1.1.0"],
        Some(&work2),
    )
    .await;
    run_git(&["push", "origin", "v1.1.0"], Some(&work2)).await;
    run_git(&["push", "origin", "--delete", "doomed"], Some(&work2)).await;

    let summary = mirror_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect("mirror fetch failed");
    assert!(
        summary.added.iter().any(|r| r == "refs/tags/v1.1.0"),
        "new tag not reported added: {summary:?}"
    );
    assert!(
        summary.pruned.iter().any(|r| r == "refs/heads/doomed"),
        "deleted branch not reported pruned: {summary:?}"
    );
    assert!(
        !summary
            .refs_after
            .contains(&"refs/heads/doomed".to_string())
    );

    // The mirror now has both commits on main and all three tags.
    let out = run_git(
        &[
            "--git-dir",
            mirror.to_str().unwrap(),
            "rev-list",
            "--count",
            "refs/heads/main",
        ],
        None,
    )
    .await;
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
    let tags = list_tags(git(), &mirror).await.unwrap();
    assert_eq!(tags.len(), 3);
    let v1 = tags.iter().find(|t| t.name == "v1.0.0").unwrap();
    assert!(v1.annotated);
    assert_eq!(v1.sha, fixture.annotated_tag_sha);
    assert_eq!(v1.tag_sha, fixture.annotated_tag_object);
    let v09 = tags.iter().find(|t| t.name == "v0.9").unwrap();
    assert!(!v09.annotated);
    assert_eq!(v09.sha, fixture.light_tag_sha);
    assert_eq!(v09.tag_sha, fixture.light_tag_sha);
    let branches = list_branches(git(), &mirror).await.unwrap();
    assert_eq!(branches.len(), 2); // main, feature
    let out = run_git(
        &[
            "--git-dir",
            fixture.bare.to_str().unwrap(),
            "rev-parse",
            "refs/heads/main",
        ],
        None,
    )
    .await;
    let expected_head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        head_sha(git(), &mirror).await.unwrap().as_deref(),
        Some(expected_head.as_str())
    );
}

#[tokio::test]
async fn mirror_clone_reports_real_progress() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    // file:// forces the transport, which is what emits progress.
    let url = format!("file://{}", fixture.bare.display());
    let (tx, mut rx) = mpsc::channel::<ProgressEvent>(256);
    let opts = SyncOptions::default().with_progress(tx);
    let info = mirror_clone(git(), &url, &mirror, &locks, opts)
        .await
        .expect("mirror clone failed");
    assert_eq!(info.head_sha.as_deref(), Some(fixture.head_sha.as_str()));

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    // Progress comes from git itself: at least one ReceivingObjects
    // event, and the last one completes the transfer.
    let receiving: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ProgressEvent::ReceivingObjects {
                received,
                total,
                percent,
                ..
            } => Some((*received, *total, *percent)),
            _ => None,
        })
        .collect();
    assert!(
        !receiving.is_empty(),
        "no ReceivingObjects events: {events:?}"
    );
    let (received, total, percent) = *receiving.last().unwrap();
    assert_eq!(received, total);
    assert_eq!(percent, 100);
}

#[tokio::test]
async fn mirror_clone_rejects_nonempty_destination() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    let url = fixture.bare.to_string_lossy().into_owned();
    mirror_clone(git(), &url, &mirror, &locks, SyncOptions::default())
        .await
        .unwrap();
    let err = mirror_clone(git(), &url, &mirror, &locks, SyncOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn mirror_clone_rejects_embedded_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    let err = mirror_clone(
        git(),
        "https://user:password123@github.com/o/r.git",
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(!mirror.exists(), "no directory must be created");
}

#[tokio::test]
async fn mirror_fetch_on_missing_path_fails() {
    let dir = tempfile::tempdir().unwrap();
    let locks = RepoLockManager::new();
    let err = mirror_fetch(
        git(),
        &dir.path().join("nope.git"),
        &locks,
        SyncOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::RepositoryNotFound);
}

#[tokio::test]
#[cfg(unix)]
async fn mirror_clone_into_readonly_parent_fails() {
    // Permission bits do not stop root from writing, so this
    // simulation is meaningless when the tests run as root (as they
    // do on this VM). The genuine write-failure coverage is the
    // RLIMIT_FSIZE test in disk_full.rs, which works as any user.
    if unsafe { libc::getuid() } == 0 {
        eprintln!("skipping read-only test: running as root");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let readonly_parent = dir.path().join("readonly");
    std::fs::create_dir(&readonly_parent).unwrap();
    // Restore permissions on drop so the tempdir can be cleaned up.
    struct RestorePerms(std::path::PathBuf);
    impl Drop for RestorePerms {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let _restore = RestorePerms(readonly_parent.clone());
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&readonly_parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    }
    let mirror = readonly_parent.join("sub").join("mirror.git");
    let url = fixture.bare.to_string_lossy().into_owned();
    let err = mirror_clone(git(), &url, &mirror, &locks, SyncOptions::default())
        .await
        .unwrap_err();
    // A write failure must surface as a failure, never as success.
    assert_ne!(err.code, ErrorCode::InternalError);
    assert!(!mirror.join("HEAD").exists());
}
