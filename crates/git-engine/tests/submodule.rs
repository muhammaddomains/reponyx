//! Submodule sync fixtures: a superproject whose HEAD carries a
//! gitlink to a submodule commit, and the submodule repository
//! itself. All local paths, no network.
//!
//! The fixture flow mirrors reality: the submodule repo is a bare
//! remote; the superproject's `.gitmodules` declares the submodule
//! with a RELATIVE url (`../sub.git`), the common GitHub layout.
//! A mirror clone of the superproject has the gitlink but NOT the
//! submodule objects — that is what the mirror-side sync fetches.

#[path = "common.rs"]
mod common;

use common::{commit_file, run_git, set_identity};
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::options::SyncOptions;
use reponyx_git_engine::submodule::{
    list_submodules, sync_submodules_mirror, sync_submodules_working,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn git() -> PathBuf {
    PathBuf::from("git")
}

fn sha_of(repo: &Path, rev: &str) -> String {
    let out = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(out.status.success(), "rev-parse failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn cat_file_exists(git_dir: &Path, sha: &str) -> bool {
    Command::new("git")
        .args([
            "--git-dir",
            &git_dir.to_string_lossy(),
            "cat-file",
            "-e",
            sha,
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Seed: `sub.git` (bare, one commit `lib.txt`) and `super.git`
/// (bare) whose HEAD has a gitlink `vendor/lib` -> the submodule
/// commit and a `.gitmodules` entry with the relative url
/// `../sub.git`. When `broken` is true, a second submodule
/// `vendor/broken` is added whose url points at a repo that does
/// not exist (`../missing.git`) — same commit sha, so only its url
/// is bad. Returns `(super_bare, sub_sha)`.
async fn seed_submodule_fixture(dir: &Path, broken: bool) -> (PathBuf, String) {
    let sub_bare = dir.join("sub.git");
    run_git(
        &["init", "--bare", "-b", "main", sub_bare.to_str().unwrap()],
        None,
    )
    .await;
    let sub_work = dir.join("subwork");
    run_git(
        &[
            "clone",
            sub_bare.to_str().unwrap(),
            sub_work.to_str().unwrap(),
        ],
        None,
    )
    .await;
    set_identity(&sub_work).await;
    commit_file(&sub_work, "lib.txt", b"lib content\n", "lib v1").await;
    run_git(&["push", "origin", "main"], Some(&sub_work)).await;
    let sub_sha = sha_of(&sub_bare, "HEAD");

    let super_bare = dir.join("super.git");
    run_git(
        &["init", "--bare", "-b", "main", super_bare.to_str().unwrap()],
        None,
    )
    .await;
    let super_work = dir.join("superwork");
    run_git(
        &[
            "clone",
            super_bare.to_str().unwrap(),
            super_work.to_str().unwrap(),
        ],
        None,
    )
    .await;
    set_identity(&super_work).await;
    commit_file(&super_work, "README.md", b"# super\n", "super v1").await;
    // Relative url, the common GitHub layout; local file protocol
    // needs the explicit allow.
    let out = run_git(
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "../sub.git",
            "vendor/lib",
        ],
        Some(&super_work),
    )
    .await;
    assert!(
        out.status.success(),
        "submodule add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    run_git(&["commit", "-m", "add submodule"], Some(&super_work)).await;
    if broken {
        // A second gitlink with a bad url: same sha (a real object
        // in sub.git), but ../missing.git does not exist.
        let gitmodules = super_work.join(".gitmodules");
        let mut text = std::fs::read_to_string(&gitmodules).unwrap();
        text.push_str(
            "[submodule \"vendor/broken\"]\n\tpath = vendor/broken\n\turl = ../missing.git\n",
        );
        std::fs::write(&gitmodules, text).unwrap();
        run_git(&["add", ".gitmodules"], Some(&super_work)).await;
        let out = run_git(
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{sub_sha},vendor/broken"),
            ],
            Some(&super_work),
        )
        .await;
        assert!(out.status.success(), "cacheinfo failed");
        run_git(&["commit", "-m", "add broken submodule"], Some(&super_work)).await;
    }
    run_git(&["push", "origin", "main"], Some(&super_work)).await;
    (super_bare, sub_sha)
}

fn mirror_clone(super_bare: &Path, dest: &Path) {
    let out = Command::new("git")
        .args([
            "clone",
            "--mirror",
            super_bare.to_str().unwrap(),
            dest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "mirror clone failed");
}

#[tokio::test]
async fn mirror_sync_fetches_submodule_commits() {
    let dir = TempDir::new().unwrap();
    let (super_bare, sub_sha) = seed_submodule_fixture(dir.path(), false).await;
    let mirror = dir.path().join("mirror.git");
    mirror_clone(&super_bare, &mirror);

    // The mirror has the gitlink but not the object.
    let subs = list_submodules(&git(), &mirror).await.unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].path, "vendor/lib");
    assert_eq!(subs[0].url.as_deref(), Some("../sub.git"));
    assert_eq!(subs[0].sha.as_deref(), Some(sub_sha.as_str()));
    assert!(!cat_file_exists(&mirror, &sub_sha));

    let locks = RepoLockManager::new();
    let summary = sync_submodules_mirror(&git(), &mirror, &locks, SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 1);
    assert_eq!(summary.synced, 1);
    assert!(summary.is_ok());
    assert!(cat_file_exists(&mirror, &sub_sha));

    // Idempotent: a second pass finds the object already present.
    let summary = sync_submodules_mirror(&git(), &mirror, &locks, SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 1);
    assert_eq!(summary.synced, 1);
    assert!(summary.is_ok());
}

#[tokio::test]
async fn mirror_sync_isolates_inaccessible_submodule() {
    let dir = TempDir::new().unwrap();
    let (super_bare, sub_sha) = seed_submodule_fixture(dir.path(), true).await;
    let mirror = dir.path().join("mirror.git");
    mirror_clone(&super_bare, &mirror);

    let locks = RepoLockManager::new();
    let summary = sync_submodules_mirror(&git(), &mirror, &locks, SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 2);
    assert_eq!(summary.synced, 1);
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].path, "vendor/broken");
    assert!(!summary.is_ok());
    // The good submodule still archived.
    assert!(cat_file_exists(&mirror, &sub_sha));
}

#[tokio::test]
async fn working_sync_checks_out_submodules() {
    let dir = TempDir::new().unwrap();
    let (super_bare, _sub_sha) = seed_submodule_fixture(dir.path(), false).await;
    let work = dir.path().join("work");
    let out = run_git(
        &[
            "clone",
            super_bare.to_str().unwrap(),
            work.to_str().unwrap(),
        ],
        None,
    )
    .await;
    assert!(out.status.success(), "working clone failed");
    // Plain clone: the submodule directory is empty.
    assert!(!work.join("vendor/lib/lib.txt").exists());

    let locks = RepoLockManager::new();
    let summary = sync_submodules_working(&git(), &work, &locks, SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 1);
    assert_eq!(summary.synced, 1);
    assert!(summary.is_ok());
    assert_eq!(
        std::fs::read_to_string(work.join("vendor/lib/lib.txt")).unwrap(),
        "lib content\n"
    );
}

#[tokio::test]
async fn repo_without_submodules_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let bare = dir.path().join("plain.git");
    run_git(
        &["init", "--bare", "-b", "main", bare.to_str().unwrap()],
        None,
    )
    .await;
    let work = dir.path().join("plainwork");
    run_git(
        &["clone", bare.to_str().unwrap(), work.to_str().unwrap()],
        None,
    )
    .await;
    set_identity(&work).await;
    commit_file(&work, "a.txt", b"a\n", "a").await;
    run_git(&["push", "origin", "main"], Some(&work)).await;
    let mirror = dir.path().join("mirror.git");
    mirror_clone(&bare, &mirror);

    let locks = RepoLockManager::new();
    let summary = sync_submodules_mirror(&git(), &mirror, &locks, SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 0);
    assert!(summary.is_ok());
    let summary = sync_submodules_working(&git(), &work, &locks, SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.total, 0);
    assert!(summary.is_ok());
}
