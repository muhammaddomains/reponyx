//! Performance harness (Phase 9 performance pass).
//!
//! Seeds a database at a realistic stress scale — 1,000
//! repositories, 10,000 jobs, 5,000 history rows — then times the
//! queries the pages actually run. Timings are printed, not
//! asserted: CI runners are too noisy for hard thresholds, and a
//! slow runner is not a regression. What IS asserted is the
//! pagination behaviour (limits respected, offsets page, totals
//! match), so a future change cannot silently reintroduce a
//! full-list load at the Tauri boundary.
//!
//! Run with `-- --nocapture` to see the numbers, e.g.:
//! `cargo test -p reponyx-database --test performance -- --nocapture`

#[path = "common.rs"]
mod common;

use chrono::{TimeZone, Utc};
use common::{make_account, open_test_db, pool};
use reponyx_core::JobStatus;
use reponyx_database::counts::dashboard_counts;
use reponyx_database::history::list_recent_history;
use reponyx_database::jobs::{JobFilter, list_jobs};
use reponyx_database::repositories::{RepositoryFilter, count_repositories, list_repositories};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::time::Instant;
use uuid::Uuid;

const REPO_COUNT: usize = 1_000;
const JOB_COUNT: usize = 10_000;
const HISTORY_COUNT: usize = 5_000;

/// Median of three runs, in milliseconds, printed for the record.
async fn time_it(label: &str, mut run: impl AsyncFnMut() -> usize) -> f64 {
    let mut times = Vec::with_capacity(3);
    let mut rows = 0;
    for _ in 0..3 {
        let start = Instant::now();
        rows = run().await;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[1];
    eprintln!("perf: {label}: median {median:.2} ms ({rows} rows)");
    median
}

/// Seed the stress scale with multi-row INSERTs in one
/// transaction — the public create helpers would take minutes at
/// this volume. `QueryBuilder::push_values` keeps every value
/// bound, so there is no dynamic SQL to audit.
async fn seed(pool: &SqlitePool) -> Vec<Uuid> {
    let account_a = make_account(pool, "alice").await;
    let account_b = make_account(pool, "bob").await;
    let accounts = [account_a.id, account_b.id];
    let base = Utc.with_ymd_and_hms(2026, 11, 22, 0, 0, 0).unwrap();

    let mut tx = pool.begin().await.unwrap();
    // Repositories, split across the two accounts.
    let mut repo_ids = Vec::with_capacity(REPO_COUNT);
    for chunk in 0..(REPO_COUNT / 1_000) {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO repositories (id, account_id, provider_repository_id, owner, name, full_name, visibility, default_branch, clone_url, archive_status, sync_enabled, sync_mode, local_path, remote_updated_at, last_synced_at, created_at, updated_at) ",
        );
        let rows: Vec<(String, String, String, String, String, String, String)> = (0..1_000)
            .map(|i| {
                let n = chunk * 1_000 + i;
                let id = Uuid::new_v4();
                repo_ids.push(id);
                (
                    id.to_string(),
                    accounts[n % 2].to_string(),
                    format!("prov-{n}"),
                    format!("owner{n:04}"),
                    format!("repo{n:04}"),
                    format!("owner{n:04}/repo{n:04}"),
                    format!("https://github.com/owner{n:04}/repo{n:04}.git"),
                )
            })
            .collect();
        builder.push_values(rows, |mut b, row| {
            let at = (base + chrono::Duration::seconds(0)).to_rfc3339();
            b.push_bind(row.0)
                .push_bind(row.1)
                .push_bind(row.2)
                .push_bind(row.3.clone())
                .push_bind(row.4.clone())
                .push_bind(row.5)
                .push_bind("private")
                .push_bind("main")
                .push_bind(row.6)
                .push_bind("pending")
                .push_bind(1_i64)
                .push_bind("mirror")
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(at.clone())
                .push_bind(at);
        });
        builder.build().execute(&mut *tx).await.unwrap();
    }
    // Jobs: mostly completed, some failed, a few pending/running —
    // the shape a long-running install accumulates.
    let statuses = [
        "completed",
        "completed",
        "completed",
        "completed",
        "completed",
        "completed",
        "completed",
        "completed",
        "failed",
        "pending",
        "pending",
        "running",
    ];
    for chunk in 0..(JOB_COUNT / 1_000) {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO sync_jobs (id, repository_id, job_type, status, priority, attempt_count, started_at, completed_at, error_code, error_message, created_at) ",
        );
        let rows: Vec<(String, String, String, String)> = (0..1_000)
            .map(|i| {
                let n = chunk * 1_000 + i;
                (
                    Uuid::new_v4().to_string(),
                    repo_ids[n % REPO_COUNT].to_string(),
                    statuses[n % statuses.len()].to_string(),
                    (base + chrono::Duration::seconds(n as i64)).to_rfc3339(),
                )
            })
            .collect();
        builder.push_values(rows, |mut b, row| {
            b.push_bind(row.0)
                .push_bind(row.1)
                .push_bind("fetch")
                .push_bind(row.2)
                .push_bind(0_i64)
                .push_bind(0_i64)
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(row.3);
        });
        builder.build().execute(&mut *tx).await.unwrap();
    }
    // History rows, one per repo round-robin.
    for chunk in 0..(HISTORY_COUNT / 1_000) {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO sync_history (id, repository_id, job_id, operation, result, duration_ms, bytes_received, started_at, completed_at) ",
        );
        let rows: Vec<(String, String, String)> = (0..1_000)
            .map(|i| {
                let n = chunk * 1_000 + i;
                (
                    Uuid::new_v4().to_string(),
                    repo_ids[n % REPO_COUNT].to_string(),
                    (base + chrono::Duration::seconds(n as i64)).to_rfc3339(),
                )
            })
            .collect();
        builder.push_values(rows, |mut b, row| {
            b.push_bind(row.0)
                .push_bind(row.1)
                .push_bind(None::<String>)
                .push_bind("fetch")
                .push_bind("success")
                .push_bind(120_i64)
                .push_bind(4096_i64)
                .push_bind(row.2.clone())
                .push_bind(row.2);
        });
        builder.build().execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    repo_ids
}

#[tokio::test]
async fn baseline_timings() {
    let (_dir, db) = open_test_db().await;
    let db_pool = pool(&db);
    let _repos = seed(db_pool).await;

    // Sanity: the seed really landed.
    assert_eq!(
        count_repositories(db_pool, RepositoryFilter::default())
            .await
            .unwrap(),
        REPO_COUNT as i64
    );

    time_it("list_repositories (all 1,000)", async || {
        list_repositories(db_pool, RepositoryFilter::default())
            .await
            .unwrap()
            .len()
    })
    .await;
    time_it("list_jobs pending", async || {
        list_jobs(
            db_pool,
            JobFilter {
                status: Some(JobStatus::Pending),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .len()
    })
    .await;
    time_it("list_jobs completed", async || {
        list_jobs(
            db_pool,
            JobFilter {
                status: Some(JobStatus::Completed),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .len()
    })
    .await;
    time_it("list_jobs all (10,000)", async || {
        list_jobs(db_pool, JobFilter::default())
            .await
            .unwrap()
            .len()
    })
    .await;
    time_it("list_repositories page (limit 100)", async || {
        list_repositories(
            db_pool,
            RepositoryFilter {
                limit: Some(100),
                offset: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .len()
    })
    .await;
    time_it("list_jobs pending (limit 100)", async || {
        list_jobs(
            db_pool,
            JobFilter {
                status: Some(JobStatus::Pending),
                limit: Some(100),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .len()
    })
    .await;
    time_it("list_jobs completed (limit 100)", async || {
        list_jobs(
            db_pool,
            JobFilter {
                status: Some(JobStatus::Completed),
                limit: Some(100),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .len()
    })
    .await;
    time_it("list_recent_history(100)", async || {
        list_recent_history(db_pool, 100).await.unwrap().len()
    })
    .await;
    time_it("dashboard_counts", async || {
        dashboard_counts(db_pool).await.unwrap();
        1
    })
    .await;
    time_it("count_repositories", async || {
        count_repositories(db_pool, RepositoryFilter::default())
            .await
            .unwrap() as usize
    })
    .await;
}

#[tokio::test]
async fn pagination_is_correct_and_bounded() {
    let (_dir, db) = open_test_db().await;
    let db_pool = pool(&db);
    let repos = seed(db_pool).await;

    // Pages cover the whole table exactly once, in full_name order.
    let mut seen = std::collections::HashSet::new();
    let mut offset = 0;
    loop {
        let page = list_repositories(
            db_pool,
            RepositoryFilter {
                limit: Some(100),
                offset: Some(offset),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 100);
        for repo in &page {
            assert!(seen.insert(repo.id), "page overlap at offset {offset}");
        }
        offset += page.len() as i64;
    }
    assert_eq!(seen.len(), REPO_COUNT);
    assert_eq!(offset, REPO_COUNT as i64);

    // The total matches, so the page header can show "N repositories".
    assert_eq!(
        count_repositories(db_pool, RepositoryFilter::default())
            .await
            .unwrap(),
        REPO_COUNT as i64
    );

    // Limit clamps: 0 becomes 1, huge becomes 500.
    let one = list_repositories(
        db_pool,
        RepositoryFilter {
            limit: Some(0),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(one.len(), 1);
    let capped = list_repositories(
        db_pool,
        RepositoryFilter {
            limit: Some(9_999),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(capped.len(), 500);

    // Offset without a limit is ignored (no LIMIT/OFFSET emitted).
    let all = list_repositories(
        db_pool,
        RepositoryFilter {
            offset: Some(500),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), REPO_COUNT);

    // Jobs: the limit bounds the poll, whatever the status.
    let pending = list_jobs(
        db_pool,
        JobFilter {
            status: Some(JobStatus::Pending),
            limit: Some(100),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(pending.len(), 100);
    let completed = list_jobs(
        db_pool,
        JobFilter {
            status: Some(JobStatus::Completed),
            limit: Some(100),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(completed.len(), 100);

    // Names: only the ids asked for, unknown ids absent, empty in.
    let wanted = &repos[..3];
    let names = reponyx_database::repositories::repository_names(db_pool, wanted)
        .await
        .unwrap();
    assert_eq!(names.len(), 3);
    for id in wanted {
        assert!(names.get(id).unwrap().starts_with("owner"));
    }
    let mut with_unknown = wanted.to_vec();
    with_unknown.push(Uuid::new_v4());
    let names = reponyx_database::repositories::repository_names(db_pool, &with_unknown)
        .await
        .unwrap();
    assert_eq!(names.len(), 3);
    let empty = reponyx_database::repositories::repository_names(db_pool, &[])
        .await
        .unwrap();
    assert!(empty.is_empty());
}
