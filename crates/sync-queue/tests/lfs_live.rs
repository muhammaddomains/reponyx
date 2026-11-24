//! LFS step, end to end: a mirror clone auto-enqueues an LfsFetch
//! job when the repo has LFS pointers, and that job downloads +
//! verifies the objects. Local fixtures only, no network, no real
//! GitHub credentials. Skips when `git lfs` is not installed.

use chrono::{Duration as ChronoDuration, Utc};
use reponyx_core::{
    AccountType, AuthenticationMethod, IntervalType, JobStatus, JobType, Provider, ScheduleScope,
    SyncMode, Visibility,
};
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::history::list_history_for_job;
use reponyx_database::jobs::{JobFilter, list_jobs};
use reponyx_database::repositories::{NewRepository, create_repository};
use reponyx_database::schedules::{NewSchedule, create_schedule};
use reponyx_database::settings::set_setting;
use reponyx_github_provider::static_pat;
use reponyx_scheduler::materialize_due;
use reponyx_sync_queue::{GitJobRunner, QueueConfig, StaticCredentialResolver, WorkerPool};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn git_lfs_available() -> bool {
    Command::new("git")
        .args(["lfs", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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

/// Bare remote holding one LFS object (push uploads it).
fn seed_lfs_remote(dir: &Path) -> PathBuf {
    git(&["init", "--bare", "-b", "main", "remote.git"], dir);
    git(&["clone", "remote.git", "work"], dir);
    let work = dir.join("work");
    git(&["lfs", "install", "--local"], &work);
    git(&["lfs", "track", "*.bin"], &work);
    let content: Vec<u8> = (0..10_240).map(|i| (i % 251) as u8).collect();
    std::fs::write(work.join("big.bin"), content).unwrap();
    git(&["add", "."], &work);
    git(
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "lfs file",
        ],
        &work,
    );
    git(&["push", "origin", "main"], &work);
    dir.join("remote.git")
}

/// Plain bare remote with no LFS content.
fn seed_plain_remote(dir: &Path) -> PathBuf {
    git(&["init", "--bare", "-b", "main", "remote.git"], dir);
    git(&["clone", "remote.git", "work"], dir);
    let work = dir.join("work");
    std::fs::write(work.join("hello.txt"), "hello\n").unwrap();
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

async fn seed_repo(
    p: &SqlitePool,
    remote: &Path,
    storage_root: &Path,
) -> (reponyx_core::Account, reponyx_core::Repository) {
    set_setting(p, "storage.root", storage_root.to_str().unwrap())
        .await
        .unwrap();
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
            clone_url: format!("file://{}", remote.display()),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();
    create_schedule(
        p,
        NewSchedule {
            scope: ScheduleScope::Global,
            account_id: None,
            repository_id: None,
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("1".into()),
            timezone: "UTC".into(),
            enabled: true,
            next_run_at: Some(Utc::now() - ChronoDuration::seconds(60)),
        },
    )
    .await
    .unwrap();
    (account, repo)
}

/// Wait until a job of `job_type` with `status` exists for the repo,
/// or panic with the failed rows after the deadline.
async fn wait_for_job(
    p: &SqlitePool,
    repo_id: uuid::Uuid,
    job_type: JobType,
    status: JobStatus,
) -> reponyx_core::SyncJob {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let jobs = list_jobs(
            p,
            JobFilter {
                repository_id: Some(repo_id),
                status: Some(status),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        if let Some(job) = jobs.into_iter().find(|j| j.job_type == job_type) {
            return job;
        }
        if tokio::time::Instant::now() > deadline {
            let failed = list_jobs(
                p,
                JobFilter {
                    repository_id: Some(repo_id),
                    status: Some(JobStatus::Failed),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            panic!("{job_type:?} job did not reach {status:?} in time; failed rows: {failed:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn run_pool(p: &SqlitePool) -> reponyx_sync_queue::QueueHandle {
    let resolver = StaticCredentialResolver::new(static_pat("<redacted>", "octocat"));
    let runner = GitJobRunner::new(resolver);
    let mut config = QueueConfig::default()
        .with_concurrency(2)
        .with_claim_poll(Duration::from_millis(20));
    config.base_backoff = Duration::from_millis(10);
    WorkerPool::start(p.clone(), std::sync::Arc::new(runner), config)
        .await
        .unwrap()
}

#[tokio::test]
async fn mirror_clone_auto_enqueues_lfs_fetch_which_verifies() {
    if !git_lfs_available() {
        eprintln!("skipping: git lfs not installed");
        return;
    }
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let remote = seed_lfs_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let (_, repo) = seed_repo(p, &remote, &storage_root).await;

    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].job_type, JobType::MirrorClone);

    let handle = run_pool(p).await;
    let lfs_job = wait_for_job(p, repo.id, JobType::LfsFetch, JobStatus::Completed).await;
    handle.shutdown();
    handle.join().await;

    // The mirror holds the object now, and the LFS job has a
    // successful history row.
    let mirror = storage_root
        .join("repositories")
        .join("github")
        .join("octocat")
        .join("hello.git");
    let ls = git(
        &[
            "-C",
            mirror.to_str().unwrap(),
            "lfs",
            "ls-files",
            "--all",
            "--long",
        ],
        dir.path(),
    );
    let oid = ls.split_whitespace().next().expect("one pointer expected");
    let object = mirror
        .join("lfs")
        .join("objects")
        .join(&oid[0..2])
        .join(&oid[2..4])
        .join(oid);
    assert!(
        object.is_file(),
        "LFS object missing at {}",
        object.display()
    );
    let history = list_history_for_job(p, lfs_job.id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].result, reponyx_core::SyncResult::Success);
}

#[tokio::test]
async fn repo_without_lfs_gets_no_lfs_job() {
    if !git_lfs_available() {
        eprintln!("skipping: git lfs not installed");
        return;
    }
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let remote = seed_plain_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let (_, repo) = seed_repo(p, &remote, &storage_root).await;

    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);

    let handle = run_pool(p).await;
    wait_for_job(p, repo.id, JobType::MirrorClone, JobStatus::Completed).await;
    handle.shutdown();
    handle.join().await;

    let jobs = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo.id),
            status: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        jobs.iter().all(|j| j.job_type != JobType::LfsFetch),
        "no LFS job must be enqueued for a repo without pointers: {jobs:?}"
    );
}
