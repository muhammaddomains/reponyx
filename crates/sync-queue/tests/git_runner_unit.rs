//! Unit-level tests for credential resolution and GitJobRunner wiring
//! without hitting a real remote (StaticTokenSource + offline errors).

use reponyx_core::{
    AccountType, AuthenticationMethod, ErrorCode, JobStatus, JobType, Provider, SyncMode,
    Visibility,
};
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::jobs::{NewJob, claim_next_job, create_job};
use reponyx_database::repositories::{NewRepository, create_repository, get_repository};
use reponyx_database::settings::set_setting;
use reponyx_github_provider::static_pat;
use reponyx_sync_queue::{
    GitJobRunner, GitRunnerConfig, JobContext, JobRunner, StaticCredentialResolver,
};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

async fn open_db() -> (TempDir, reponyx_database::Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = reponyx_database::open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    (dir, db)
}

fn pool(db: &reponyx_database::Database) -> &SqlitePool {
    &db.pool
}

/// Run git and fail the test with stderr on any error.
fn git(args: &[&str], dir: &Path) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Build a bare "remote" with one commit; return its path.
fn seed_bare_remote(dir: &Path) -> PathBuf {
    git(&["init", "--bare", "-b", "main", "remote.git"], dir);
    git(&["clone", "remote.git", "work"], dir);
    let work = dir.join("work");
    std::fs::write(work.join("hello.txt"), "hello reponyx\n").unwrap();
    git(&["add", "."], &work);
    git(
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "first",
        ],
        &work,
    );
    git(&["push", "origin", "main"], &work);
    dir.join("remote.git")
}

#[tokio::test]
async fn git_runner_fails_cleanly_when_storage_root_missing() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);

    let account = create_account(
        p,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: "42".into(),
            username: "octocat".into(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap();

    let repo = create_repository(
        p,
        NewRepository {
            account_id: account.id,
            provider_repository_id: "100".into(),
            owner: "octocat".into(),
            name: "hello".into(),
            visibility: Visibility::Public,
            default_branch: Some("main".into()),
            clone_url: "https://github.com/octocat/hello.git".into(),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();

    create_job(
        p,
        NewJob {
            repository_id: repo.id,
            job_type: JobType::MirrorClone,
            priority: 0,
        },
    )
    .await
    .unwrap();

    let job = claim_next_job(p, "test-worker", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, JobStatus::Running);

    // Fake PAT source — we never reach the network if storage.root is unset.
    let resolver = StaticCredentialResolver::new(static_pat("ghp_test_not_real_token", "octocat"));
    let runner = GitJobRunner::new(resolver);

    let err = runner
        .run(JobContext {
            pool: p.clone(),
            job,
            timeout: Duration::from_secs(30),
            cancel: CancellationToken::new(),
            progress: None,
        })
        .await
        .unwrap_err();

    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(
        err.message.contains("storage.root"),
        "unexpected message: {}",
        err.message
    );
}

#[tokio::test]
async fn git_runner_submodule_job_without_local_clone_fails_not_found() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);

    let account = create_account(
        p,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: "43".into(),
            username: "octocat".into(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap();

    let repo = create_repository(
        p,
        NewRepository {
            account_id: account.id,
            provider_repository_id: "101".into(),
            owner: "octocat".into(),
            name: "submodule-demo".into(),
            visibility: Visibility::Public,
            default_branch: Some("main".into()),
            clone_url: "https://github.com/octocat/submodule-demo.git".into(),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();

    create_job(
        p,
        NewJob {
            repository_id: repo.id,
            job_type: JobType::SubmoduleSync,
            priority: 0,
        },
    )
    .await
    .unwrap();

    let job = claim_next_job(p, "test-worker", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let resolver = StaticCredentialResolver::new(static_pat("ghp_test_not_real_token", "octocat"));
    let runner = GitJobRunner::with_config(
        resolver,
        GitRunnerConfig {
            storage_root: Some(std::env::temp_dir()),
            git_path: None,
            ..GitRunnerConfig::default()
        },
    );

    let err = runner
        .run(JobContext {
            pool: p.clone(),
            job,
            timeout: Duration::from_secs(10),
            cancel: CancellationToken::new(),
            progress: None,
        })
        .await
        .unwrap_err();

    assert_eq!(err.code, ErrorCode::RepositoryNotFound);
    assert!(err.message.contains("no local clone"));
}

/// Phase 9: LFS jobs are no longer rejected — but an LFS job for a
/// repository with no local clone yet must fail with
/// RepositoryNotFound, not claim success it did not earn.
#[tokio::test]
async fn git_runner_lfs_job_without_local_clone_fails_not_found() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);

    let account = create_account(
        p,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: "43".into(),
            username: "octocat".into(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap();

    let repo = create_repository(
        p,
        NewRepository {
            account_id: account.id,
            provider_repository_id: "101".into(),
            owner: "octocat".into(),
            name: "lfs-demo".into(),
            visibility: Visibility::Public,
            default_branch: Some("main".into()),
            clone_url: "https://github.com/octocat/lfs-demo.git".into(),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();

    create_job(
        p,
        NewJob {
            repository_id: repo.id,
            job_type: JobType::LfsFetch,
            priority: 0,
        },
    )
    .await
    .unwrap();

    let job = claim_next_job(p, "test-worker", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let resolver = StaticCredentialResolver::new(static_pat("<redacted>", "octocat"));
    let runner = GitJobRunner::with_config(
        resolver,
        GitRunnerConfig {
            storage_root: Some(std::env::temp_dir()),
            git_path: None,
            ..GitRunnerConfig::default()
        },
    );

    let err = runner
        .run(JobContext {
            pool: p.clone(),
            job,
            timeout: Duration::from_secs(10),
            cancel: CancellationToken::new(),
            progress: None,
        })
        .await
        .unwrap_err();

    assert_eq!(err.code, ErrorCode::RepositoryNotFound);
    assert!(err.message.contains("no local clone"));
}

#[tokio::test]
async fn storage_root_setting_is_read() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    set_setting(p, "storage.root", "/tmp/reponyx-test-root")
        .await
        .unwrap();
    let root = reponyx_sync_queue::storage_root_from_settings(p)
        .await
        .unwrap();
    assert_eq!(root, std::path::PathBuf::from("/tmp/reponyx-test-root"));
}

#[tokio::test]
async fn git_runner_fetch_relocates_doubled_layout_to_collapsed() {
    // Account == owner: the mirror was cloned under the old doubled
    // layout (`octocat/octocat/hello.git`). A fetch must move it to
    // the collapsed layout (`octocat/hello.git`), update local_path,
    // and then fetch there.
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let storage = tempfile::tempdir().unwrap();
    let storage_root = storage.path().to_path_buf();
    let fixture = tempfile::tempdir().unwrap();
    let remote = seed_bare_remote(fixture.path());

    let account = create_account(
        p,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: "42".into(),
            username: "octocat".into(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap();

    let old_mirror = storage_root
        .join("repositories")
        .join("github")
        .join("octocat")
        .join("octocat")
        .join("hello.git");
    git(
        &[
            "clone",
            "--mirror",
            remote.to_str().unwrap(),
            old_mirror.to_str().unwrap(),
        ],
        fixture.path(),
    );
    assert!(old_mirror.is_dir());

    let repo = create_repository(
        p,
        NewRepository {
            account_id: account.id,
            provider_repository_id: "100".into(),
            owner: "octocat".into(),
            name: "hello".into(),
            visibility: Visibility::Public,
            default_branch: Some("main".into()),
            clone_url: format!("file://{}", remote.display()),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: Some(old_mirror.to_string_lossy().into_owned()),
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();

    set_setting(p, "storage.root", storage_root.to_str().unwrap())
        .await
        .unwrap();

    create_job(
        p,
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();

    let job = claim_next_job(p, "test-worker", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();

    let resolver = StaticCredentialResolver::new(static_pat("ghp_test_not_real_token", "octocat"));
    let runner = GitJobRunner::new(resolver);
    runner
        .run(JobContext {
            pool: p.clone(),
            job,
            timeout: Duration::from_secs(30),
            cancel: CancellationToken::new(),
            progress: None,
        })
        .await
        .unwrap();

    let new_mirror = storage_root
        .join("repositories")
        .join("github")
        .join("octocat")
        .join("hello.git");
    assert!(!old_mirror.exists(), "old doubled path must be gone");
    assert!(new_mirror.is_dir(), "collapsed path must exist");
    let after = get_repository(p, repo.id).await.unwrap().unwrap();
    assert_eq!(
        after.local_path.as_deref(),
        Some(new_mirror.to_str().unwrap())
    );
    assert!(after.last_synced_at.is_some(), "fetch must have succeeded");
}
