//! Job + history repository integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::{ErrorCode, JobStatus, JobType, SyncResult};
use reponyx_database::history::{count_history_for_repository, list_history_for_job};
use reponyx_database::jobs::{
    JobFilter, JobOutcome, NewJob, cancel_job, claim_next_job, complete_job,
    complete_job_with_history, count_jobs_by_status, create_job, create_retry_job, fail_job,
    get_job, heartbeat_job, is_unique_violation, list_jobs, request_cancel_job,
};
use std::collections::HashSet;

const TEST_LEASE: std::time::Duration = std::time::Duration::from_secs(60);

#[tokio::test]
async fn create_and_get_round_trip() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 5,
        },
    )
    .await
    .unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.priority, 5);
    assert_eq!(job.attempt_count, 0);
    assert!(job.started_at.is_none());
    let fetched = get_job(pool(&db), job.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, job.id);
}

#[tokio::test]
async fn create_job_rejects_missing_repository_fk() {
    let (_dir, db) = open_test_db().await;
    let err = create_job(
        pool(&db),
        NewJob {
            repository_id: uuid::Uuid::new_v4(),
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn claim_order_is_priority_then_oldest() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let low = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 1,
        },
    )
    .await
    .unwrap();
    let high = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::MirrorClone,
            priority: 10,
        },
    )
    .await
    .unwrap();
    let _low2 = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::WorkingClone,
            priority: 1,
        },
    )
    .await
    .unwrap();

    let first = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.id, high.id, "highest priority first");
    assert_eq!(first.status, JobStatus::Running);
    assert_eq!(first.attempt_count, 1);
    assert!(first.started_at.is_some());

    let second = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.id, low.id, "oldest of the remaining first");
}

#[tokio::test]
async fn claim_is_atomic_under_concurrency() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let mut created = Vec::new();
    for i in 0..3 {
        // One active job per (repository, job type) — the dedupe index
        // forbids three identical Fetch jobs for one repo, so each
        // job here targets its own repository.
        let repo = make_repository(
            pool(&db),
            account.id,
            &format!("456-{i}"),
            "octocat",
            &format!("hello-{i}"),
        )
        .await;
        created.push(
            create_job(
                pool(&db),
                NewJob {
                    repository_id: repo.id,
                    job_type: JobType::Fetch,
                    priority: 0,
                },
            )
            .await
            .unwrap(),
        );
    }
    let mut handles = Vec::new();
    for worker in 0..4 {
        let pool_clone = pool(&db).clone();
        handles.push(tokio::spawn(async move {
            claim_next_job(&pool_clone, &format!("worker-{worker}"), TEST_LEASE).await
        }));
    }
    let mut claimed: HashSet<uuid::Uuid> = HashSet::new();
    let mut none_count = 0;
    for handle in handles {
        match handle.await.unwrap().unwrap() {
            Some(job) => {
                assert!(claimed.insert(job.id), "same job claimed twice");
            }
            None => none_count += 1,
        }
    }
    assert_eq!(claimed.len(), 3, "each job claimed exactly once");
    assert_eq!(none_count, 1, "fourth claimer finds nothing pending");
    for created_job in &created {
        let job = get_job(pool(&db), created_job.id).await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.attempt_count, 1);
    }
}

#[tokio::test]
async fn invalid_job_transitions_rejected() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    // pending -> completed skips running.
    assert_eq!(
        complete_job(pool(&db), job.id).await.unwrap_err().code,
        ErrorCode::InvalidInput
    );
    // pending -> cancelled is fine; cancelled is terminal.
    let cancelled = cancel_job(pool(&db), job.id).await.unwrap();
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    assert!(cancelled.completed_at.is_some());
    assert!(
        claim_next_job(pool(&db), "test-worker", TEST_LEASE)
            .await
            .unwrap()
            .is_none(),
        "cancelled job is not claimable"
    );
    let job2 = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    let failed = fail_job(pool(&db), job2.id, "GitFetchFailed", "boom")
        .await
        .unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("GitFetchFailed"));
    assert_eq!(
        complete_job(pool(&db), job2.id).await.unwrap_err().code,
        ErrorCode::InvalidInput,
        "failed is terminal"
    );
}

#[tokio::test]
async fn complete_job_with_history_writes_both() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::MirrorClone,
            priority: 0,
        },
    )
    .await
    .unwrap();
    claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    let (finished, history) = complete_job_with_history(
        pool(&db),
        job.id,
        JobOutcome {
            result: SyncResult::Success,
            duration_ms: Some(1234),
            bytes_received: Some(5678),
            error_code: None,
            error_message: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(finished.status, JobStatus::Completed);
    assert!(finished.completed_at.is_some());
    assert_eq!(history.repository_id, repo.id);
    assert_eq!(history.job_id, Some(job.id));
    assert_eq!(history.operation, "mirror_clone");
    assert_eq!(history.result, SyncResult::Success);
    assert_eq!(history.duration_ms, Some(1234));
    assert_eq!(history.bytes_received, Some(5678));
    assert_eq!(
        count_history_for_repository(pool(&db), repo.id)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        list_history_for_job(pool(&db), job.id).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn complete_job_with_history_rejects_terminal_transition_without_writing() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    // Still pending: cannot complete.
    let err = complete_job_with_history(
        pool(&db),
        job.id,
        JobOutcome {
            result: SyncResult::Success,
            duration_ms: None,
            bytes_received: None,
            error_code: None,
            error_message: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(
        count_history_for_repository(pool(&db), repo.id)
            .await
            .unwrap(),
        0,
        "failed finish must not leave a history row"
    );
    let job = get_job(pool(&db), job.id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Pending);
}

#[tokio::test]
async fn list_and_count_jobs_by_status() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let j1 = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let _j2 = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::MirrorClone,
            priority: 0,
        },
    )
    .await
    .unwrap();
    cancel_job(pool(&db), j1.id).await.unwrap();

    let pending = list_jobs(
        pool(&db),
        JobFilter {
            status: Some(JobStatus::Pending),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(pending.len(), 1);
    let by_repo = list_jobs(
        pool(&db),
        JobFilter {
            repository_id: Some(repo.id),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_repo.len(), 2);

    let counts = count_jobs_by_status(pool(&db)).await.unwrap();
    let pending_count = counts
        .iter()
        .find(|c| c.status == JobStatus::Pending)
        .map(|c| c.count)
        .unwrap_or(0);
    let cancelled_count = counts
        .iter()
        .find(|c| c.status == JobStatus::Cancelled)
        .map(|c| c.count)
        .unwrap_or(0);
    assert_eq!(pending_count, 1);
    assert_eq!(cancelled_count, 1);
}

#[tokio::test]
async fn retry_row_is_not_claimable_before_available_at() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let claimed = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, job.id);

    // In the real flow the failed job is terminal before its retry
    // row is created (otherwise the dedupe index would reject it).
    let failed = fail_job(pool(&db), claimed.id, "NetworkUnavailable", "down")
        .await
        .unwrap();
    assert_eq!(failed.attempt_count, 1);

    // Retry row carries the attempt count and is not claimable
    // before its backoff elapses.
    let retry = create_retry_job(
        pool(&db),
        &failed,
        chrono::Utc::now() + chrono::Duration::seconds(60),
    )
    .await
    .unwrap();
    assert_ne!(retry.id, claimed.id);
    assert_eq!(retry.attempt_count, 1, "attempt count carries over");
    assert!(retry.available_at.is_some());
    assert!(
        claim_next_job(pool(&db), "test-worker", TEST_LEASE)
            .await
            .unwrap()
            .is_none(),
        "backoff must be enforced by the claim query"
    );

    // Once the backoff has passed, the retry is claimable.
    sqlx::query("UPDATE sync_jobs SET available_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00Z")
        .bind(retry.id.to_string())
        .execute(pool(&db))
        .await
        .unwrap();
    let claimed_retry = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed_retry.id, retry.id);
    assert_eq!(claimed_retry.attempt_count, 2);
}

#[tokio::test]
async fn duplicate_retry_row_is_a_unique_violation() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let claimed = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, job.id);
    let failed = fail_job(pool(&db), claimed.id, "NetworkUnavailable", "down")
        .await
        .unwrap();
    let _retry = create_retry_job(pool(&db), &failed, chrono::Utc::now())
        .await
        .unwrap();

    // A second retry row for the same parent collides on the dedupe
    // index while the first retry is still pending. The executor and
    // recovery rely on recognising this collision ("already queued").
    // Regression: `map_error` rewrites the message, so the check must
    // also look at `details`.
    let err = create_retry_job(pool(&db), &failed, chrono::Utc::now())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(
        is_unique_violation(&err),
        "a dedupe collision must be recognised as a unique violation; got message {:?} details {:?}",
        err.message,
        err.details
    );
}

#[tokio::test]
async fn priority_aging_promotes_old_jobs() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo_old = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let repo_new = make_repository(pool(&db), account.id, "457", "octocat", "world").await;
    // Old low-priority job (priority 0, created 4h ago).
    let old = create_job(
        pool(&db),
        NewJob {
            repository_id: repo_old.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let four_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(4))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    sqlx::query("UPDATE sync_jobs SET created_at = ? WHERE id = ?")
        .bind(&four_hours_ago)
        .bind(old.id.to_string())
        .execute(pool(&db))
        .await
        .unwrap();
    // Fresh higher-priority job (priority 2).
    let fresh = create_job(
        pool(&db),
        NewJob {
            repository_id: repo_new.id,
            job_type: JobType::Fetch,
            priority: 2,
        },
    )
    .await
    .unwrap();

    // Aging (+3 after 3h) lifts the old job to effective 3 > 2.
    let first = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        first.id, old.id,
        "aged job must win over fresh higher priority"
    );
    let second = claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.id, fresh.id);
}

#[tokio::test]
async fn request_cancel_pending_and_running() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let pending = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let cancelled = request_cancel_job(pool(&db), pending.id).await.unwrap();
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    assert!(cancelled.cancel_requested_at.is_some());

    let running = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::MirrorClone,
            priority: 0,
        },
    )
    .await
    .unwrap();
    claim_next_job(pool(&db), "test-worker", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    let still_running = request_cancel_job(pool(&db), running.id).await.unwrap();
    assert_eq!(still_running.status, JobStatus::Running);
    assert!(
        still_running.cancel_requested_at.is_some(),
        "running job records the request for cooperative cancel"
    );
}

#[tokio::test]
async fn heartbeat_renews_lease_only_for_owner() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let job = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let claimed = claim_next_job(pool(&db), "worker-1", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.worker_id.as_deref(), Some("worker-1"));
    assert!(claimed.lease_expires_at.is_some());
    assert!(claimed.heartbeat_at.is_some());

    // Another worker cannot renew someone else's lease.
    assert!(
        heartbeat_job(pool(&db), job.id, "worker-2", TEST_LEASE)
            .await
            .unwrap()
            .is_none()
    );
    // The owner can.
    let renewed = heartbeat_job(pool(&db), job.id, "worker-1", TEST_LEASE)
        .await
        .unwrap()
        .unwrap();
    assert!(renewed.lease_expires_at.unwrap() >= claimed.lease_expires_at.unwrap());
}
