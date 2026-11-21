//! Phase 6 integration tests: concurrency, retries, recovery, permanent fail.

use reponyx_core::{
    AccountType, AuthenticationMethod, ErrorCode, JobStatus, JobType, Provider, SyncMode,
    Visibility,
};
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::jobs::{
    JobFilter, NewJob, claim_next_job, count_jobs_by_status, create_job, list_jobs,
};
use reponyx_database::repositories::{NewRepository, create_repository};
use reponyx_sync_queue::{
    MockJobRunner, MockOutcome, QueueConfig, WorkerPool, recover_interrupted_jobs,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

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

async fn seed_repo(pool: &SqlitePool) -> Uuid {
    let account = create_account(
        pool,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: format!("prov-{}", Uuid::new_v4()),
            username: "tester".into(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap();
    let repo = create_repository(
        pool,
        NewRepository {
            account_id: account.id,
            provider_repository_id: format!("{}", Uuid::new_v4()),
            owner: "tester".into(),
            name: "demo".into(),
            visibility: Visibility::Private,
            default_branch: Some("main".into()),
            clone_url: "https://github.com/tester/demo.git".into(),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();
    repo.id
}

#[tokio::test]
async fn hundred_jobs_complete_with_bounded_concurrency() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let repo_id = seed_repo(p).await;

    for _ in 0..100 {
        create_job(
            p,
            NewJob {
                repository_id: repo_id,
                job_type: JobType::Fetch,
                priority: 0,
            },
        )
        .await
        .unwrap();
    }

    let runner = MockJobRunner::always_ok();
    let config = QueueConfig::default()
        .with_concurrency(5)
        .with_claim_poll(Duration::from_millis(20));
    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();

    // Wait until no pending/running remain.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let counts = count_jobs_by_status(p).await.unwrap();
        let pending = counts
            .iter()
            .find(|c| c.status == JobStatus::Pending)
            .map(|c| c.count)
            .unwrap_or(0);
        let running = counts
            .iter()
            .find(|c| c.status == JobStatus::Running)
            .map(|c| c.count)
            .unwrap_or(0);
        if pending == 0 && running == 0 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("timeout waiting for queue drain; pending={pending} running={running}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    handle.shutdown();
    handle.join().await;

    let completed = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Completed),
        },
    )
    .await
    .unwrap();
    assert_eq!(completed.len(), 100);
}

#[tokio::test]
async fn transient_failure_retries_then_succeeds() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let repo_id = seed_repo(p).await;

    create_job(
        p,
        NewJob {
            repository_id: repo_id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();

    let runner = MockJobRunner::new(vec![
        MockOutcome::Fail {
            code: ErrorCode::NetworkUnavailable,
            message: "net down".into(),
        },
        MockOutcome::Success { duration_ms: 5 },
    ]);

    let config = QueueConfig::default()
        .with_concurrency(1)
        .with_claim_poll(Duration::from_millis(20))
        .with_max_attempts(6);
    // Shrink backoff so the test is fast: base is still used as sleep cap in pool.
    let mut config = config;
    config.base_backoff = Duration::from_millis(10);
    config.max_backoff = Duration::from_millis(20);

    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let completed = list_jobs(
            p,
            JobFilter {
                repository_id: Some(repo_id),
                status: Some(JobStatus::Completed),
            },
        )
        .await
        .unwrap();
        if !completed.is_empty() {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("retry did not succeed in time");
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    handle.shutdown();
    handle.join().await;

    let failed = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Failed),
        },
    )
    .await
    .unwrap();
    // First attempt failed, second (new job) completed.
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].error_code.as_deref(), Some("NetworkUnavailable"));
}

#[tokio::test]
async fn permanent_auth_failure_does_not_retry() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let repo_id = seed_repo(p).await;

    create_job(
        p,
        NewJob {
            repository_id: repo_id,
            job_type: JobType::MirrorClone,
            priority: 0,
        },
    )
    .await
    .unwrap();

    let runner = MockJobRunner::new(vec![MockOutcome::Fail {
        code: ErrorCode::AuthenticationRevoked,
        message: "revoked".into(),
    }]);

    let config = QueueConfig::default()
        .with_concurrency(1)
        .with_claim_poll(Duration::from_millis(20));
    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();

    tokio::time::sleep(Duration::from_millis(400)).await;
    handle.shutdown();
    handle.join().await;

    let pending = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Pending),
        },
    )
    .await
    .unwrap();
    assert!(pending.is_empty(), "must not enqueue a retry for permanent auth failure");

    let failed = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Failed),
        },
    )
    .await
    .unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].error_code.as_deref(),
        Some("AuthenticationRevoked")
    );
}

#[tokio::test]
async fn recovery_requeues_abandoned_running_jobs() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let repo_id = seed_repo(p).await;

    create_job(
        p,
        NewJob {
            repository_id: repo_id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();

    // Claim without completing — simulates a killed worker.
    let claimed = claim_next_job(p).await.unwrap().expect("job");
    assert_eq!(claimed.status, JobStatus::Running);

    // Backdate started_at so recovery grace is exceeded.
    sqlx::query("UPDATE sync_jobs SET started_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00Z")
        .bind(claimed.id.to_string())
        .execute(p)
        .await
        .unwrap();

    let report = recover_interrupted_jobs(p, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(report.requeued, 1);

    let pending = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Pending),
        },
    )
    .await
    .unwrap();
    assert_eq!(pending.len(), 1);
    assert_ne!(pending[0].id, claimed.id, "retry must be a new job row");

    let failed = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Failed),
        },
    )
    .await
    .unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, claimed.id);
    assert_eq!(failed[0].error_code.as_deref(), Some("CrashRecovered"));
}

#[tokio::test]
async fn graceful_shutdown_stops_workers() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let _repo_id = seed_repo(p).await;

    let runner = MockJobRunner::always_ok();
    let config = QueueConfig::default().with_concurrency(2);
    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();
    handle.shutdown();
    // Should return promptly with nothing to do.
    tokio::time::timeout(Duration::from_secs(5), handle.join())
        .await
        .expect("join timed out");
}
