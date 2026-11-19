//! Precedence tests: the scheduler resolves the effective schedule
//! for a repository through Phase 2 (`effective_schedule_for` —
//! repository > account > global) and computes next runs from it.

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, make_schedule, open_test_db, pool, utc};
use reponyx_core::{ErrorCode, IntervalType, ScheduleScope, SyncMode};
use reponyx_database::schedules::delete_schedule;
use reponyx_scheduler::effective_next_runs;
use uuid::Uuid;

fn now() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 19, 10, 0)
}

#[tokio::test]
async fn repository_beats_account_beats_global() {
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
    // Global: daily 02:30 UTC.
    make_schedule(
        pool(&db),
        ScheduleScope::Global,
        None,
        None,
        IntervalType::Daily,
        Some("02:30".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 20, 2, 30)),
    )
    .await;
    // Account: every 6 h from 04:00.
    let account_schedule = make_schedule(
        pool(&db),
        ScheduleScope::Account,
        Some(account.id),
        None,
        IntervalType::IntervalHours,
        Some("6".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
    )
    .await;
    // Repository: Mondays 09:00 (2026-11-19 is a Thursday).
    let repo_schedule = make_schedule(
        pool(&db),
        ScheduleScope::Repository,
        Some(account.id),
        Some(repo.id),
        IntervalType::Weekly,
        Some("1 09:00".to_string()),
        "UTC",
        true,
        Some(utc(2026, 11, 23, 9, 0)),
    )
    .await;

    // Repository schedule wins.
    let effective = effective_next_runs(pool(&db), repo.id, now(), 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.schedule.id, repo_schedule.id);
    assert_eq!(effective.runs, vec![utc(2026, 11, 23, 9, 0)]);

    // Without it, the account schedule wins.
    delete_schedule(pool(&db), repo_schedule.id).await.unwrap();
    let effective = effective_next_runs(pool(&db), repo.id, now(), 2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.schedule.id, account_schedule.id);
    assert_eq!(
        effective.runs,
        vec![utc(2026, 11, 19, 16, 0), utc(2026, 11, 19, 22, 0)]
    );

    // Without it, the global schedule wins.
    delete_schedule(pool(&db), account_schedule.id)
        .await
        .unwrap();
    let effective = effective_next_runs(pool(&db), repo.id, now(), 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.schedule.interval_type, IntervalType::Daily);
    assert_eq!(effective.runs, vec![utc(2026, 11, 20, 2, 30)]);
}

#[tokio::test]
async fn no_schedule_means_none_and_unknown_repo_is_invalid() {
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
    assert!(
        effective_next_runs(pool(&db), repo.id, now(), 1)
            .await
            .unwrap()
            .is_none()
    );
    let err = effective_next_runs(pool(&db), Uuid::new_v4(), now(), 1)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn disabled_effective_schedule_has_no_runs() {
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
        IntervalType::Daily,
        Some("02:30".to_string()),
        "UTC",
        false,
        Some(utc(2026, 11, 20, 2, 30)),
    )
    .await;
    let effective = effective_next_runs(pool(&db), repo.id, now(), 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.schedule.id, schedule.id);
    assert!(effective.runs.is_empty());
}
