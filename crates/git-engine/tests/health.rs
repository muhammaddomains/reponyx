//! Integration tests: repository health diagnosis.
//! Fixtures are hand-built partial/corrupt repos — deterministic,
//! no timing involved.

#[path = "common.rs"]
mod common;

use common::{commit_file, run_git, seeded_bare_repo, working_clone};
use reponyx_git_engine::health::{HealthStatus, verify_repository};
use std::path::Path;
use std::time::Duration;

fn git() -> &'static Path {
    Path::new("git")
}

fn verify(
    path: &Path,
) -> impl std::future::Future<Output = reponyx_git_engine::health::RepositoryDiagnosis> + '_ {
    verify_repository(git(), path, Duration::from_secs(30))
}

#[tokio::test]
async fn healthy_mirror_and_working_clone() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = reponyx_git_engine::locks::RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    reponyx_git_engine::mirror::mirror_clone(
        git(),
        &fixture.bare.to_string_lossy(),
        &mirror,
        &locks,
        reponyx_git_engine::options::SyncOptions::default(),
    )
    .await
    .unwrap();
    let diagnosis = verify(&mirror).await;
    assert_eq!(diagnosis.status, HealthStatus::Healthy);
    assert_eq!(
        diagnosis.head_sha.as_deref(),
        Some(fixture.head_sha.as_str())
    );
    assert!(diagnosis.error.is_none());

    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;
    let diagnosis = verify(&work).await;
    assert_eq!(diagnosis.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn not_a_repository_cases() {
    let dir = tempfile::tempdir().unwrap();
    // Missing path.
    let diagnosis = verify(&dir.path().join("nope")).await;
    assert_eq!(diagnosis.status, HealthStatus::NotARepository);
    // Empty directory.
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert_eq!(verify(&empty).await.status, HealthStatus::NotARepository);
    // Directory with junk but no Git markers.
    let junk = dir.path().join("junk");
    std::fs::create_dir(&junk).unwrap();
    std::fs::write(junk.join("notes.txt"), "hello\n").unwrap();
    assert_eq!(verify(&junk).await.status, HealthStatus::NotARepository);
}

#[tokio::test]
async fn incomplete_clone_cases() {
    let dir = tempfile::tempdir().unwrap();
    // .git present but HEAD missing.
    let partial = dir.path().join("partial.git");
    std::fs::create_dir_all(partial.join(".git").join("objects")).unwrap();
    std::fs::create_dir_all(partial.join(".git").join("refs")).unwrap();
    assert_eq!(verify(&partial).await.status, HealthStatus::IncompleteClone);
    // HEAD present but objects missing.
    let partial2 = dir.path().join("partial2.git");
    std::fs::create_dir_all(partial2.join(".git").join("refs")).unwrap();
    std::fs::write(partial2.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    assert_eq!(
        verify(&partial2).await.status,
        HealthStatus::IncompleteClone
    );
    // Bare-shaped dir with HEAD but no objects.
    let partial3 = dir.path().join("partial3.git");
    std::fs::create_dir_all(partial3.join("refs")).unwrap();
    std::fs::write(partial3.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    assert_eq!(
        verify(&partial3).await.status,
        HealthStatus::IncompleteClone
    );
}

#[tokio::test]
async fn corrupt_head_is_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;
    std::fs::write(work.join(".git").join("HEAD"), "this is not a valid head\n").unwrap();
    let diagnosis = verify(&work).await;
    assert_eq!(diagnosis.status, HealthStatus::Corrupt);
    assert!(diagnosis.error.is_some());
    assert!(diagnosis.suggested_action.contains("recovery"));
}

#[tokio::test]
async fn corrupt_object_is_caught_by_fsck() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;
    commit_file(&work, "data.txt", b"precious\n", "precious").await;
    // Corrupt the commit object HEAD points at.
    let out = run_git(&["rev-parse", "HEAD"], Some(&work)).await;
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let object = work
        .join(".git")
        .join("objects")
        .join(&sha[..2])
        .join(&sha[2..]);
    let mut bytes = std::fs::read(&object).unwrap();
    bytes[0] ^= 0xff;
    std::fs::write(&object, bytes).unwrap();
    let diagnosis = verify(&work).await;
    assert_eq!(diagnosis.status, HealthStatus::Corrupt);
}

#[tokio::test]
#[cfg(unix)]
async fn stale_and_fresh_index_lock() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let work = dir.path().join("work");
    working_clone(&fixture.bare, &work).await;
    let index_lock = work.join(".git").join("index.lock");
    std::fs::write(&index_lock, "").unwrap();

    // Fresh lock: held by a (supposedly) live process.
    let diagnosis = verify(&work).await;
    assert_eq!(diagnosis.status, HealthStatus::Locked);
    assert!(diagnosis.detail.contains("index.lock"));
    assert!(!diagnosis.detail.contains("Stale"));

    // Backdate the lock by 2 hours: stale.
    backdate(&index_lock, 2 * 3600);
    let diagnosis = verify(&work).await;
    assert_eq!(diagnosis.status, HealthStatus::Locked);
    assert!(
        diagnosis.detail.contains("Stale"),
        "stale lock not flagged: {}",
        diagnosis.detail
    );
}

/// Set a file's atime/mtime to `secs_ago` seconds in the past.
#[cfg(unix)]
fn backdate(path: &Path, secs_ago: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let when = libc::timespec {
        tv_sec: now - secs_ago,
        tv_nsec: 0,
    };
    let times = [when, when];
    let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, cpath.as_ptr(), times.as_ptr(), 0) };
    assert_eq!(rc, 0, "utimensat failed");
}
