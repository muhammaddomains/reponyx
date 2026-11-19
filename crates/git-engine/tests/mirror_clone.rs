//! Integration test: mirror clone + fetch against local Git fixtures.
//! No network, no GitHub credentials. The source is a local bare repo
//! created with `git init --bare`, seeded with a commit via a temp clone.

use reponyx_git_engine::commands::{build_clone_mirror, build_fetch};
use reponyx_git_engine::process::run_git_command;
use std::path::Path;
use std::time::Duration;

fn cancel_channel() -> tokio::sync::watch::Receiver<bool> {
    let (_tx, rx) = tokio::sync::watch::channel(false);
    rx
}

async fn run_git(args: &[&str], cwd: Option<&Path>) -> std::process::Output {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.output().await.expect("git fixture command failed")
}

/// Create a bare repo with one commit on `main`, return its path.
async fn seeded_bare_repo(dir: &Path) -> std::path::PathBuf {
    let bare = dir.join("source.git");
    let out = run_git(&["init", "--bare", bare.to_str().unwrap()], None).await;
    assert!(out.status.success(), "init --bare failed");

    let work = dir.join("work");
    let out = run_git(
        &["clone", bare.to_str().unwrap(), work.to_str().unwrap()],
        None,
    )
    .await;
    assert!(out.status.success(), "seed clone failed");
    run_git(&["config", "user.email", "test@reponyx.local"], Some(&work)).await;
    run_git(&["config", "user.name", "RepoNyx Test"], Some(&work)).await;
    std::fs::write(work.join("README.md"), "# hello\n").unwrap();
    run_git(&["add", "."], Some(&work)).await;
    run_git(&["commit", "-m", "initial"], Some(&work)).await;
    let out = run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work)).await;
    assert!(
        out.status.success(),
        "seed push failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    run_git(&["symbolic-ref", "HEAD", "refs/heads/main"], Some(&bare)).await;
    bare
}

#[tokio::test]
async fn mirror_clone_preserves_history_and_fetch_updates() {
    let dir = tempfile::tempdir().unwrap();
    let bare = seeded_bare_repo(dir.path()).await;
    let url = bare.to_string_lossy().into_owned();
    let mirror = dir.path().join("mirror.git");

    // Mirror clone.
    let cmd = build_clone_mirror(Path::new("git"), &url, &mirror);
    let out = run_git_command(&cmd, Duration::from_secs(30), cancel_channel())
        .await
        .expect("mirror clone command failed");
    assert!(out.success, "mirror clone failed: {}", out.stderr);
    assert!(mirror.join("HEAD").exists());
    assert!(mirror.join("objects").is_dir());

    // Second commit in the source, then fetch --prune into the mirror.
    let work2 = dir.path().join("work2");
    run_git(&["clone", &url, work2.to_str().unwrap()], None).await;
    run_git(
        &["config", "user.email", "test@reponyx.local"],
        Some(&work2),
    )
    .await;
    run_git(&["config", "user.name", "RepoNyx Test"], Some(&work2)).await;
    std::fs::write(work2.join("CHANGELOG.md"), "v2\n").unwrap();
    run_git(&["add", "."], Some(&work2)).await;
    run_git(&["commit", "-m", "second"], Some(&work2)).await;
    run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work2)).await;

    let cmd = build_fetch(Path::new("git"), &mirror, true);
    let out = run_git_command(&cmd, Duration::from_secs(30), cancel_channel())
        .await
        .expect("fetch command failed");
    assert!(out.success, "fetch failed: {}", out.stderr);

    // The mirror now contains both commits.
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
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}
