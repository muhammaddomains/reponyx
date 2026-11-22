//! Config import/export integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use chrono::{DateTime, Utc};
use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::{
    ErrorCode, IntervalType, Provider, RepoNyxError, Schedule, ScheduleScope, SyncMode,
};
use reponyx_database::config::{
    ConfigFile, ConfigRepository, ConfigSchedule, ConfigSetting, apply_config_import,
    build_config_export, parse_config_file,
};
use reponyx_database::schedules::{NewSchedule, create_schedule, list_schedules};
use reponyx_database::settings::{get_setting, list_settings, set_setting};

/// The `next_run` stub for tests: accepts any cadence, never
/// schedules a run. The real computation lives in
/// `reponyx-scheduler` and is passed in by the desktop.
fn no_next_run(
    _schedule: &Schedule,
    _from: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, RepoNyxError> {
    Ok(None)
}

fn failing_next_run(
    _schedule: &Schedule,
    _from: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, RepoNyxError> {
    Err(RepoNyxError::new(ErrorCode::InvalidInput, "bad cadence"))
}

async fn seed_settings(db_pool: &sqlx::SqlitePool) {
    for (key, value) in [
        ("ui.theme", "dark"),
        ("sync.default_mode", "both"),
        ("sync.concurrency", "3"),
        ("storage.root", "/tmp/reponyx-test-root"),
    ] {
        set_setting(db_pool, key, value).await.unwrap();
    }
}

async fn seed_global_schedule(db_pool: &sqlx::SqlitePool) {
    create_schedule(
        db_pool,
        NewSchedule {
            scope: ScheduleScope::Global,
            account_id: None,
            repository_id: None,
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("6".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
            next_run_at: None,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn export_parse_round_trip() {
    let (_dir, db) = open_test_db().await;
    let db_pool = pool(&db);
    let account = make_account(db_pool, "alice").await;
    let _repo = make_repository(db_pool, account.id, "100", "alice", "demo").await;
    seed_settings(db_pool).await;
    seed_global_schedule(db_pool).await;

    let exported = build_config_export(db_pool, "0.1.0").await.unwrap();
    assert_eq!(exported.format, "reponyx-config");
    assert_eq!(exported.version, 1);
    assert_eq!(exported.settings.len(), 4);
    assert_eq!(exported.repositories.len(), 1);
    assert_eq!(exported.schedules.len(), 1);

    let text = serde_json::to_string(&exported).unwrap();
    let parsed = parse_config_file(&text).unwrap();
    assert_eq!(parsed.repositories[0].provider_repository_id, "100");
    assert_eq!(parsed.repositories[0].account_username, "alice");
    assert_eq!(parsed.repositories[0].provider_account_id, "prov-alice");
    assert_eq!(parsed.schedules[0].scope, ScheduleScope::Global);
    assert_eq!(
        parsed
            .settings
            .iter()
            .find(|s| s.key == "ui.theme")
            .unwrap()
            .value,
        "dark"
    );
}

#[tokio::test]
async fn apply_to_fresh_db() {
    let (_dir_a, db_a) = open_test_db().await;
    let pool_a = pool(&db_a);
    let account_a = make_account(pool_a, "alice").await;
    let repo_a = make_repository(pool_a, account_a.id, "100", "alice", "demo").await;
    // Give the source repo a distinctive selection so the test
    // proves the import actually applies it (the fixture default
    // is Mirror + enabled).
    reponyx_database::repositories::set_sync_mode(pool_a, repo_a.id, SyncMode::Both)
        .await
        .unwrap();
    reponyx_database::repositories::set_sync_enabled(pool_a, repo_a.id, false)
        .await
        .unwrap();
    seed_settings(pool_a).await;
    seed_global_schedule(pool_a).await;
    let exported = build_config_export(pool_a, "0.1.0").await.unwrap();
    let text = serde_json::to_string(&exported).unwrap();

    // A second machine: the same account + repo (same provider
    // ids), different local state — sync disabled, mirror mode,
    // default settings.
    let (_dir_b, db_b) = open_test_db().await;
    let pool_b = pool(&db_b);
    let account_b = make_account(pool_b, "alice").await;
    let repo_b = make_repository(pool_b, account_b.id, "100", "alice", "demo").await;
    assert_ne!(repo_b.sync_mode, SyncMode::Both);

    let file = parse_config_file(&text).unwrap();
    let report = apply_config_import(pool_b, &file, no_next_run)
        .await
        .unwrap();
    assert_eq!(report.settings_applied, 4);
    assert_eq!(report.repositories_applied, 1);
    assert_eq!(report.schedules_created, 1);
    assert_eq!(report.schedules_updated, 0);
    assert!(report.skipped.is_empty());

    let theme = get_setting(pool_b, "ui.theme").await.unwrap().unwrap();
    assert_eq!(theme.value, "dark");
    let mode = get_setting(pool_b, "sync.default_mode")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode.value, "both");
    let repo = reponyx_database::repositories::get_repository(pool_b, repo_b.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repo.sync_mode, SyncMode::Both);
    assert!(!repo.sync_enabled);
    let schedules = list_schedules(pool_b).await.unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].interval_value.as_deref(), Some("6"));
}

#[tokio::test]
async fn apply_skips_unknown_repo_and_schedule() {
    let (_dir, db) = open_test_db().await;
    let db_pool = pool(&db);
    let file = ConfigFile {
        format: "reponyx-config".to_string(),
        version: 1,
        app_version: "0.1.0".to_string(),
        exported_at: Utc::now().to_rfc3339(),
        settings: vec![ConfigSetting {
            key: "ui.theme".to_string(),
            value: "light".to_string(),
        }],
        repositories: vec![ConfigRepository {
            provider: Provider::GitHub,
            provider_repository_id: "999".to_string(),
            provider_account_id: "prov-ghost".to_string(),
            account_username: "ghost".to_string(),
            owner: "ghost".to_string(),
            name: "missing".to_string(),
            full_name: "ghost/missing".to_string(),
            clone_url: "https://github.com/ghost/missing.git".to_string(),
            sync_mode: SyncMode::Mirror,
            sync_enabled: true,
        }],
        schedules: vec![ConfigSchedule {
            scope: ScheduleScope::Account,
            provider: Some(Provider::GitHub),
            provider_account_id: Some("prov-ghost".to_string()),
            account_username: Some("ghost".to_string()),
            provider_repository_id: None,
            repository_full_name: None,
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("6".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
        }],
    };
    let report = apply_config_import(db_pool, &file, no_next_run)
        .await
        .unwrap();
    assert_eq!(report.settings_applied, 1);
    assert_eq!(report.repositories_applied, 0);
    assert_eq!(report.schedules_created, 0);
    assert_eq!(report.skipped.len(), 2);
    assert!(report.skipped.iter().any(|s| s.kind == "repository"));
    assert!(report.skipped.iter().any(|s| s.kind == "schedule"));
    // The setting still applied — skips are not failures.
    let theme = get_setting(db_pool, "ui.theme").await.unwrap().unwrap();
    assert_eq!(theme.value, "light");
}

#[tokio::test]
async fn parse_rejects_wrong_format_and_version() {
    let base = r#"{"format":"%FORMAT%","version":%VERSION%,"app_version":"0.1.0","exported_at":"2026-11-22T00:00:00Z","settings":[],"repositories":[],"schedules":[]}"#;
    let wrong_format = base
        .replace("%FORMAT%", "something-else")
        .replace("%VERSION%", "1");
    assert!(parse_config_file(&wrong_format).is_err());
    let wrong_version = base
        .replace("%FORMAT%", "reponyx-config")
        .replace("%VERSION%", "2");
    assert!(parse_config_file(&wrong_version).is_err());
    assert!(parse_config_file("not json at all").is_err());
}

#[tokio::test]
async fn parse_rejects_bad_settings() {
    let file = |settings: &str| {
        format!(
            r#"{{"format":"reponyx-config","version":1,"app_version":"0.1.0","exported_at":"2026-11-22T00:00:00Z","settings":[{{{settings}}}],"repositories":[],"schedules":[]}}"#
        )
    };
    // Unknown key.
    assert!(parse_config_file(&file(r#""key":"some.secret","value":"x""#)).is_err());
    // Bad theme value.
    assert!(parse_config_file(&file(r#""key":"ui.theme","value":"purple""#)).is_err());
    // Concurrency out of range.
    assert!(parse_config_file(&file(r#""key":"sync.concurrency","value":"99""#)).is_err());
    // A valid setting parses.
    assert!(parse_config_file(&file(r#""key":"sync.concurrency","value":"5""#)).is_ok());
}

#[tokio::test]
async fn apply_refuses_bad_cadence_without_writing() {
    let (_dir, db) = open_test_db().await;
    let db_pool = pool(&db);
    let before = get_setting(db_pool, "ui.theme").await.unwrap().unwrap();
    assert_ne!(before.value, "dark");

    let file = ConfigFile {
        format: "reponyx-config".to_string(),
        version: 1,
        app_version: "0.1.0".to_string(),
        exported_at: Utc::now().to_rfc3339(),
        settings: vec![ConfigSetting {
            key: "ui.theme".to_string(),
            value: "dark".to_string(),
        }],
        repositories: vec![],
        schedules: vec![ConfigSchedule {
            scope: ScheduleScope::Global,
            provider: None,
            provider_account_id: None,
            account_username: None,
            provider_repository_id: None,
            repository_full_name: None,
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("not-a-number".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
        }],
    };
    let result = apply_config_import(db_pool, &file, failing_next_run).await;
    assert!(result.is_err());
    // Nothing was written — not even the valid setting.
    let after = get_setting(db_pool, "ui.theme").await.unwrap().unwrap();
    assert_eq!(after.value, before.value);
    assert!(list_schedules(db_pool).await.unwrap().is_empty());
    assert!(list_settings(db_pool).await.unwrap().len() >= 4);
}

#[tokio::test]
async fn apply_updates_existing_schedule() {
    let (_dir, db) = open_test_db().await;
    let db_pool = pool(&db);
    seed_global_schedule(db_pool).await;

    let file = ConfigFile {
        format: "reponyx-config".to_string(),
        version: 1,
        app_version: "0.1.0".to_string(),
        exported_at: Utc::now().to_rfc3339(),
        settings: vec![],
        repositories: vec![],
        schedules: vec![ConfigSchedule {
            scope: ScheduleScope::Global,
            provider: None,
            provider_account_id: None,
            account_username: None,
            provider_repository_id: None,
            repository_full_name: None,
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("12".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
        }],
    };
    let report = apply_config_import(db_pool, &file, no_next_run)
        .await
        .unwrap();
    assert_eq!(report.schedules_created, 0);
    assert_eq!(report.schedules_updated, 1);
    let schedules = list_schedules(db_pool).await.unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].interval_value.as_deref(), Some("12"));
}
