//! Submodule step, end to end: a mirror clone auto-enqueues a
//! SubmoduleSync job when the repo has submodules, and that job
//! fetches the submodule commits into the mirror; a working clone
//! gets its submodules checked out the same way. Local fixtures
//! only, no network, no real GitHub credentials.

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

/// Bare superproject remote whose HEAD has a gitlink `vendor/lib`
/// (relative url `../sub.git`) to a one-commit submodule repo.
/// Returns `(super_remote, submodule_head_sha)`.
fn seed_submodule_remote(dir: &Path) -> (PathBuf, String) {
    git(&["init", "--bare", "-b", "main", "sub.git"], dir);
    git(&["clone", "sub.git", "subwork"], dir);
    let sub_work = dir.join("subwork");
    std::fs::write(sub_work.join("lib.txt"), "lib content\n").unwrap();
    git(&["add", "."], &sub_work);
    git(
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "lib v1",
        ],
        &sub_work,
    );
    git(&["push", "origin", "main"], &sub_work);
    let sub_sha = git(&["rev-parse", "HEAD"], &dir.join("sub.git"));

    git(&["init", "--bare", "-b", "main", "super.git"], dir);
    git(&["clone", "super.git", "superwork"], dir);
    let super_work = dir.join("superwork");
    std::fs::write(super_work.join("README.md"), "# super\n").unwrap();
    git(&["add", "."], &super_work);
    git(
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "super v1",
        ],
        &super_work,
    );
    git(
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "../sub.git",
            "vendor/lib",
        ],
        &super_work,
    );
    git(
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "add submodule",
        ],
        &super_work,
    );
    git(&["push", "origin", "main"], &super_work);
    (dir.join("super.git"), sub_sha)
}

/// Plain bare remote with no submodules.
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
    sync_mode: SyncMode,
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
            sync_mode,
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
async fn mirror_clone_auto_enqueues_submodule_sync() {
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let (remote, sub_sha) = seed_submodule_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let (_, repo) = seed_repo(p, &remote, &storage_root, SyncMode::Mirror).await;

    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].job_type, JobType::MirrorClone);

    let handle = run_pool(p).await;
    let sub_job = wait_for_job(p, repo.id, JobType::SubmoduleSync, JobStatus::Completed).await;
    handle.shutdown();
    handle.join().await;

    // The mirror now holds the submodule commit object, and the
    // submodule job has a successful history row.
    let mirror = storage_root
        .join("repositories")
        .join("github")
        .join("octocat")
        .join("hello.git");
    let out = Command::new("git")
        .args([
            "--git-dir",
            mirror.to_str().unwrap(),
            "cat-file",
            "-e",
            &sub_sha,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "submodule commit {sub_sha} missing from the mirror"
    );
    let history = list_history_for_job(p, sub_job.id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].result, reponyx_core::SyncResult::Success);
}

#[tokio::test]
async fn working_clone_auto_enqueues_submodule_sync() {
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let (remote, _sub_sha) = seed_submodule_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let (_, repo) = seed_repo(p, &remote, &storage_root, SyncMode::Working).await;

    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].job_type, JobType::WorkingClone);

    let handle = run_pool(p).await;
    wait_for_job(p, repo.id, JobType::SubmoduleSync, JobStatus::Completed).await;
    handle.shutdown();
    handle.join().await;

    // The layout puts working clones at
    // <root>/working-clones/<account>/<name> (no `github`
    // segment, unlike the mirror path; account == owner here,
    // so the username appears once).
    let work = storage_root
        .join("working-clones")
        .join("octocat")
        .join("hello");
    assert_eq!(
        std::fs::read_to_string(work.join("vendor/lib/lib.txt")).unwrap(),
        "lib content\n"
    );
}

#[tokio::test]
async fn repo_without_submodules_gets_no_submodule_job() {
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let remote = seed_plain_remote(&fixture);
    let storage_root = dir.path().join("storage");
    let (_, repo) = seed_repo(p, &remote, &storage_root, SyncMode::Mirror).await;

    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(report.created.len(), 1);

    let handle = run_pool(p).await;
    wait_for_job(p, repo.id, JobType::MirrorClone, JobStatus::Completed).await;
    // Give the follow-up checks time to run (they run right after
    // the clone completes), then assert no submodule job exists.
    tokio::time::sleep(Duration::from_secs(1)).await;
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
        jobs.iter().all(|j| j.job_type != JobType::SubmoduleSync),
        "unexpected submodule job: {jobs:?}"
    );
}
