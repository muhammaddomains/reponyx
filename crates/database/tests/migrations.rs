//! Integration tests: migrations apply cleanly and counts work on a
//! fresh database. Uses a temp file (not in-memory) so WAL mode is real.
//!
//! The upgrade tests below are the Phase 10 migration coverage: a
//! database created at V1 (or V3) must upgrade to the latest schema
//! with its data intact, because users upgrade the app in place and
//! never rebuild their database by hand.

use reponyx_core::JobType;
use reponyx_database::jobs::{NewJob, create_job};
use reponyx_database::open_database;
use reponyx_database::repositories::{RepositoryFilter, list_repositories};
use sqlx::Row;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;
use tempfile::tempdir;
use uuid::Uuid;

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

/// Apply only the named migration files, via the runtime Migrator
/// (the compile-time `migrate!` macro always applies everything).
/// The repo migrations dir is resolved from the crate manifest so
/// the test does not depend on the process working directory.
async fn apply_migrations(pool: &sqlx::SqlitePool, files: &[&str]) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/sqlite");
    let dir = tempdir().unwrap();
    for file in files {
        std::fs::copy(source.join(file), dir.path().join(file)).unwrap();
    }
    Migrator::new(dir.path())
        .await
        .unwrap()
        .run(pool)
        .await
        .unwrap();
}

/// Seed rows using only V1 columns, the way an early build wrote
/// them: one account, two repositories, three jobs (pending,
/// completed, failed with its error recorded), one global schedule,
/// one history row. Ids are real UUIDs — the repository layer
/// parses them — and are returned so the tests can address the rows.
async fn seed_v1_rows(pool: &sqlx::SqlitePool) -> (Uuid, Uuid, Uuid) {
    let account_id = Uuid::new_v4();
    let repo_hello = Uuid::new_v4();
    let repo_world = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, provider, provider_account_id, username, authentication_method, created_at, updated_at)
         VALUES (?, 'github', '123', 'octocat', 'fine_grained_pat', datetime('now'), datetime('now'))",
    )
    .bind(account_id.to_string())
    .execute(pool)
    .await
    .unwrap();
    for (id, provider_id, name) in [(repo_hello, "456", "hello"), (repo_world, "457", "world")] {
        sqlx::query(
            "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, archive_status, sync_mode, created_at, updated_at)
             VALUES (?, ?, ?, 'octocat', ?, ?, ?, 'synced', 'mirror', datetime('now'), datetime('now'))",
        )
        .bind(id.to_string())
        .bind(account_id.to_string())
        .bind(provider_id)
        .bind(name)
        .bind(format!("octocat/{name}"))
        .bind(format!("https://github.com/octocat/{name}.git"))
        .execute(pool)
        .await
        .unwrap();
    }
    for (repo, job_type, status, error_code) in [
        (repo_hello, "fetch", "pending", None),
        (repo_hello, "mirror_clone", "completed", None),
        (repo_world, "fetch", "failed", Some("NetworkUnavailable")),
    ] {
        sqlx::query(
            "INSERT INTO sync_jobs (id, repository_id, job_type, status, priority, attempt_count, error_code, created_at)
             VALUES (?, ?, ?, ?, 1, 2, ?, datetime('now'))",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(repo.to_string())
        .bind(job_type)
        .bind(status)
        .bind(error_code)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO schedules (id, scope, interval_type, timezone, enabled, created_at, updated_at)
         VALUES (?, 'global', 'interval_hours', 'UTC', 1, datetime('now'), datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sync_history (id, repository_id, job_id, operation, result, started_at)
         VALUES (?, ?, NULL, 'mirror_clone', 'success', datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(repo_hello.to_string())
    .execute(pool)
    .await
    .unwrap();
    (account_id, repo_hello, repo_world)
}

async fn count(pool: &sqlx::SqlitePool, table: &str) -> i64 {
    // Static queries per table: sqlx rejects dynamic SQL strings,
    // and a table name cannot be a bound parameter.
    let sql = match table {
        "accounts" => "SELECT COUNT(*) AS n FROM accounts",
        "repositories" => "SELECT COUNT(*) AS n FROM repositories",
        "sync_jobs" => "SELECT COUNT(*) AS n FROM sync_jobs",
        "schedules" => "SELECT COUNT(*) AS n FROM schedules",
        "sync_history" => "SELECT COUNT(*) AS n FROM sync_history",
        "settings" => "SELECT COUNT(*) AS n FROM settings",
        _ => panic!("unknown table {table}"),
    };
    sqlx::query(sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i64, _>("n")
}

/// A V1 database upgrades to the latest schema with its data
/// intact — the in-place upgrade every existing install takes.
#[tokio::test]
async fn v1_database_upgrades_to_latest_with_data_intact() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("reponyx.sqlite");
    let pool = SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();

    apply_migrations(&pool, &["0001_init.sql"]).await;
    let (account_id, repo_hello, _repo_world) = seed_v1_rows(&pool).await;

    // The full migration set, exactly as the app runs it.
    sqlx::migrate!("../../migrations/sqlite")
        .run(&pool)
        .await
        .unwrap();

    // All four migrations recorded, in order, by description.
    let applied: Vec<String> =
        sqlx::query("SELECT description FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<String, _>("description"))
            .collect();
    assert_eq!(
        applied,
        [
            "init",
            "unique full name",
            "pat expiry",
            "queue fields",
            "phase5 indexes"
        ]
    );

    // Data survived the jump.
    assert_eq!(count(&pool, "accounts").await, 1);
    assert_eq!(count(&pool, "repositories").await, 2);
    assert_eq!(count(&pool, "sync_jobs").await, 3);
    assert_eq!(count(&pool, "schedules").await, 1);
    assert_eq!(count(&pool, "sync_history").await, 1);

    // V4 columns exist, with the old rows' values preserved and
    // the new columns at their defaults. The failed job is the one
    // with error_code set (only job-3 in the seed carries one).
    let job: (i64, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT attempt_count, available_at, dedupe_key FROM sync_jobs WHERE error_code = 'NetworkUnavailable'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(job.0, 2, "attempt_count survives the upgrade");
    assert!(job.1.is_none(), "available_at defaults to NULL");
    assert!(job.2.is_none(), "dedupe_key defaults to NULL");

    // V2's uniqueness now holds: a second row with the same
    // full_name in the same account is rejected at the DB layer.
    let dup = sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, created_at, updated_at)
         VALUES (?, ?, '999', 'octocat', 'hello', 'octocat/hello', 'https://github.com/octocat/hello.git', datetime('now'), datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(account_id.to_string())
    .execute(&pool)
    .await;
    assert!(
        dup.is_err(),
        "V2 unique full_name index must reject the duplicate"
    );

    // The V1 settings seed is still there.
    assert_eq!(count(&pool, "settings").await, 4);

    // The app's own functions work on the upgraded database:
    // creating a job now writes a dedupe key, and listing works.
    let repos = list_repositories(&pool, RepositoryFilter::default())
        .await
        .unwrap();
    assert_eq!(repos.len(), 2);
    let job = create_job(
        &pool,
        NewJob {
            repository_id: repo_hello,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await
    .unwrap();
    let dedupe: Option<String> =
        sqlx::query_scalar("SELECT dedupe_key FROM sync_jobs WHERE id = ?")
            .bind(job.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        dedupe.as_deref() == Some(&format!("repo:{repo_hello}:fetch")),
        "new jobs get a dedupe key after the upgrade"
    );
}

/// A V3 database upgrades to V4, and the new partial unique index
/// actually dedupes: two active jobs for the same repository and
/// type cannot both exist afterwards.
#[tokio::test]
async fn v5_indexes_and_case_insensitive_names() {
    let dir = tempdir().unwrap();
    let db = open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    let pool = &db.pool;

    // The V5 indexes exist.
    let indexes: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='index' AND (name LIKE 'idx_schedules_account_id' OR name LIKE 'idx_sync_history_job_id' OR name LIKE 'idx_audit_logs_%' OR name LIKE 'idx_repositories_local_path')",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get::<String, _>("name"))
    .collect();
    for expected in [
        "idx_schedules_account_id",
        "idx_sync_history_job_id",
        "idx_audit_logs_actor_created",
        "idx_audit_logs_resource_created",
        "idx_repositories_local_path",
    ] {
        assert!(
            indexes.contains(&expected.to_string()),
            "missing index {expected}"
        );
    }

    // Seed one repository, then prove the case-insensitive rules:
    // a second row whose full_name differs only by case, and a
    // second row reusing the same local_path, must both fail.
    let account_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, provider, provider_account_id, username, authentication_method, created_at, updated_at)
         VALUES (?, 'github', '123', 'octocat', 'fine_grained_pat', datetime('now'), datetime('now'))",
    )
    .bind(account_id.to_string())
    .execute(pool)
    .await
    .unwrap();
    let repo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, archive_status, sync_mode, local_path, created_at, updated_at)
         VALUES (?, ?, '456', 'octocat', 'hello', 'octocat/hello', 'https://github.com/octocat/hello.git', 'synced', 'mirror', '/tmp/hello.git', datetime('now'), datetime('now'))",
    )
    .bind(repo_id.to_string())
    .bind(account_id.to_string())
    .execute(pool)
    .await
    .unwrap();
    let case_variant = sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, archive_status, sync_mode, created_at, updated_at)
         VALUES (?, ?, '789', 'OctoCat', 'Hello', 'OctoCat/Hello', 'https://github.com/octocat/hello.git', 'pending', 'mirror', datetime('now'), datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(account_id.to_string())
    .execute(pool)
    .await;
    assert!(
        case_variant.is_err(),
        "NOCASE full_name index must reject a case-variant duplicate"
    );
    let same_path = sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, clone_url, archive_status, sync_mode, local_path, created_at, updated_at)
         VALUES (?, ?, '999', 'octocat', 'world', 'octocat/world', 'https://github.com/octocat/world.git', 'pending', 'mirror', '/tmp/hello.git', datetime('now'), datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(account_id.to_string())
    .execute(pool)
    .await;
    assert!(
        same_path.is_err(),
        "local_path UNIQUE index must reject a shared archive path"
    );
}

/// Audit DB-m2: a database whose migrations were applied without
/// records (the old desktop bug) must open cleanly — the repair
/// backfills the records from the embedded checksums and the
/// normal migrator applies only what is genuinely missing.
#[tokio::test]
async fn open_database_repairs_unrecorded_migrations() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("reponyx.sqlite");
    // A V1-shaped database, built the old way: 0001's SQL run
    // directly, with no `_sqlx_migrations` table at all.
    let setup = SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/sqlite");
    let v1 = std::fs::read_to_string(source.join("0001_init.sql")).unwrap();
    // The file is our own migration, read from the repo — audited
    // as safe to execute as-is.
    sqlx::raw_sql(sqlx::AssertSqlSafe(v1.as_str()))
        .execute(&setup)
        .await
        .unwrap();
    setup.close().await;

    let db = open_database(&db_path).await.unwrap();
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(versions, vec![1, 2, 3, 4, 5]);
    // The later migrations were applied, not just recorded: the V3
    // column and the V5 index exist.
    let pat_col: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name = 'pat_expires_at'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(pat_col, 1);
    let v5_index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_repositories_local_path'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(v5_index, 1);
    // And opening it a second time is a clean no-op.
    drop(db);
    let again = open_database(&db_path).await.unwrap();
    let versions_again: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&again.pool)
            .await
            .unwrap();
    assert_eq!(versions_again, vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn v3_database_upgrades_to_v4_and_dedupes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("reponyx.sqlite");
    let pool = SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();

    apply_migrations(
        &pool,
        &[
            "0001_init.sql",
            "0002_unique_full_name.sql",
            "0003_pat_expiry.sql",
        ],
    )
    .await;
    let (account_id, _repo_hello, _repo_world) = seed_v1_rows(&pool).await;

    sqlx::migrate!("../../migrations/sqlite")
        .run(&pool)
        .await
        .unwrap();

    // The V4 indexes exist.
    let indexes: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_sync_jobs%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get::<String, _>("name"))
    .collect();
    for expected in [
        "idx_sync_jobs_dedupe_active",
        "idx_sync_jobs_available_at",
        "idx_sync_jobs_lease_running",
    ] {
        assert!(
            indexes.contains(&expected.to_string()),
            "missing index {expected}"
        );
    }

    // V3 added pat_expires_at; the column exists and is NULL for
    // the old row.
    let expiry: Option<String> =
        sqlx::query_scalar("SELECT pat_expires_at FROM accounts WHERE id = ?")
            .bind(account_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(expiry.is_none());

    // Dedupe: after the upgrade, two NEW active jobs for the same
    // repository and type cannot both exist. (Rows written before
    // the upgrade carry a NULL dedupe key, so the index does not
    // retroactively dedupe them — the scheduler's status-based
    // `has_active_job` check is what keeps those from duplicating;
    // the index is the second line of defence for new rows.)
    let repos = list_repositories(&pool, RepositoryFilter::default())
        .await
        .unwrap();
    let repo_1 = repos.iter().find(|r| r.name == "hello").unwrap();
    let first = create_job(
        &pool,
        NewJob {
            repository_id: repo_1.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await;
    assert!(first.is_ok(), "first post-upgrade fetch must succeed");
    let dup = create_job(
        &pool,
        NewJob {
            repository_id: repo_1.id,
            job_type: JobType::Fetch,
            priority: 0,
        },
    )
    .await;
    assert!(
        dup.is_err(),
        "V4 dedupe index must reject the second active fetch"
    );
}
