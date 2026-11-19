//! Transaction rollback, concurrent-write sanity, and the V2 unique
//! index — integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::JobType;
use reponyx_core::{SyncMode, Visibility};
use reponyx_database::jobs::{NewJob, create_job, get_job};
use reponyx_database::repositories::{NewRepository, count_repositories, upsert_repository};
use reponyx_database::settings::set_setting;
use sqlx::Row;

#[tokio::test]
async fn transaction_rolls_back_on_failure() {
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

    // A transaction that updates the job, then fails on a foreign-key
    // violation (history for a nonexistent repository), must roll the
    // job update back too.
    let mut tx = pool(&db).begin().await.unwrap();
    sqlx::query("UPDATE sync_jobs SET status = 'running' WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let bad_insert = sqlx::query(
        r#"
        INSERT INTO sync_history
            (id, repository_id, job_id, operation, result, started_at)
        VALUES ('hist-1', 'no-such-repo', ?, 'fetch', 'success', datetime('now'))
        "#,
    )
    .bind(job.id.to_string())
    .execute(&mut *tx)
    .await;
    assert!(bad_insert.is_err(), "FK violation must fail the insert");
    tx.rollback().await.unwrap();

    let job_after = get_job(pool(&db), job.id).await.unwrap().unwrap();
    assert_eq!(
        job_after.status,
        reponyx_core::JobStatus::Pending,
        "the rolled-back update must not stick"
    );
    let history_count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM sync_history")
        .fetch_one(pool(&db))
        .await
        .unwrap()
        .get("n");
    assert_eq!(history_count, 0);
}

#[tokio::test]
async fn concurrent_writes_all_succeed_without_corruption() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let mut handles = Vec::new();
    for task in 0..8 {
        let pool_clone = pool(&db).clone();
        let account_id = account.id;
        handles.push(tokio::spawn(async move {
            for i in 0..5 {
                let name = format!("repo-{task}-{i}");
                make_repository(
                    &pool_clone,
                    account_id,
                    &format!("prov-{task}-{i}"),
                    "octocat",
                    &name,
                )
                .await;
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(
        count_repositories(pool(&db), Default::default())
            .await
            .unwrap(),
        40,
        "all 40 concurrent inserts must land exactly once"
    );

    // Concurrent settings writes to distinct keys.
    let mut handles = Vec::new();
    for task in 0..8 {
        let pool_clone = pool(&db).clone();
        handles.push(tokio::spawn(async move {
            set_setting(&pool_clone, &format!("bench.key-{task}"), "v")
                .await
                .unwrap();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    // 4 seeded + 8 written.
    assert_eq!(
        reponyx_database::settings::list_settings(pool(&db))
            .await
            .unwrap()
            .len(),
        12
    );
}

#[tokio::test]
async fn concurrent_upsert_of_same_repository_yields_one_row() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let mut handles = Vec::new();
    for _ in 0..4 {
        let pool_clone = pool(&db).clone();
        let account_id = account.id;
        handles.push(tokio::spawn(async move {
            upsert_repository(
                &pool_clone,
                NewRepository {
                    account_id,
                    provider_repository_id: "456".to_string(),
                    owner: "octocat".to_string(),
                    name: "hello".to_string(),
                    visibility: Visibility::Private,
                    default_branch: Some("main".to_string()),
                    clone_url: "https://github.com/octocat/hello.git".to_string(),
                    sync_enabled: true,
                    sync_mode: SyncMode::Mirror,
                    local_path: None,
                    remote_updated_at: None,
                },
            )
            .await
            .unwrap();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(
        count_repositories(pool(&db), Default::default())
            .await
            .unwrap(),
        1,
        "concurrent upserts of the same identity must not duplicate"
    );
}

#[tokio::test]
async fn v2_unique_full_name_index_exists_and_enforces() {
    let (_dir, db) = open_test_db().await;
    let indexes: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'repositories'",
    )
    .fetch_all(pool(&db))
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get::<String, _>("name"))
    .collect();
    assert!(
        indexes.contains(&"idx_repositories_account_full_name".to_string()),
        "V2 migration must create the unique full_name index, got {indexes:?}"
    );

    let account = make_account(pool(&db), "octocat").await;
    make_repository(pool(&db), account.id, "1", "octocat", "taken").await;
    // Same full_name, different provider id: raw SQL must fail at the
    // storage layer, not just in the repository code.
    let duplicate = sqlx::query(
        r#"
        INSERT INTO repositories
            (id, account_id, provider_repository_id, owner, name, full_name,
             clone_url, created_at, updated_at)
        VALUES ('repo-dup', ?, '2', 'octocat', 'taken', 'octocat/taken',
                'https://github.com/octocat/taken.git', datetime('now'), datetime('now'))
        "#,
    )
    .bind(account.id.to_string())
    .execute(pool(&db))
    .await;
    assert!(
        duplicate.is_err(),
        "V2 unique index must reject the duplicate"
    );
    assert_eq!(
        count_repositories(pool(&db), Default::default())
            .await
            .unwrap(),
        1
    );
}
