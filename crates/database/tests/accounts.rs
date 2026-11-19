//! Account repository integration tests (temp SQLite).

#[path = "common.rs"]
mod common;

use common::{make_account, make_repository, open_test_db, pool};
use reponyx_core::InstallationType;
use reponyx_core::{AccountStatus, AccountType, AuthenticationMethod, ErrorCode, Provider};
use reponyx_database::accounts::{
    AccountPatch, NewAccount, count_accounts, create_account, delete_account, get_account,
    get_account_by_provider, list_accounts, reconnect_account, set_account_status, update_account,
};
use reponyx_database::installations::{NewInstallation, create_installation, get_installation};

fn new_account(username: &str) -> NewAccount {
    NewAccount {
        provider: Provider::GitHub,
        provider_account_id: format!("prov-{username}"),
        username: username.to_string(),
        display_name: Some(format!("{username} display")),
        label: None,
        account_type: AccountType::Personal,
        authentication_method: AuthenticationMethod::FineGrainedPat,
    }
}

#[tokio::test]
async fn create_and_get_round_trip() {
    let (_dir, db) = open_test_db().await;
    let created = create_account(pool(&db), new_account("octocat"))
        .await
        .unwrap();
    assert_eq!(created.username, "octocat");
    assert_eq!(created.provider, Provider::GitHub);
    assert_eq!(created.status, AccountStatus::Connected);
    assert_eq!(created.account_type, AccountType::Personal);
    assert_eq!(created.display_name.as_deref(), Some("octocat display"));
    assert!(created.last_validated_at.is_none());

    let fetched = get_account(pool(&db), created.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.provider_account_id, "prov-octocat");
    assert!(
        get_account(pool(&db), uuid::Uuid::new_v4())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn create_validates_empty_fields() {
    let (_dir, db) = open_test_db().await;
    let mut bad = new_account("x");
    bad.username = "   ".to_string();
    let err = create_account(pool(&db), bad).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);

    let mut bad = new_account("y");
    bad.provider_account_id = String::new();
    let err = create_account(pool(&db), bad).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn duplicate_provider_identity_rejected() {
    let (_dir, db) = open_test_db().await;
    create_account(pool(&db), new_account("octocat"))
        .await
        .unwrap();
    let err = create_account(pool(&db), new_account("octocat"))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(count_accounts(pool(&db)).await.unwrap(), 1);
}

#[tokio::test]
async fn get_by_provider_identity() {
    let (_dir, db) = open_test_db().await;
    let created = create_account(pool(&db), new_account("octocat"))
        .await
        .unwrap();
    let found = get_account_by_provider(pool(&db), Provider::GitHub, "prov-octocat")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, created.id);
    assert!(
        get_account_by_provider(pool(&db), Provider::GitHub, "prov-nobody")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn update_account_patch() {
    let (_dir, db) = open_test_db().await;
    let created = create_account(pool(&db), new_account("octocat"))
        .await
        .unwrap();
    let updated = update_account(
        pool(&db),
        created.id,
        AccountPatch {
            display_name: Some(None), // clear it
            label: Some(Some("work".to_string())),
            account_type: Some(AccountType::Organization),
        },
    )
    .await
    .unwrap();
    assert!(updated.display_name.is_none());
    assert_eq!(updated.label.as_deref(), Some("work"));
    assert_eq!(updated.account_type, AccountType::Organization);
    // Username is untouched by the patch.
    assert_eq!(updated.username, "octocat");
}

#[tokio::test]
async fn status_transitions_and_reconnect() {
    let (_dir, db) = open_test_db().await;
    let account = create_account(pool(&db), new_account("octocat"))
        .await
        .unwrap();

    // connected -> paused -> connected is a normal transition.
    let paused = set_account_status(pool(&db), account.id, AccountStatus::Paused)
        .await
        .unwrap();
    assert_eq!(paused.status, AccountStatus::Paused);
    let back = set_account_status(pool(&db), account.id, AccountStatus::Connected)
        .await
        .unwrap();
    assert_eq!(back.status, AccountStatus::Connected);

    // connected -> revoked, then revoked is terminal-ish.
    set_account_status(pool(&db), account.id, AccountStatus::Revoked)
        .await
        .unwrap();
    let err = set_account_status(pool(&db), account.id, AccountStatus::Paused)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    let err = set_account_status(pool(&db), account.id, AccountStatus::Connected)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);

    // Explicit reconnect restores it and stamps last_validated_at.
    let reconnected = reconnect_account(pool(&db), account.id).await.unwrap();
    assert_eq!(reconnected.status, AccountStatus::Connected);
    assert!(reconnected.last_validated_at.is_some());

    // Reconnect from connected is rejected (nothing to reconnect).
    let err = reconnect_account(pool(&db), account.id).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);

    // token_expired -> connected needs reconnect too.
    set_account_status(pool(&db), account.id, AccountStatus::TokenExpired)
        .await
        .unwrap();
    let err = set_account_status(pool(&db), account.id, AccountStatus::Connected)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    let reconnected = reconnect_account(pool(&db), account.id).await.unwrap();
    assert_eq!(reconnected.status, AccountStatus::Connected);
}

#[tokio::test]
async fn list_and_count() {
    let (_dir, db) = open_test_db().await;
    assert_eq!(count_accounts(pool(&db)).await.unwrap(), 0);
    create_account(pool(&db), new_account("alice"))
        .await
        .unwrap();
    create_account(pool(&db), new_account("bob")).await.unwrap();
    assert_eq!(count_accounts(pool(&db)).await.unwrap(), 2);
    let listed = list_accounts(pool(&db)).await.unwrap();
    assert_eq!(listed.len(), 2);
    // Ordered by username.
    assert_eq!(listed[0].username, "alice");
    assert_eq!(listed[1].username, "bob");
}

#[tokio::test]
async fn delete_account_cascades_to_children() {
    let (_dir, db) = open_test_db().await;
    let account = make_account(pool(&db), "octocat").await;
    let repo = make_repository(pool(&db), account.id, "456", "octocat", "hello").await;
    let installation = create_installation(
        pool(&db),
        NewInstallation {
            account_id: account.id,
            installation_id: "999".to_string(),
            installation_type: InstallationType::User,
            permissions_json: "{}".to_string(),
        },
    )
    .await
    .unwrap();

    assert!(delete_account(pool(&db), account.id).await.unwrap());
    assert!(get_account(pool(&db), account.id).await.unwrap().is_none());
    assert!(
        reponyx_database::repositories::get_repository(pool(&db), repo.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        get_installation(pool(&db), installation.id)
            .await
            .unwrap()
            .is_none()
    );
    // Deleting again reports nothing deleted.
    assert!(!delete_account(pool(&db), account.id).await.unwrap());
}
