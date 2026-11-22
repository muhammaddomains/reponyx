//! Both-mode completion, end to end: a Both repo gets a mirror
//! AND a working clone. The scheduler still creates one job per
//! due repo (MirrorClone for a fresh repo, Fetch once it has a
//! local path); the runner completes the mode — after a MirrorClone
//! it enqueues the WorkingClone, and a Fetch enqueues whichever
//! side is missing. Local fixtures only, no network, no real
//! GitHub credentials.

use chrono::{Duration as ChronoDuration, Utc};
use reponyx_core::{
    AccountType, AuthenticationMethod, IntervalType, JobStatus, JobType, Provider, ScheduleScope,
    SyncMode, Visibility,
};
use reponyx_database::accounts::{NewAccount, create_account};
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

/// Plain bare remote with one commit.
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

async fn seed_both_repo(
    p: &SqlitePool,
    remote: &Path,
    storage_root: &Path,
) -> reponyx_core::Repository {
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
            sync_mode: SyncMode::Both,
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
    repo
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

fn mirror_path(storage_root: &Path) -> PathBuf {
    storage_root
        .join("repositories")
        .join("github")
        .join("octocat")
        .join("octocat")
        .join("hello.git")
}

fn working_path(storage_root: &Path) -> PathBuf {
    storage_root
        .join("working-clones")
        .join("octocat")
        .join("octocat")
        .join("hello")
}

#[tokio::test]
async fn both_repo_gets_mirror_and_working_clone() {
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let remote = seed_plain_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let repo = seed_both_repo(p, &remote, &storage_root).await;

    // The scheduler still creates a single MirrorClone for a fresh
    // Both repo; the runner completes the mode.
    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].job_type, JobType::MirrorClone);

    let handle = run_pool(p).await;
    wait_for_job(p, repo.id, JobType::MirrorClone, JobStatus::Completed).await;
    wait_for_job(p, repo.id, JobType::WorkingClone, JobStatus::Completed).await;
    handle.shutdown();
    handle.join().await;

    assert!(mirror_path(&storage_root).join("HEAD").is_file());
    let work = working_path(&storage_root);
    assert_eq!(
        std::fs::read_to_string(work.join("hello.txt")).unwrap(),
        "hello\n"
    );
}

#[tokio::test]
async fn mirror_only_both_repo_gets_working_clone_on_fetch() {
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let remote = seed_plain_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let repo = seed_both_repo(p, &remote, &storage_root).await;

    // Pre-seed the mirror only (as if an earlier version, or an
    // external tool, built it) and point local_path at it. There is
    // no database helper for this yet (the runner has a local one),
    // so the test issues the same bound-parameter UPDATE.
    let mirror = mirror_path(&storage_root);
    std::fs::create_dir_all(mirror.parent().unwrap()).unwrap();
    git(
        &[
            "clone",
            "--mirror",
            remote.to_str().unwrap(),
            mirror.to_str().unwrap(),
        ],
        dir.path(),
    );
    sqlx::query("UPDATE repositories SET local_path = ?, updated_at = ? WHERE id = ?")
        .bind(mirror.to_str().unwrap())
        .bind(Utc::now().to_rfc3339())
        .bind(repo.id.to_string())
        .execute(p)
        .await
        .unwrap();

    // The scheduler now creates a Fetch (local_path is set); the
    // Fetch fetches the mirror and enqueues the missing working
    // clone instead of failing.
    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].job_type, JobType::Fetch);

    let handle = run_pool(p).await;
    wait_for_job(p, repo.id, JobType::Fetch, JobStatus::Completed).await;
    wait_for_job(p, repo.id, JobType::WorkingClone, JobStatus::Completed).await;
    handle.shutdown();
    handle.join().await;

    let work = working_path(&storage_root);
    assert_eq!(
        std::fs::read_to_string(work.join("hello.txt")).unwrap(),
        "hello\n"
    );
    // A second fetch now finds both sides and completes without
    // enqueuing another clone. (The first materialize advanced the
    // schedule an hour out, so make it due again first.)
    sqlx::query("UPDATE schedules SET next_run_at = ?")
        .bind((Utc::now() - ChronoDuration::seconds(60)).to_rfc3339())
        .execute(p)
        .await
        .unwrap();
    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].job_type, JobType::Fetch);
    let handle = run_pool(p).await;
    wait_for_job(p, repo.id, JobType::Fetch, JobStatus::Completed).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    handle.shutdown();
    handle.join().await;
    let jobs = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo.id),
            status: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        jobs.iter()
            .filter(|j| j.job_type == JobType::WorkingClone)
            .count(),
        1,
        "no second WorkingClone job should have been created: {jobs:?}"
    );
}
