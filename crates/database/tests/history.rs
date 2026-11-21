//! Sync-history integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use chrono::{Duration, Utc};
use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::SyncResult;
use reponyx_database::history::{
    NewHistory, list_history_for_repository, list_recent_history, record_history,
};

#[tokio::test]
async fn recent_history_is_newest_first_and_limited() {
    let (_dir, db) = open_test_db().await;
    let p = pool(&db);
    let account = make_account(p, "octocat").await;
    let repo_a = make_repository(p, account.id, "1", "octocat", "hello").await;
    let repo_b = make_repository(p, account.id, "2", "octocat", "world").await;
    let now = Utc::now();
    for (repo, minutes_ago, operation) in [
        (repo_a.id, 120, "mirror_clone"),
        (repo_b.id, 60, "mirror_clone"),
        (repo_a.id, 0, "fetch"),
    ] {
        record_history(
            p,
            NewHistory {
                repository_id: repo,
                job_id: None,
                operation: operation.to_string(),
                result: SyncResult::Success,
                duration_ms: Some(41),
                bytes_received: None,
                started_at: now - Duration::minutes(minutes_ago),
                completed_at: Some(now - Duration::minutes(minutes_ago)),
            },
        )
        .await
        .unwrap();
    }

    let recent = list_recent_history(p, 2).await.unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].operation, "fetch");
    assert_eq!(recent[1].repository_id, repo_b.id);

    let all = list_recent_history(p, 100).await.unwrap();
    assert_eq!(all.len(), 3);

    let for_a = list_history_for_repository(p, repo_a.id, 10).await.unwrap();
    assert_eq!(for_a.len(), 2);
    assert_eq!(for_a[0].operation, "fetch");
    assert_eq!(for_a[1].operation, "mirror_clone");
}
