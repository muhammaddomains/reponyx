//! Discovery-to-database tests: mocked discovery writes into a temp
//! SQLite database via the Phase 2 repositories. Upserts preserve
//! local state; repositories absent from discovery are reported, not
//! deleted. All tokens are fake.

#[path = "common.rs"]
mod common;

use common::{installation_json, mount_sequence, next_link, provider_for, repo_json};
use reponyx_core::{
    AccountType, ArchiveStatus, AuthenticationMethod, Provider, SyncMode, Visibility,
};
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::installations::list_installations_for_account;
use reponyx_database::repositories::{
    NewRepository, RepositoryFilter, create_repository, list_repositories,
};
use reponyx_database::settings::set_setting;
use reponyx_github_provider::sync::{discover_and_sync, sync_installations};
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn open_test_db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let db = reponyx_database::open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    (dir, db.pool)
}

async fn make_account(pool: &SqlitePool, username: &str) -> Uuid {
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
    .id
}

async fn seed_repo(
    pool: &SqlitePool,
    account_id: Uuid,
    provider_id: &str,
    owner: &str,
    name: &str,
) -> Uuid {
    create_repository(
        pool,
        NewRepository {
            account_id,
            provider_repository_id: provider_id.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            visibility: Visibility::Private,
            default_branch: Some("main".to_string()),
            clone_url: format!("https://github.com/{owner}/{name}.git"),
            sync_enabled: false,
            sync_mode: SyncMode::Working,
            local_path: Some(format!("/tmp/local/{name}")),
            remote_updated_at: None,
        },
    )
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn discovery_upsert_preserves_local_state_and_reports_not_seen() {
    let (_dir, pool) = open_test_db().await;
    let account_id = make_account(&pool, "octocat").await;
    // Seen in discovery: local state must survive the refresh.
    let kept_id = seed_repo(&pool, account_id, "1", "octocat", "alpha").await;
    // Not in discovery: must be reported, never deleted.
    let unseen_id = seed_repo(&pool, account_id, "99", "octocat", "vanished").await;

    let server = MockServer::start().await;
    let provider = provider_for(&server);
    mount_sequence(
        &server,
        "/user/repos",
        vec![
            ResponseTemplate::new(200)
                .set_body_json(json!([repo_json(1, "octocat", "alpha")]))
                .insert_header("link", next_link(&server, 2)),
            ResponseTemplate::new(200).set_body_json(json!([{
                "id": 2,
                "name": "beta",
                "full_name": "octocat/beta",
                "owner": {"login": "octocat", "type": "User"},
                "private": true,
                "visibility": "private",
                "default_branch": "dev",
                "clone_url": "https://github.com/octocat/beta.git",
                "size": 7,
                "updated_at": "2026-11-18T01:00:00Z",
                "archived": false,
                "fork": true
            }])),
        ],
    )
    .await;

    let outcome = discover_and_sync(&pool, account_id, provider.discover_all_stream())
        .await
        .unwrap();
    assert_eq!(outcome.created, 1);
    assert_eq!(outcome.updated, 1);
    assert_eq!(outcome.not_seen, vec![unseen_id]);

    let repos = list_repositories(
        &pool,
        RepositoryFilter {
            account_id: Some(account_id),
            ..RepositoryFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(repos.len(), 3);

    // The refreshed row kept its local state.
    let kept = repos.iter().find(|r| r.id == kept_id).unwrap();
    assert!(!kept.sync_enabled);
    assert_eq!(kept.sync_mode, SyncMode::Working);
    assert_eq!(kept.local_path.as_deref(), Some("/tmp/local/alpha"));
    assert_eq!(kept.archive_status, ArchiveStatus::Pending);
    assert_eq!(kept.visibility, Visibility::Public);

    // The new row got the product defaults.
    let created = repos
        .iter()
        .find(|r| r.provider_repository_id == "2")
        .unwrap();
    assert!(created.sync_enabled);
    assert_eq!(created.sync_mode, SyncMode::Mirror);
    assert_eq!(created.default_branch.as_deref(), Some("dev"));
    assert!(created.local_path.is_none());

    // The unseen row is untouched.
    let unseen = repos.iter().find(|r| r.id == unseen_id).unwrap();
    assert_eq!(unseen.full_name, "octocat/vanished");
}

#[tokio::test]
async fn discovery_new_rows_use_default_sync_mode_setting() {
    let (_dir, pool) = open_test_db().await;
    let account_id = make_account(&pool, "octocat").await;
    set_setting(&pool, "sync.default_mode", "working")
        .await
        .unwrap();

    let server = MockServer::start().await;
    let provider = provider_for(&server);
    mount_sequence(
        &server,
        "/user/repos",
        vec![ResponseTemplate::new(200).set_body_json(json!([repo_json(1, "octocat", "alpha")]))],
    )
    .await;

    let outcome = discover_and_sync(&pool, account_id, provider.discover_all_stream())
        .await
        .unwrap();
    assert_eq!(outcome.created, 1);

    let repos = list_repositories(
        &pool,
        RepositoryFilter {
            account_id: Some(account_id),
            ..RepositoryFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].sync_mode, SyncMode::Working);
}

#[tokio::test]
async fn sync_installations_persists_and_refreshes() {
    let (_dir, pool) = open_test_db().await;
    let account_id = make_account(&pool, "octocat").await;

    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/installations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "installations": [installation_json(11, "octocat", "User", None)],
        })))
        .mount(&server)
        .await;

    let installations = provider.list_installations().await.unwrap();
    let stored = sync_installations(&pool, account_id, &installations)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].installation_id, "11");
    assert_eq!(
        stored[0].installation_status,
        reponyx_core::InstallationStatus::Active
    );
    assert_eq!(
        stored[0].permissions_json,
        r#"{"contents":"read","metadata":"read"}"#
    );

    // Second sync with a suspended installation refreshes in place.
    let mut suspended = installations[0].clone();
    suspended.suspended = true;
    suspended.permissions = json!({"contents": "write"});
    let stored = sync_installations(&pool, account_id, &[suspended])
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored[0].installation_status,
        reponyx_core::InstallationStatus::Suspended
    );
    assert_eq!(stored[0].permissions_json, r#"{"contents":"write"}"#);
    // Still one row, not two.
    assert_eq!(
        list_installations_for_account(&pool, account_id)
            .await
            .unwrap()
            .len(),
        1
    );
}
