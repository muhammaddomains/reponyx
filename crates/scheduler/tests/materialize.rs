//! Materialize integration tests (temp SQLite databases).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, make_schedule, open_test_db, pool, utc};
use reponyx_core::{IntervalType, JobStatus, JobType, ScheduleScope, SyncMode};
use reponyx_database::jobs::{JobFilter, complete_job, list_jobs};
use reponyx_database::schedules::{SchedulePatch, get_schedule, update_schedule};
use reponyx_scheduler::materialize_due;
use uuid::Uuid;

fn now() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 19, 10, 0)
}

#[tokio::test]
async fn materialize_creates_one_job_and_advances_the_schedule() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    let repo = make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let schedule = make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;

    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.checked, 1);
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].repository_id, repo.id);
    assert_eq!(report.created[0].job_type, JobType::MirrorClone);
    assert_eq!(report.created[0].status, JobStatus::Pending);
    assert_eq!(report.created[0].priority, 0);
    assert!(report.errors.is_empty());

    // Advanced to the next 6-hour slot after `now`.
    let advanced = get_schedule(pool(&db), schedule.id).await.unwrap().unwrap();
    assert_eq!(advanced.next_run_at, Some(utc(2026, 11, 19, 16, 0)));
}

#[tokio::test]
async fn second_materialize_creates_no_duplicate_while_job_pending() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    let repo = make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let first = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(first.created.len(), 1);

    // A second, repository-scoped due schedule for the same repo.
    make_schedule(
        pool(&db),
        ScheduleScope::Repository,
        Some(account.id),
        Some(repo.id),
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let second = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(second.checked, 1);
    assert_eq!(second.created.len(), 0);
    assert_eq!(second.skipped_duplicate, 1);
}

#[tokio::test]
async fn materialize_after_job_completes_creates_the_next_job() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    let repo = make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let schedule = make_schedule(
        pool(&db),
        ScheduleScope::Repository,
        Some(account.id),
        Some(repo.id),
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let first = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(first.created.len(), 1);
    // A pending job must be claimed (running) before it can complete.
    let claimed = reponyx_database::jobs::claim_next_job(
        pool(&db),
        "test-worker",
        std::time::Duration::from_secs(60),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(claimed.id, first.created[0].id);
    complete_job(pool(&db), first.created[0].id).await.unwrap();

    // Make the schedule due again.
    update_schedule(
        pool(&db),
        schedule.id,
        SchedulePatch {
            next_run_at: Some(Some(utc(2026, 11, 19, 4, 0))),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let second = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(second.created.len(), 1);
    assert_ne!(second.created[0].id, first.created[0].id);
    // Exactly two jobs exist for the repo; the first is completed.
    let jobs = list_jobs(
        pool(&db),
        JobFilter {
            repository_id: Some(repo.id),
            status: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(jobs.len(), 2);
}

#[tokio::test]
async fn sync_disabled_repository_is_skipped() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        false,
        SyncMode::Mirror,
        None,
    )
    .await;
    make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.created.len(), 0);
    assert_eq!(report.skipped_sync_disabled, 1);
}

#[tokio::test]
async fn account_scope_covers_only_its_repositories() {
    let (_dir, db) = open_test_db().await;
    let alice = make_account(pool(&db), "alice").await;
    let bob = make_account(pool(&db), "bob").await;
    let a1 = make_repository(
        pool(&db),
        alice.id,
        "prov-a1",
        "alice",
        "a1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let a2 = make_repository(
        pool(&db),
        alice.id,
        "prov-a2",
        "alice",
        "a2",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    make_repository(
        pool(&db),
        bob.id,
        "prov-b1",
        "bob",
        "b1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    make_schedule(
        pool(&db),
        ScheduleScope::Account,
        Some(alice.id),
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.created.len(), 2);
    let mut ids: Vec<Uuid> = report.created.iter().map(|j| j.repository_id).collect();
    ids.sort();
    let mut expected = vec![a1.id, a2.id];
    expected.sort();
    assert_eq!(ids, expected);
}

#[tokio::test]
async fn job_type_depends_on_local_path_and_sync_mode() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    let mirror = make_repository(
        pool(&db),
        account.id,
        "prov-m",
        "alice",
        "m",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let working = make_repository(
        pool(&db),
        account.id,
        "prov-w",
        "alice",
        "w",
        true,
        SyncMode::Working,
        None,
    )
    .await;
    let existing = make_repository(
        pool(&db),
        account.id,
        "prov-e",
        "alice",
        "e",
        true,
        SyncMode::Mirror,
        Some("/data/e.git".to_string()),
    )
    .await;
    make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.created.len(), 3);
    let by_repo = |id: Uuid| {
        report
            .created
            .iter()
            .find(|j| j.repository_id == id)
            .unwrap()
            .job_type
    };
    assert_eq!(by_repo(mirror.id), JobType::MirrorClone);
    assert_eq!(by_repo(working.id), JobType::WorkingClone);
    assert_eq!(by_repo(existing.id), JobType::Fetch);
}

#[tokio::test]
async fn priming_sets_next_run_for_schedules_without_one() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let schedule = make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::Daily,
        Some("02:30".to_string()),
        "UTC",
        true,
        None,
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.primed, 1);
    assert_eq!(report.created.len(), 0);
    let primed = get_schedule(pool(&db), schedule.id).await.unwrap().unwrap();
    assert!(primed.next_run_at.is_some());
}

#[tokio::test]
async fn out_of_window_run_is_skipped_and_advanced() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    // Daily 02:30, but the window is 03:00-05:00: the stored run
    // (yesterday 02:30) is outside the window.
    let schedule = make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::Daily,
        Some(r#"{"time": "02:30", "window": {"start": "03:00", "end": "05:00"}}"#.to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 18, 2, 30)),
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.checked, 1);
    assert_eq!(report.created.len(), 0);
    assert_eq!(report.skipped_out_of_window, 1);
    // The daily time is outside its window every day, so there is no
    // future run: next_run_at becomes None.
    let advanced = get_schedule(pool(&db), schedule.id).await.unwrap().unwrap();
    assert_eq!(advanced.next_run_at, None);
}

#[tokio::test]
async fn invalid_cadence_is_reported_and_not_advanced() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let schedule = make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::Daily,
        Some("not-a-time".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.checked, 1);
    assert_eq!(report.created.len(), 0);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].schedule_id, schedule.id);
    // Left un-advanced so the broken schedule stays visible.
    let unchanged = get_schedule(pool(&db), schedule.id).await.unwrap().unwrap();
    assert_eq!(unchanged.next_run_at, Some(utc(2026, 11, 19, 4, 0)));
}

#[tokio::test]
async fn disabled_schedule_never_materializes() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "alice").await;
    make_repository(
        pool(&db),
        account.id,
        "prov-r1",
        "alice",
        "r1",
        true,
        SyncMode::Mirror,
        None,
    )
    .await;
    let schedule = make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        false,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    let report = materialize_due(pool(&db), now()).await.unwrap();
    assert_eq!(report.checked, 0);
    assert_eq!(report.created.len(), 0);
    // The disabled schedule's next_run_at is untouched by the run.
    let unchanged = get_schedule(pool(&db), schedule.id).await.unwrap().unwrap();
    assert_eq!(unchanged.next_run_at, Some(utc(2026, 11, 19, 4, 0)));
}
