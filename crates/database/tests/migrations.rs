//! Integration tests: migrations apply cleanly and counts work on a
//! fresh database. Uses a temp file (not in-memory) so WAL mode is real.

use reponyx_database::open_database;
use sqlx::Row;
use tempfile::tempdir;

#[tokio::test]
async fn migrations_create_all_tables() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("reponyx.sqlite");
    let db = open_database(&db_path).await.unwrap();

    let tables: Vec<String> =
        sqlx::query("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&db.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();

    for expected in [
        "accounts",
        "installations",
        "repositories",
        "schedules",
        "sync_jobs",
        "sync_history",
        "settings",
        "devices",
        "audit_logs",
    ] {
        assert!(
            tables.contains(&expected.to_string()),
            "missing table {expected}"
        );
    }
}

#[tokio::test]
async fn fresh_database_counts_are_zero() {
    let dir = tempdir().unwrap();
    let db = open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    let counts = reponyx_database::dashboard_counts(&db.pool).await.unwrap();
    assert_eq!(counts.accounts, 0);
    assert_eq!(counts.repositories, 0);
    assert_eq!(counts.pending_jobs, 0);
}

#[tokio::test]
async fn accounts_and_repositories_round_trip() {
    let dir = tempdir().unwrap();
    let db = open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO accounts (id, provider, provider_account_id, username, authentication_method, created_at, updated_at)
         VALUES ('acc-1', 'github', '123', 'octocat', 'fine_grained_pat', datetime('now'), datetime('now'))",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, archive_status, created_at, updated_at)
         VALUES ('repo-1', 'acc-1', '456', 'octocat', 'hello', 'octocat/hello', 'https://github.com/octocat/hello.git', 'synced', datetime('now'), datetime('now'))",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let counts = reponyx_database::dashboard_counts(&db.pool).await.unwrap();
    assert_eq!(counts.accounts, 1);
    assert_eq!(counts.repositories, 1);
    assert_eq!(counts.synced, 1);

    // Foreign keys: a repository referencing a missing account must fail.
    let bad = sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, created_at, updated_at)
         VALUES ('repo-2', 'no-such-account', '789', 'o', 'n', 'o/n', 'https://example.invalid/x.git', datetime('now'), datetime('now'))",
    )
    .execute(&db.pool)
    .await;
    assert!(bad.is_err(), "FK violation should be rejected");
}
