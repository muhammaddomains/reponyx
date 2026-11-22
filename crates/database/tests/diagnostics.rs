//! Diagnostics counts + database backup/validate tests (Phase 9).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, open_test_db, pool};
use reponyx_database::diagnostics::{
    backup_database, diagnostics_counts, is_safe_for_diagnostics, validate_backup_file,
};

#[tokio::test]
async fn counts_are_zero_on_a_fresh_database() {
    let (_dir, db) = open_test_db().await;
    let counts = diagnostics_counts(pool(&db)).await.unwrap();
    assert_eq!(counts.accounts, 0);
    assert_eq!(counts.repositories, 0);
    assert_eq!(counts.schedules, 0);
    assert_eq!(counts.history_rows, 0);
    assert_eq!(counts.failed_history_7d, 0);
}

#[tokio::test]
async fn counts_reflect_seeded_rows() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    make_repository(pool(&db), account.id, "1", "octocat", "hello").await;
    make_repository(pool(&db), account.id, "2", "octocat", "world").await;
    let counts = diagnostics_counts(pool(&db)).await.unwrap();
    assert_eq!(counts.accounts, 1);
    assert_eq!(counts.repositories, 2);
    assert_eq!(counts.synced, 0);
}

#[tokio::test]
async fn backup_round_trip_keeps_rows() {
    let (dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    make_repository(pool(&db), account.id, "1", "octocat", "hello").await;
    let dest = dir.path().join("backup.sqlite");
    let bytes = backup_database(pool(&db), &dest).await.unwrap();
    assert!(bytes > 0);
    assert!(dest.exists());
    // The backup is a standalone database with the same rows.
    validate_backup_file(&dest).await.unwrap();
    let backup_pool =
        sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", dest.to_string_lossy()))
            .await
            .unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repositories")
        .fetch_one(&backup_pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    let settings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
        .fetch_one(&backup_pool)
        .await
        .unwrap();
    assert!(settings >= 4, "seeded settings survive the backup");
    backup_pool.close().await;
    // The live database is untouched by the backup.
    let counts = diagnostics_counts(pool(&db)).await.unwrap();
    assert_eq!(counts.repositories, 1);
}

#[tokio::test]
async fn backup_refuses_existing_destination() {
    let (dir, db) = open_test_db().await;
    let dest = dir.path().join("backup.sqlite");
    backup_database(pool(&db), &dest).await.unwrap();
    let err = backup_database(pool(&db), &dest).await.unwrap_err();
    assert_eq!(err.code, reponyx_core::ErrorCode::InvalidInput);
}

#[tokio::test]
async fn validate_rejects_garbage_and_truncated_files() {
    let dir = tempfile::tempdir().unwrap();
    let garbage = dir.path().join("garbage.sqlite");
    std::fs::write(&garbage, b"this is not a database at all").unwrap();
    assert!(validate_backup_file(&garbage).await.is_err());

    // Header only: the magic matches, but there is no database behind it.
    let (_db_dir, db) = open_test_db().await;
    let dest = dir.path().join("backup.sqlite");
    backup_database(pool(&db), &dest).await.unwrap();
    let bytes = std::fs::read(&dest).unwrap();
    let truncated = dir.path().join("truncated.sqlite");
    std::fs::write(&truncated, &bytes[..100]).unwrap();
    assert!(validate_backup_file(&truncated).await.is_err());
}

#[tokio::test]
async fn validate_rejects_other_sqlite_files() {
    // A valid SQLite file that is not a RepoNyx database: the header
    // and integrity check pass, but there is no settings table.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.sqlite");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
        .await
        .unwrap();
    sqlx::query("CREATE TABLE t (x INTEGER)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let err = validate_backup_file(&path).await.unwrap_err();
    assert_eq!(err.code, reponyx_core::ErrorCode::InvalidInput);
    assert!(err.message.contains("settings"));
}

#[test]
fn settings_allowlist_is_exact() {
    for key in [
        "ui.theme",
        "sync.default_mode",
        "sync.concurrency",
        "storage.root",
    ] {
        assert!(is_safe_for_diagnostics(key), "{key} should be safe");
    }
    for key in [
        "device.identifier",
        "device_identifier",
        "broker.setup_token",
        "storage.root.evil",
        "ui.theme ",
        "",
    ] {
        assert!(!is_safe_for_diagnostics(key), "{key} should be excluded");
    }
}
