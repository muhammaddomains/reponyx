//! Schedule repository integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use chrono::{Duration, Utc};
use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::{ErrorCode, IntervalType, ScheduleScope};
use reponyx_database::schedules::{
    NewSchedule, SchedulePatch, create_schedule, delete_schedule, due_schedules,
    effective_schedule_for, get_schedule, update_schedule,
};
use uuid::Uuid;

fn global_schedule() -> NewSchedule {
    NewSchedule {
        scope: ScheduleScope::Global,
        account_id: None,
        repository_id: None,
        interval_type: IntervalType::IntervalHours,
        interval_value: Some("6".to_string()),
        timezone: "UTC".to_string(),
        enabled: true,
        next_run_at: Some(Utc::now() + Duration::hours(6)),
    }
}

#[tokio::test]
async fn create_and_get_round_trip() {
    let (_dir, db) = open_test_db().await;
    let created = create_schedule(pool(&db), global_schedule()).await.unwrap();
    assert_eq!(created.scope, ScheduleScope::Global);
    assert_eq!(created.interval_type, IntervalType::IntervalHours);
    assert_eq!(created.interval_value.as_deref(), Some("6"));
    assert_eq!(created.timezone, "UTC");
    assert!(created.enabled);
    let fetched = get_schedule(pool(&db), created.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, created.id);
    assert!(
        get_schedule(pool(&db), Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn scope_validation_rejects_inconsistent_ids() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;

    let mut global_with_account = global_schedule();
    global_with_account.account_id = Some(account.id);
    assert_eq!(
        create_schedule(pool(&db), global_with_account)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );

    let mut account_without_id = global_schedule();
    account_without_id.scope = ScheduleScope::Account;
    assert_eq!(
        create_schedule(pool(&db), account_without_id)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );

    let mut repo_without_id = global_schedule();
    repo_without_id.scope = ScheduleScope::Repository;
    assert_eq!(
        create_schedule(pool(&db), repo_without_id)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );

    let mut empty_timezone = global_schedule();
    empty_timezone.timezone = "  ".to_string();
    assert_eq!(
        create_schedule(pool(&db), empty_timezone)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn schedule_for_unknown_repository_fk_rejected() {
    let (_dir, db) = open_test_db().await;
    let input = NewSchedule {
        scope: ScheduleScope::Repository,
        account_id: None,
        repository_id: Some(Uuid::new_v4()),
        interval_type: IntervalType::Daily,
        interval_value: Some("09:00".to_string()),
        timezone: "UTC".to_string(),
        enabled: true,
        next_run_at: None,
    };
    assert_eq!(
        create_schedule(pool(&db), input).await.unwrap_err().code,
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn effective_schedule_precedence_repository_over_account_over_global() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;

    // No schedules at all.
    assert!(
        effective_schedule_for(pool(&db), repo.id)
            .await
            .unwrap()
            .is_none()
    );

    let global = create_schedule(pool(&db), global_schedule()).await.unwrap();
    let effective = effective_schedule_for(pool(&db), repo.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.id, global.id);

    let account_schedule = create_schedule(
        pool(&db),
        NewSchedule {
            scope: ScheduleScope::Account,
            account_id: Some(account.id),
            repository_id: None,
            interval_type: IntervalType::Daily,
            interval_value: Some("09:00".to_string()),
            timezone: "America/Phoenix".to_string(),
            enabled: true,
            next_run_at: None,
        },
    )
    .await
    .unwrap();
    let effective = effective_schedule_for(pool(&db), repo.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.id, account_schedule.id, "account beats global");

    let repo_schedule = create_schedule(
        pool(&db),
        NewSchedule {
            scope: ScheduleScope::Repository,
            account_id: Some(account.id),
            repository_id: Some(repo.id),
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("2".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
            next_run_at: None,
        },
    )
    .await
    .unwrap();
    let effective = effective_schedule_for(pool(&db), repo.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.id, repo_schedule.id, "repository beats account");

    // Removing the repository schedule falls back to the account one.
    delete_schedule(pool(&db), repo_schedule.id).await.unwrap();
    let effective = effective_schedule_for(pool(&db), repo.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.id, account_schedule.id);

    // Removing the account schedule falls back to global.
    delete_schedule(pool(&db), account_schedule.id)
        .await
        .unwrap();
    let effective = effective_schedule_for(pool(&db), repo.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.id, global.id);

    // Unknown repository id is an error, not a silent None.
    assert_eq!(
        effective_schedule_for(pool(&db), Uuid::new_v4())
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn account_schedule_does_not_leak_to_other_accounts() {
    let (_dir, db) = open_test_db().await;
    let a = make_account(pool(&db), "alice").await;
    let b = make_account(pool(&db), "bob").await;
    let repo_b = make_repository(pool(&db), b.id, "9", "bob", "repo").await;
    create_schedule(
        pool(&db),
        NewSchedule {
            scope: ScheduleScope::Account,
            account_id: Some(a.id),
            repository_id: None,
            interval_type: IntervalType::Daily,
            interval_value: Some("09:00".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
            next_run_at: None,
        },
    )
    .await
    .unwrap();
    assert!(
        effective_schedule_for(pool(&db), repo_b.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn due_schedules_only_enabled_and_overdue() {
    let (_dir, db) = open_test_db().await;
    let now = Utc::now();
    let overdue = create_schedule(
        pool(&db),
        NewSchedule {
            next_run_at: Some(now - Duration::hours(1)),
            ..global_schedule()
        },
    )
    .await
    .unwrap();
    let _future = create_schedule(
        pool(&db),
        NewSchedule {
            next_run_at: Some(now + Duration::hours(1)),
            ..global_schedule()
        },
    )
    .await
    .unwrap();
    let _disabled_overdue = create_schedule(
        pool(&db),
        NewSchedule {
            enabled: false,
            next_run_at: Some(now - Duration::hours(2)),
            ..global_schedule()
        },
    )
    .await
    .unwrap();
    let _no_next_run = create_schedule(
        pool(&db),
        NewSchedule {
            next_run_at: None,
            ..global_schedule()
        },
    )
    .await
    .unwrap();
    let earlier_overdue = create_schedule(
        pool(&db),
        NewSchedule {
            next_run_at: Some(now - Duration::hours(3)),
            ..global_schedule()
        },
    )
    .await
    .unwrap();

    let due = due_schedules(pool(&db), now).await.unwrap();
    assert_eq!(due.len(), 2, "only enabled + overdue schedules are due");
    assert_eq!(due[0].id, earlier_overdue.id, "oldest first");
    assert_eq!(due[1].id, overdue.id);
}

#[tokio::test]
async fn update_and_delete_schedule() {
    let (_dir, db) = open_test_db().await;
    let created = create_schedule(pool(&db), global_schedule()).await.unwrap();
    let updated = update_schedule(
        pool(&db),
        created.id,
        SchedulePatch {
            interval_value: Some(Some("12".to_string())),
            enabled: Some(false),
            next_run_at: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.interval_value.as_deref(), Some("12"));
    assert!(!updated.enabled);
    assert!(updated.next_run_at.is_none());
    assert!(delete_schedule(pool(&db), created.id).await.unwrap());
    assert!(get_schedule(pool(&db), created.id).await.unwrap().is_none());
    assert!(!delete_schedule(pool(&db), created.id).await.unwrap());
}
