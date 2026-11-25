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
    // One job per repository: the dedupe index allows at most one
    // active job per (repository, job type), so 100 jobs need 100
    // repositories — which also makes this a real concurrency test.
    let mut repo_ids = Vec::new();
    for _ in 0..100 {
        repo_ids.push(seed_repo(p).await);
    }

    for repo_id in &repo_ids {
        create_job(
            p,
            NewJob {
                repository_id: *repo_id,
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
            repository_id: None,
            status: Some(JobStatus::Completed),
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        pending.is_empty(),
        "must not enqueue a retry for permanent auth failure"
    );

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
    let claimed = claim_next_job(p, "test-worker", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .expect("job");
    assert_eq!(claimed.status, JobStatus::Running);

    // Backdate started_at and the lease so recovery treats the job
    // as abandoned (lease-based recovery, not the old grace heuristic).
    sqlx::query("UPDATE sync_jobs SET started_at = ?, lease_expires_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00Z")
        .bind("2020-01-01T00:00:00Z")
        .bind(claimed.id.to_string())
        .execute(p)
        .await
        .unwrap();

    let report = recover_interrupted_jobs(p, Duration::from_secs(1), 6)
        .await
        .unwrap();
    assert_eq!(report.requeued, 1);

    let pending = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Pending),
            ..Default::default()
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
            ..Default::default()
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

#[tokio::test]
async fn sweeper_reclaims_a_job_stranded_after_startup() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let repo_id = seed_repo(p).await;

    let runner = MockJobRunner::always_ok();
    let config = QueueConfig::default()
        .with_concurrency(1)
        .with_sweep_interval(Duration::from_millis(200));
    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();

    // Strand a job AFTER startup: create it and claim it directly
    // (the worker polls every 250 ms, so the direct claim wins),
    // then backdate its lease so the sweeper treats it as
    // abandoned. Without the sweeper this job would stay `running`
    // forever and the dedupe key would block every new job for
    // the repo (audit B9).
    let mut claimed = None;
    for _ in 0..10 {
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
        if let Some(job) = claim_next_job(p, "test-worker", Duration::from_secs(60))
            .await
            .unwrap()
        {
            claimed = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let claimed = claimed.expect("could not claim a job for stranding");
    sqlx::query("UPDATE sync_jobs SET started_at = ?, lease_expires_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00Z")
        .bind("2020-01-01T00:00:00Z")
        .bind(claimed.id.to_string())
        .execute(p)
        .await
        .unwrap();

    // The sweeper (200 ms tick) must requeue it: the stranded row
    // becomes failed and a new pending retry appears, which the
    // worker then runs to completion — so the queue drains.
    wait_for_drain(p, 10).await;
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
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, claimed.id);
    assert_eq!(failed[0].error_code.as_deref(), Some("CrashRecovered"));

    handle.shutdown();
    tokio::time::timeout(Duration::from_secs(5), handle.join())
        .await
        .expect("join timed out");
}

async fn wait_for_drain(p: &SqlitePool, secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
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
            return;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("queue did not drain; pending={pending} running={running}");
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

#[tokio::test]
async fn retry_budget_binds_across_retries() {
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

    // 20 transient failures queued up: with a working budget only 3
    // attempts run (max_attempts = 3). The old bug reset the count
    // on every retry row, so this test would never drain.
    let script = (0..20)
        .map(|_| MockOutcome::Fail {
            code: ErrorCode::NetworkUnavailable,
            message: "net down".into(),
        })
        .collect();
    let runner = MockJobRunner::new(script);

    let mut config = QueueConfig::default()
        .with_concurrency(1)
        .with_claim_poll(Duration::from_millis(20))
        .with_max_attempts(3);
    config.base_backoff = Duration::from_millis(10);
    config.max_backoff = Duration::from_millis(20);

    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();
    wait_for_drain(p, 15).await;
    handle.shutdown();
    handle.join().await;

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
    assert_eq!(failed.len(), 3, "exactly max_attempts failed rows");
    let mut attempts: Vec<i32> = failed.iter().map(|j| j.attempt_count).collect();
    attempts.sort_unstable();
    assert_eq!(
        attempts,
        vec![1, 2, 3],
        "attempt count carries across retries"
    );

    let pending = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_id),
            status: Some(JobStatus::Pending),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(pending.is_empty(), "no further retry may be queued");
}

#[tokio::test]
async fn rate_limiter_defers_job_starts() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    // Two repositories under ONE account, so they share the
    // per-account budget.
    let account = create_account(
        p,
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
    let mut repo_ids = Vec::new();
    for name in ["one", "two"] {
        let repo = create_repository(
            p,
            NewRepository {
                account_id: account.id,
                provider_repository_id: format!("{}", Uuid::new_v4()),
                owner: "tester".into(),
                name: name.into(),
                visibility: Visibility::Private,
                default_branch: Some("main".into()),
                clone_url: format!("https://github.com/tester/{name}.git"),
                sync_enabled: true,
                sync_mode: SyncMode::Mirror,
                local_path: None,
                remote_updated_at: None,
            },
        )
        .await
        .unwrap();
        repo_ids.push(repo.id);
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
    }

    let runner = MockJobRunner::always_ok();
    let mut config = QueueConfig::default()
        .with_concurrency(2)
        .with_claim_poll(Duration::from_millis(20))
        .with_global_jobs_per_minute(0)
        .with_per_account_jobs_per_minute(1);
    config.base_backoff = Duration::from_millis(10);
    let handle = WorkerPool::start(p.clone(), runner, config).await.unwrap();

    // The first job starts; the second must be deferred, not failed.
    tokio::time::sleep(Duration::from_secs(2)).await;
    handle.shutdown();
    handle.join().await;

    let counts = count_jobs_by_status(p).await.unwrap();
    let completed = counts
        .iter()
        .find(|c| c.status == JobStatus::Completed)
        .map(|c| c.count)
        .unwrap_or(0);
    let pending = counts
        .iter()
        .find(|c| c.status == JobStatus::Pending)
        .map(|c| c.count)
        .unwrap_or(0);
    let failed = counts
        .iter()
        .find(|c| c.status == JobStatus::Failed)
        .map(|c| c.count)
        .unwrap_or(0);
    assert_eq!(completed, 1, "one job runs within the budget");
    assert_eq!(pending, 1, "the second job is deferred, not dropped");
    assert_eq!(failed, 0, "rate limiting must never fail a job");

    let deferred = list_jobs(
        p,
        JobFilter {
            repository_id: None,
            status: Some(JobStatus::Pending),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(deferred.len(), 1);
    assert!(
        deferred[0]
            .available_at
            .is_some_and(|t| t > chrono::Utc::now()),
        "deferred job must not be claimable before its wait elapses"
    );
}

#[tokio::test]
async fn lease_expiry_drives_recovery() {
    let (_dir, db) = open_db().await;
    let p = pool(&db);
    let repo_a = seed_repo(p).await;
    let repo_b = seed_repo(p).await;

    for repo_id in [repo_a, repo_b] {
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
    let claimed_a = claim_next_job(p, "worker-a", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let claimed_b = claim_next_job(p, "worker-b", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();

    // A keeps a valid lease: recovery must leave it alone.
    // B has an expired lease AND an exhausted budget: it is failed
    // only, with no retry row.
    sqlx::query("UPDATE sync_jobs SET lease_expires_at = ?, attempt_count = 6 WHERE id = ?")
        .bind("2020-01-01T00:00:00Z")
        .bind(claimed_b.id.to_string())
        .execute(p)
        .await
        .unwrap();

    let report = recover_interrupted_jobs(p, Duration::from_secs(1), 6)
        .await
        .unwrap();
    assert_eq!(report.examined, 2);
    assert_eq!(report.skipped_fresh, 1, "valid lease is not abandoned");
    assert_eq!(report.requeued, 0);
    assert_eq!(
        report.failed_only, 1,
        "exhausted budget fails without requeue"
    );

    let still_running = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_a),
            status: Some(JobStatus::Running),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(still_running.len(), 1);
    assert_eq!(still_running[0].id, claimed_a.id);

    let failed = list_jobs(
        p,
        JobFilter {
            repository_id: Some(repo_b),
            status: Some(JobStatus::Failed),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, claimed_b.id);
    assert_eq!(failed[0].error_code.as_deref(), Some("CrashRecovered"));
}
