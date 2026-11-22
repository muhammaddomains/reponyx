//! LFS fetch + verification (Phase 9). Local fixtures only, no
//! network. Skips when `git lfs` is not installed.

#[path = "common.rs"]
mod common;

use common::{run_git, set_identity};
use reponyx_git_engine::lfs::{has_lfs_pointers, lfs_fetch, verify_lfs_objects};
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::mirror::mirror_clone;
use reponyx_git_engine::options::SyncOptions;
use std::path::{Path, PathBuf};

fn git_lfs_available() -> bool {
    std::process::Command::new("git")
        .args(["lfs", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git() -> &'static Path {
    Path::new("git")
}

/// Deterministic 10 KiB of LFS content.
fn lfs_content() -> Vec<u8> {
    (0..10_240).map(|i| (i % 251) as u8).collect()
}

fn assert_ok(out: std::process::Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build a bare remote holding one LFS object: init bare, seed a
/// work clone with git-lfs tracking `*.bin`, commit, push (the
/// pre-push hook uploads the object to the remote).
/// Returns (remote path, object oid from `git lfs ls-files`).
async fn seed_lfs_remote(dir: &Path) -> (PathBuf, String) {
    let remote = dir.join("remote.git");
    assert_ok(
        run_git(
            &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
            None,
        )
        .await,
        "init bare remote",
    );
    let work = dir.join("work");
    assert_ok(
        run_git(&["init", "-b", "main", work.to_str().unwrap()], None).await,
        "init work clone",
    );
    set_identity(&work).await;
    assert_ok(
        run_git(&["lfs", "install", "--local"], Some(&work)).await,
        "git lfs install",
    );
    assert_ok(
        run_git(&["lfs", "track", "*.bin"], Some(&work)).await,
        "git lfs track",
    );
    std::fs::write(work.join("big.bin"), lfs_content()).unwrap();
    assert_ok(run_git(&["add", "."], Some(&work)).await, "git add");
    assert_ok(
        run_git(&["commit", "-m", "lfs file"], Some(&work)).await,
        "git commit",
    );
    assert_ok(
        run_git(
            &["remote", "add", "origin", remote.to_str().unwrap()],
            Some(&work),
        )
        .await,
        "git remote add",
    );
    assert_ok(
        run_git(&["push", "origin", "main"], Some(&work)).await,
        "git push (uploads the LFS object)",
    );
    // The object must be in the remote's store now.
    let objects = dir.join("remote.git").join("lfs").join("objects");
    assert!(
        objects.is_dir(),
        "remote must hold the LFS object after push"
    );
    (remote, String::new())
}

/// `git lfs ls-files --all --long` in a repo; returns the pointer oids.
fn ls_files_oids(repo: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            repo.to_str().unwrap(),
            "lfs",
            "ls-files",
            "--all",
            "--long",
        ])
        .output()
        .expect("git lfs ls-files failed to run");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

fn object_path(mirror: &Path, oid: &str) -> PathBuf {
    mirror
        .join("lfs")
        .join("objects")
        .join(&oid[0..2])
        .join(&oid[2..4])
        .join(oid)
}

#[tokio::test]
async fn lfs_fetch_downloads_and_verifies_objects() {
    if !git_lfs_available() {
        eprintln!("skipping: git lfs not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (remote, _) = seed_lfs_remote(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    mirror_clone(
        git(),
        remote.to_str().unwrap(),
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .expect("mirror clone failed");

    // The mirror holds the pointer, not the object.
    assert!(has_lfs_pointers(git(), &mirror).await.unwrap());
    let oids = ls_files_oids(&mirror);
    assert_eq!(oids.len(), 1, "one LFS pointer expected");
    let oid = &oids[0];
    assert!(
        !object_path(&mirror, oid).is_file(),
        "object must be absent before fetch"
    );
    let before = verify_lfs_objects(git(), &mirror).await.unwrap();
    assert_eq!(before.pointers, 1);
    assert_eq!(before.missing, vec![oid.clone()]);
    assert!(before.corrupt.is_empty());

    // Fetch downloads it and verifies.
    let summary = lfs_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect("lfs fetch failed");
    assert_eq!(summary.pointers, 1);
    assert_eq!(summary.fetched, 1);
    assert!(summary.missing.is_empty());
    assert!(summary.corrupt.is_empty());
    assert!(
        object_path(&mirror, oid).is_file(),
        "object must exist after fetch"
    );
    assert!(verify_lfs_objects(git(), &mirror).await.unwrap().is_ok());

    // Second fetch is a no-op.
    let again = lfs_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect("second lfs fetch failed");
    assert_eq!(again.fetched, 0);
}

#[tokio::test]
async fn verify_reports_missing_and_corrupt_objects() {
    if !git_lfs_available() {
        eprintln!("skipping: git lfs not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (remote, _) = seed_lfs_remote(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    mirror_clone(
        git(),
        remote.to_str().unwrap(),
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .expect("mirror clone failed");
    lfs_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect("lfs fetch failed");
    let oid = ls_files_oids(&mirror).into_iter().next().unwrap();
    let path = object_path(&mirror, &oid);

    // Deleted object -> missing.
    std::fs::remove_file(&path).unwrap();
    let report = verify_lfs_objects(git(), &mirror).await.unwrap();
    assert_eq!(report.missing, vec![oid.clone()]);
    assert!(report.corrupt.is_empty());
    // Fetch repairs a missing object.
    lfs_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect("re-fetch failed");
    assert!(verify_lfs_objects(git(), &mirror).await.unwrap().is_ok());

    // Corrupted bytes -> corrupt, and a fetch self-heals by
    // re-downloading the provably-wrong file. The corrupt file is
    // written as a NEW file (delete + write), not edited in place:
    // local-path remotes hardlink LFS objects, so an in-place
    // rewrite would corrupt the remote's copy too.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] ^= 0xff;
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, bytes).unwrap();
    let report = verify_lfs_objects(git(), &mirror).await.unwrap();
    assert!(report.missing.is_empty());
    assert_eq!(report.corrupt, vec![oid.clone()]);
    lfs_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect("fetch after corruption failed");
    assert!(
        verify_lfs_objects(git(), &mirror).await.unwrap().is_ok(),
        "corrupt object must be re-downloaded and verify clean"
    );
}

#[tokio::test]
async fn lfs_fetch_fails_when_remote_lacks_objects() {
    if !git_lfs_available() {
        eprintln!("skipping: git lfs not installed");
        return;
    }
    // A bare clone copies pointers but NOT LFS objects — the same
    // shape as a remote that never received them. The fetch must
    // fail with the missing oid named, not report success.
    let dir = tempfile::tempdir().unwrap();
    let (remote, _) = seed_lfs_remote(dir.path()).await;
    let bare_copy = dir.path().join("remote-nobjects.git");
    assert_ok(
        run_git(
            &[
                "clone",
                "--bare",
                remote.to_str().unwrap(),
                bare_copy.to_str().unwrap(),
            ],
            None,
        )
        .await,
        "bare clone of the remote",
    );
    assert!(
        !bare_copy.join("lfs").join("objects").is_dir(),
        "bare clone must not carry LFS objects"
    );
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    mirror_clone(
        git(),
        bare_copy.to_str().unwrap(),
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .expect("mirror clone failed");
    let err = lfs_fetch(git(), &mirror, &locks, SyncOptions::default())
        .await
        .expect_err("fetch must fail when the remote lacks the object");
    assert_eq!(err.code.as_str(), "GitLfsFailed");
    assert!(
        err.details
            .as_deref()
            .unwrap_or("")
            .contains("remote missing object"),
        "error must name the missing object: {err:?}"
    );
}
