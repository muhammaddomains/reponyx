//! Dashboard counts and other read queries used by the desktop UI.
//! All counts are real queries against the live database; a fresh
//! database returns zeros.

use reponyx_core::DashboardCounts;
use sqlx::{Row, SqlitePool};

/// Counts for the Overview dashboard.
pub async fn dashboard_counts(pool: &SqlitePool) -> Result<DashboardCounts, sqlx::Error> {
    // Plain `query` (not `query!`): the compile-time-checked macro would
    // require a live database at build time. Counts are simple integers.
    let row = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*) FROM accounts) AS accounts,
            (SELECT COUNT(*) FROM repositories) AS repositories,
            (SELECT COUNT(*) FROM repositories WHERE archive_status = 'synced') AS synced,
            (SELECT COUNT(*) FROM repositories WHERE archive_status = 'syncing') AS syncing,
            (SELECT COUNT(*) FROM repositories WHERE archive_status = 'failed') AS failed,
            (SELECT COUNT(*) FROM sync_jobs WHERE status = 'pending') AS pending_jobs
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(DashboardCounts {
        accounts: row.get("accounts"),
        repositories: row.get("repositories"),
        synced: row.get("synced"),
        syncing: row.get("syncing"),
        failed: row.get("failed"),
        pending_jobs: row.get("pending_jobs"),
    })
}
