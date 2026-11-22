//! Repository repository integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::{ArchiveStatus, ErrorCode, SyncMode, Visibility};
use reponyx_database::repositories::{
    NewRepository, RepositoryFilter, count_repositories, create_repository, delete_repository,
    get_repository, get_repository_by_provider_id, list_repositories, mark_synced,
    rename_repository, set_archive_status, set_local_path, set_sync_enabled, set_sync_mode,
    upsert_repository,
};
use uuid::Uuid;

fn new_repo(account_id: Uuid, provider_id: &str, owner: &str, name: &str) -> NewRepository {
    NewRepository {
        account_id,
        provider_repository_id: provider_id.to_string(),
        owner: owner.to_string(),
        name: name.to_string(),
        visibility: Visibility::Private,
        default_branch: Some("main".to_string()),
        clone_url: format!("https://github.com/{owner}/{name}.git"),
        sync_enabled: true,
        sync_mode: SyncMode::Mirror,
        local_path: None,
        remote_updated_at: None,
    }
}

#[tokio::test]
async fn create_and_get_round_trip() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    assert_eq!(repo.full_name, "octocat/hello");
    assert_eq!(repo.archive_status, ArchiveStatus::Pending);
    assert!(repo.sync_enabled);
    assert_eq!(repo.sync_mode, SyncMode::Mirror);
    let fetched = get_repository(pool(&db), repo.id).await.unwrap().unwrap();
    assert_eq!(fetched.provider_repository_id, "456");
    assert_eq!(fetched.clone_url, "https://github.com/octocat/hello.git");
}

#[tokio::test]
async fn create_rejects_missing_account_fk() {
    let (_dir, db) = open_test_db().await;
    let err = create_repository(pool(&db), new_repo(Uuid::new_v4(), "1", "ghost", "repo"))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn upsert_by_provider_id_is_stable_identity() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let (first, created) =
        upsert_repository(pool(&db), new_repo(account.id, "456", "octocat", "hello"))
            .await
            .unwrap();
    assert!(created);
    let mut changed = new_repo(account.id, "456", "octocat", "hello");
    changed.visibility = Visibility::Public;
    changed.default_branch = Some("trunk".to_string());
    let (second, created_again) = upsert_repository(pool(&db), changed).await.unwrap();
    assert!(!created_again);
    assert_eq!(second.id, first.id, "upsert must not create a second row");
    assert_eq!(second.visibility, Visibility::Public);
    assert_eq!(second.default_branch.as_deref(), Some("trunk"));
    assert_eq!(
        count_repositories(pool(&db), RepositoryFilter::default())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn upsert_preserves_local_state() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let (repo, _) = upsert_repository(pool(&db), new_repo(account.id, "456", "octocat", "hello"))
        .await
        .unwrap();
    set_sync_enabled(pool(&db), repo.id, false).await.unwrap();
    set_sync_mode(pool(&db), repo.id, SyncMode::Working)
        .await
        .unwrap();
    set_archive_status(pool(&db), repo.id, ArchiveStatus::Synced)
        .await
        .unwrap();
    let (after, _) = upsert_repository(pool(&db), new_repo(account.id, "456", "octocat", "hello"))
        .await
        .unwrap();
    assert!(!after.sync_enabled, "upsert must not reset sync_enabled");
    assert_eq!(after.sync_mode, SyncMode::Working);
    assert_eq!(after.archive_status, ArchiveStatus::Synced);
}

#[tokio::test]
async fn filters_and_counts() {
    let (_dir, db) = open_test_db().await;
    let a = make_account(pool(&db), "alice").await;
    let b = make_account(pool(&db), "bob").await;
    let r1 = make_repository(pool(&db), a.id, "1", "alice", "alpha").await;
    let r2 = make_repository(pool(&db), a.id, "2", "alice", "beta").await;
    let _r3 = make_repository(pool(&db), b.id, "3", "bob", "alpha").await;
    set_archive_status(pool(&db), r2.id, ArchiveStatus::Synced)
        .await
        .unwrap();
    set_sync_enabled(pool(&db), r1.id, false).await.unwrap();

    let by_account = list_repositories(
        pool(&db),
        RepositoryFilter {
            account_id: Some(a.id),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_account.len(), 2);

    let synced = list_repositories(
        pool(&db),
        RepositoryFilter {
            archive_status: Some(ArchiveStatus::Synced),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(synced.len(), 1);
    assert_eq!(synced[0].id, r2.id);

    let disabled = list_repositories(
        pool(&db),
        RepositoryFilter {
            sync_enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].id, r1.id);

    let search = list_repositories(
        pool(&db),
        RepositoryFilter {
            full_name_search: Some("alice/al".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(search.len(), 1);
    assert_eq!(search[0].full_name, "alice/alpha");

    assert_eq!(
        count_repositories(
            pool(&db),
            RepositoryFilter {
                account_id: Some(b.id),
                ..Default::default()
            }
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        count_repositories(pool(&db), RepositoryFilter::default())
            .await
            .unwrap(),
        3
    );
}

#[tokio::test]
async fn rename_updates_identity_and_clone_url() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let renamed = rename_repository(pool(&db), account.id, "456", "octocat", "hello-world")
        .await
        .unwrap();
    assert_eq!(renamed.id, repo.id, "rename keeps the same row");
    assert_eq!(renamed.name, "hello-world");
    assert_eq!(renamed.full_name, "octocat/hello-world");
    assert_eq!(
        renamed.clone_url,
        "https://github.com/octocat/hello-world.git"
    );
    // Still findable by provider id.
    let found = get_repository_by_provider_id(pool(&db), account.id, "456")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.full_name, "octocat/hello-world");
}

#[tokio::test]
async fn rename_collision_is_rejected_and_other_repo_untouched() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let _taken = make_repository(pool(&db), account.id, "1", "octocat", "taken").await;
    let other = make_repository(pool(&db), account.id, "2", "octocat", "free").await;
    let err = rename_repository(pool(&db), account.id, "2", "octocat", "taken")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    let unchanged = get_repository(pool(&db), other.id).await.unwrap().unwrap();
    assert_eq!(unchanged.full_name, "octocat/free");

    // Same full_name in a *different* account is fine.
    let other_account = make_account(pool(&db), "someone-else").await;
    let moved = make_repository(pool(&db), other_account.id, "9", "someone-else", "x").await;
    let renamed = rename_repository(pool(&db), other_account.id, "9", "octocat", "taken")
        .await
        .unwrap();
    assert_eq!(renamed.id, moved.id);
    assert_eq!(renamed.full_name, "octocat/taken");
}

#[tokio::test]
async fn rename_missing_repository_rejected() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let err = rename_repository(pool(&db), account.id, "nope", "octocat", "x")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn mark_synced_and_delete() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let synced = mark_synced(pool(&db), repo.id, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(synced.archive_status, ArchiveStatus::Synced);
    assert!(synced.last_synced_at.is_some());
    assert!(delete_repository(pool(&db), repo.id).await.unwrap());
    assert!(get_repository(pool(&db), repo.id).await.unwrap().is_none());
    assert!(!delete_repository(pool(&db), repo.id).await.unwrap());
}

#[tokio::test]
async fn set_local_path_sets_and_clears() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    assert!(repo.local_path.is_none());

    let path = std::path::Path::new("/data/reponyx/repositories/github/octocat/octocat/hello.git");
    let updated = set_local_path(pool(&db), repo.id, Some(path))
        .await
        .unwrap();
    assert_eq!(updated.local_path.as_deref(), Some(path.to_str().unwrap()));

    let cleared = set_local_path(pool(&db), repo.id, None).await.unwrap();
    assert!(cleared.local_path.is_none());

    let err = set_local_path(pool(&db), Uuid::new_v4(), Some(path))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn upsert_full_name_collision_rejected() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let _first = make_repository(pool(&db), account.id, "1", "octocat", "taken").await;
    // A different provider id, but the same owner/name.
    let err = upsert_repository(pool(&db), new_repo(account.id, "2", "octocat", "taken"))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(
        count_repositories(pool(&db), RepositoryFilter::default())
            .await
            .unwrap(),
        1
    );
}
