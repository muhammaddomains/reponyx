//! Live end-to-end chain: scheduler -> queue -> real git mirror clone.
//!
//! No network and no real GitHub credentials: the "remote" is a local bare
//! repository created with the git CLI, and credentials come from a static
//! fake PAT source (a `file://` URL needs no auth at all). This proves the
//! pieces are linked in real time on this machine: a due schedule creates a
//! job row, the worker pool claims it, and the git engine lands a real
//! mirror clone on disk carrying the remote's refs.

use chrono::{Duration as ChronoDuration, Utc};
use reponyx_core::{
    AccountType, AuthenticationMethod, IntervalType, JobStatus, JobType, Provider, ScheduleScope,
    SyncMode, Visibility,
};
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::history::list_history_for_job;
use reponyx_database::jobs::{JobFilter, list_jobs};
use reponyx_database::repositories::{NewRepository, create_repository, get_repository};
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

/// Build a bare "remote" with two commits; return its path and HEAD sha.
fn seed_bare_remote(dir: &Path) -> (PathBuf, String) {
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
    std::fs::write(work.join("second.txt"), "second\n").unwrap();
    git(&["add", "."], &work);
    git(
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "second",
        ],
        &work,
    );
    git(&["push", "origin", "main"], &work);
    let head = git(&["rev-parse", "HEAD"], &work);
    (dir.join("remote.git"), head)
}

#[tokio::test]
async fn scheduler_to_queue_to_real_mirror_clone() {
    let (dir, db) = open_db().await;
    let p = pool(&db);
    let fixture = dir.path().join("fixture");
    std::fs::create_dir(&fixture).unwrap();
    let (remote, head) = seed_bare_remote(&fixture);

    let storage_root = dir.path().join("storage");
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

    // 1. The scheduler creates the job for the due schedule.
    let report = materialize_due(p, Utc::now()).await.unwrap();
    assert_eq!(
        report.created.len(),
        1,
        "one due schedule must create one job"
    );
    assert_eq!(report.created[0].job_type, JobType::MirrorClone);

    // 2. The queue runs it with the real git runner.
    let resolver = StaticCredentialResolver::new(static_pat("ghp_test_not_real_token", "octocat"));
    let runner = GitJobRunner::new(resolver);
    let mut config = QueueConfig::default()
        .with_concurrency(2)
        .with_claim_poll(Duration::from_millis(20));
    config.base_backoff = Duration::from_millis(10);
    let handle = WorkerPool::start(p.clone(), std::sync::Arc::new(runner), config)
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let completed = loop {
        let done = list_jobs(
            p,
            JobFilter {
                repository_id: Some(repo.id),
                status: Some(JobStatus::Completed),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        if !done.is_empty() {
            break done;
        }
        if tokio::time::Instant::now() > deadline {
            let failed = list_jobs(
                p,
                JobFilter {
                    repository_id: Some(repo.id),
                    status: Some(JobStatus::Failed),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            panic!("job did not complete in time; failed rows: {failed:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    handle.shutdown();
    handle.join().await;

    // 3. The mirror clone is really on disk with the remote's refs.
    // Account == owner: the layout collapses the username to one segment.
    let mirror = storage_root
        .join("repositories")
        .join("github")
        .join("octocat")
        .join("hello.git");
    assert!(
        mirror.join("HEAD").exists(),
        "mirror clone missing at {}",
        mirror.display()
    );
    let mirror_head = git(
        &["--git-dir", mirror.to_str().unwrap(), "rev-parse", "HEAD"],
        dir.path(),
    );
    assert_eq!(mirror_head, head, "mirror HEAD must match the remote");
    let refs = git(
        &[
            "--git-dir",
            mirror.to_str().unwrap(),
            "for-each-ref",
            "--format=%(refname)",
        ],
        dir.path(),
    );
    assert!(
        refs.contains("refs/heads/main"),
        "mirror must carry main; refs: {refs}"
    );

    // 4. The repo row knows its local path, and history was recorded.
    let repo_after = get_repository(p, repo.id).await.unwrap().unwrap();
    assert_eq!(
        repo_after.local_path.as_deref(),
        Some(mirror.to_str().unwrap())
    );
    let history = list_history_for_job(p, completed[0].id).await.unwrap();
    assert!(
        !history.is_empty(),
        "a completed job must have a history row"
    );
}
