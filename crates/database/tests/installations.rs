//! Installation repository integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use common::{make_account, open_test_db, pool};
use reponyx_core::{ErrorCode, InstallationStatus, InstallationType};
use reponyx_database::installations::{
    NewInstallation, create_installation, delete_installation, get_installation,
    get_installation_by_account_and_id, list_installations_for_account, set_installation_status,
    upsert_installation,
};
use uuid::Uuid;

fn new_installation(account_id: Uuid, installation_id: &str) -> NewInstallation {
    NewInstallation {
        account_id,
        installation_id: installation_id.to_string(),
        installation_type: InstallationType::User,
        permissions_json: r#"{"contents":"read"}"#.to_string(),
    }
}

#[tokio::test]
async fn create_and_get_round_trip() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let created = create_installation(pool(&db), new_installation(account.id, "999"))
        .await
        .unwrap();
    assert_eq!(created.account_id, account.id);
    assert_eq!(created.installation_id, "999");
    assert_eq!(created.installation_type, InstallationType::User);
    assert_eq!(created.installation_status, InstallationStatus::Active);
    assert_eq!(created.permissions_json, r#"{"contents":"read"}"#);
    let fetched = get_installation(pool(&db), created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id, created.id);
}

#[tokio::test]
async fn lookup_by_account_and_installation_id() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let created = create_installation(pool(&db), new_installation(account.id, "999"))
        .await
        .unwrap();
    let found = get_installation_by_account_and_id(pool(&db), account.id, "999")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, created.id);
    assert!(
        get_installation_by_account_and_id(pool(&db), account.id, "000")
            .await
            .unwrap()
            .is_none()
    );
    let other = make_account(pool(&db), "someone").await;
    assert!(
        get_installation_by_account_and_id(pool(&db), other.id, "999")
            .await
            .unwrap()
            .is_none(),
        "same installation id under another account is a different row"
    );
}

#[tokio::test]
async fn duplicate_installation_rejected() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    create_installation(pool(&db), new_installation(account.id, "999"))
        .await
        .unwrap();
    assert_eq!(
        create_installation(pool(&db), new_installation(account.id, "999"))
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );
    assert_eq!(
        list_installations_for_account(pool(&db), account.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn installation_status_transitions() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let created = create_installation(pool(&db), new_installation(account.id, "999"))
        .await
        .unwrap();
    let suspended = set_installation_status(pool(&db), created.id, InstallationStatus::Suspended)
        .await
        .unwrap();
    assert_eq!(suspended.installation_status, InstallationStatus::Suspended);
    let active = set_installation_status(pool(&db), created.id, InstallationStatus::Active)
        .await
        .unwrap();
    assert_eq!(active.installation_status, InstallationStatus::Active);
    let deleted = set_installation_status(pool(&db), created.id, InstallationStatus::Deleted)
        .await
        .unwrap();
    assert_eq!(deleted.installation_status, InstallationStatus::Deleted);
    assert_eq!(
        set_installation_status(pool(&db), created.id, InstallationStatus::Active)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput,
        "deleted is terminal"
    );
}

#[tokio::test]
async fn installation_rejects_missing_account_fk() {
    let (_dir, db) = open_test_db().await;
    assert_eq!(
        create_installation(pool(&db), new_installation(Uuid::new_v4(), "999"))
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn installation_permissions_must_be_json() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let mut bad = new_installation(account.id, "999");
    bad.permissions_json = "not json".to_string();
    assert_eq!(
        create_installation(pool(&db), bad).await.unwrap_err().code,
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn delete_installation_removes_row() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let created = create_installation(pool(&db), new_installation(account.id, "999"))
        .await
        .unwrap();
    assert!(delete_installation(pool(&db), created.id).await.unwrap());
    assert!(
        get_installation(pool(&db), created.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn upsert_installation_creates_then_refreshes_in_place() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let first = upsert_installation(
        pool(&db),
        new_installation(account.id, "999"),
        InstallationStatus::Active,
    )
    .await
    .unwrap();
    assert_eq!(first.installation_status, InstallationStatus::Active);
    assert_eq!(first.permissions_json, r#"{"contents":"read"}"#);
    let mut refreshed = new_installation(account.id, "999");
    refreshed.permissions_json = r#"{"contents":"write"}"#.to_string();
    let second = upsert_installation(pool(&db), refreshed, InstallationStatus::Suspended)
        .await
        .unwrap();
    // Same row (same id), new permissions and status.
    assert_eq!(second.id, first.id);
    assert_eq!(second.installation_status, InstallationStatus::Suspended);
    assert_eq!(second.permissions_json, r#"{"contents":"write"}"#);
    assert_eq!(
        list_installations_for_account(pool(&db), account.id)
            .await
            .unwrap()
            .len(),
        1
    );
}
