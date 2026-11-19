//! Job + history repository integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::{ErrorCode, JobStatus, JobType, SyncResult};
use reponyx_database::history::{count_history_for_repository, list_history_for_job};
use reponyx_database::jobs::{
    JobFilter, JobOutcome, NewJob, cancel_job, claim_next_job, complete_job,
    complete_job_with_history, count_jobs_by_status, create_job, fail_job, get_job, list_jobs,
};
use std::collections::HashSet;

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
            job_type: JobType::Fetch,
            priority: 10,
        },
    )
    .await
    .unwrap();
    let _low2 = create_job(
        pool(&db),
        NewJob {
            repository_id: repo.id,
            job_type: JobType::Fetch,
            priority: 1,
        },
    )
    .await
    .unwrap();

    let first = claim_next_job(pool(&db)).await.unwrap().unwrap();
    assert_eq!(first.id, high.id, "highest priority first");
    assert_eq!(first.status, JobStatus::Running);
    assert_eq!(first.attempt_count, 1);
    assert!(first.started_at.is_some());

    let second = claim_next_job(pool(&db)).await.unwrap().unwrap();
    assert_eq!(second.id, low.id, "oldest of the remaining first");
}

#[tokio::test]
async fn claim_is_atomic_under_concurrency() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let mut created = Vec::new();
    for _ in 0..3 {
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
    for _ in 0..4 {
        let pool_clone = pool(&db).clone();
        handles.push(tokio::spawn(
            async move { claim_next_job(&pool_clone).await },
        ));
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
        claim_next_job(pool(&db)).await.unwrap().is_none(),
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
    claim_next_job(pool(&db)).await.unwrap().unwrap();
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
    claim_next_job(pool(&db)).await.unwrap().unwrap();
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
            job_type: JobType::Fetch,
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
