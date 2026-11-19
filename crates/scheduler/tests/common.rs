#![allow(dead_code)]
//! Shared helpers for scheduler integration tests: temp databases and
//! fixture builders. Every test opens its own temp SQLite file so WAL
//! mode is real.

use chrono::{DateTime, TimeZone, Utc};
use reponyx_core::{
    Account, AccountType, AuthenticationMethod, IntervalType, Provider, Repository, Schedule,
    ScheduleScope, SyncMode, Visibility,
};
use reponyx_database::Database;
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::repositories::{NewRepository, create_repository};
use reponyx_database::schedules::{NewSchedule, create_schedule};
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;

pub async fn open_test_db() -> (TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = reponyx_database::open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    (dir, db)
}

pub fn pool(db: &Database) -> &SqlitePool {
    &db.pool
}

/// A UTC instant built from parts (no DST involved — UTC has none).
pub fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

pub async fn make_account(pool: &SqlitePool, username: &str) -> Account {
    create_account(
        pool,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: format!("prov-{username}"),
            username: username.to_string(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
pub async fn make_repository(
    pool: &SqlitePool,
    account_id: Uuid,
    provider_repository_id: &str,
    owner: &str,
    name: &str,
    sync_enabled: bool,
    sync_mode: SyncMode,
    local_path: Option<String>,
) -> Repository {
    create_repository(
        pool,
        NewRepository {
            account_id,
            provider_repository_id: provider_repository_id.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            visibility: Visibility::Private,
            default_branch: Some("main".to_string()),
            clone_url: format!("https://github.com/{owner}/{name}.git"),
            sync_enabled,
            sync_mode,
            local_path,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
pub async fn make_schedule(
    pool: &SqlitePool,
    scope: ScheduleScope,
    account_id: Option<Uuid>,
    repository_id: Option<Uuid>,
    interval_type: IntervalType,
    interval_value: Option<String>,
    timezone: &str,
    enabled: bool,
    next_run_at: Option<DateTime<Utc>>,
) -> Schedule {
    create_schedule(
        pool,
        NewSchedule {
            scope,
            account_id,
            repository_id,
            interval_type,
            interval_value,
            timezone: timezone.to_string(),
            enabled,
            next_run_at,
        },
    )
    .await
    .unwrap()
}

/// A `Schedule` struct built in memory (no database) for pure
/// next-run math tests.
pub fn schedule_struct(
    interval_type: IntervalType,
    interval_value: Option<&str>,
    timezone: &str,
    enabled: bool,
    next_run_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
) -> Schedule {
    Schedule {
        id: Uuid::new_v4(),
        scope: ScheduleScope::Global,
        account_id: None,
        repository_id: None,
        interval_type,
        interval_value: interval_value.map(str::to_string),
        timezone: timezone.to_string(),
        enabled,
        next_run_at,
        created_at,
        updated_at: created_at,
    }
}
