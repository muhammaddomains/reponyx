//! PAT account flow tests against a mocked GitHub API (wiremock)
//! and a temp SQLite database. All tokens are obviously fake
//! (`test-pat-…`); no real credentials, no network to github.com.

use chrono::{Duration, Utc};
use reponyx_core::{
    AccountStatus, AccountType, AuthenticationMethod, ErrorCode, SyncMode, Visibility,
};
use reponyx_credential_store::{
    PatExpiryStatus, PatInput, PatTokenSource, SERVICE_PAT, SecretString, add_pat_account,
    delete_pat_account, memory_store, pat_expiry_status, reconnect_pat_account, remove_pat,
    replace_pat, validate_pat_account,
};
use reponyx_database::{
    accounts::{get_account, get_account_by_provider, list_accounts, set_account_status},
    open_database,
    repositories::{NewRepository, create_repository, get_repository},
};
use reponyx_github_provider::credentials::CredentialSource;
use serde_json::json;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Obviously fake PATs in real GitHub format (classic `ghp_` +
/// 36 chars). All zeros / a trailing digit: never a real token.
const PAT_OCTOCAT: &str = "ghp_000000000000000000000000000000000001";
const PAT_BAD: &str = "ghp_000000000000000000000000000000000002";
const PAT_OTHER: &str = "ghp_000000000000000000000000000000000003";

async fn temp_db() -> (TempDir, SqlitePool) {
    let dir = TempDir::new().unwrap();
    let db = open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    (dir, db.pool)
}

/// `GET /user` returning `octocat` (id 42).
async fn mock_user_ok(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("token {PAT_OCTOCAT}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42,
            "login": "octocat",
            "name": "The Octocat",
            "avatar_url": "https://example.invalid/avatar.png",
            "type": "User",
        })))
        .mount(server)
        .await;
}

async fn mock_user_status(server: &MockServer, status: u16, token: &str) {
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("token {token}")))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "message": if status == 401 { "Bad credentials" } else { "Forbidden" },
        })))
        .mount(server)
        .await;
}

fn pat_input(pat: &str) -> PatInput {
    PatInput {
        pat: SecretString::new(pat),
        label: Some("test".to_string()),
        account_type: AccountType::Personal,
        expires_at: None,
    }
}

#[tokio::test]
async fn add_pat_account_validates_stores_and_creates() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();

    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();

    assert_eq!(account.username, "octocat");
    assert_eq!(account.provider_account_id, "42");
    assert_eq!(account.status, AccountStatus::Connected);
    assert_eq!(
        account.authentication_method,
        AuthenticationMethod::FineGrainedPat
    );
    // The PAT is in the store…
    assert_eq!(
        store
            .get(SERVICE_PAT, &account.id.to_string())
            .unwrap()
            .unwrap()
            .expose(),
        PAT_OCTOCAT
    );
    // …and nowhere in the database: no column of any row contains it.
    let rows = sqlx::query("SELECT * FROM accounts")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    for row in &rows {
        for i in 0..row.len() {
            if let Ok(value) = row.try_get::<String, _>(i) {
                assert!(!value.contains(PAT_OCTOCAT), "PAT leaked into the database");
            }
        }
    }
}

#[tokio::test]
async fn add_rejects_bad_pat_format_without_network() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    let store = memory_store();
    let err = add_pat_account(&pool, &store, &server.uri(), pat_input("not-a-token"))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(list_accounts(&pool).await.unwrap().len(), 0);
    assert!(store.get(SERVICE_PAT, "anything").unwrap().is_none());
    // No request ever reached the mock server.
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn add_with_401_stores_nothing_and_fails_once() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_status(&server, 401, PAT_BAD).await;
    let store = memory_store();
    let err = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_BAD))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationExpired);
    assert_eq!(list_accounts(&pool).await.unwrap().len(), 0);
    // Two attempts, not one: the client refreshes once after a 401
    // and retries a single time, then fails fast. Never more.
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn add_duplicate_identity_is_rejected() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();
    let err = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(list_accounts(&pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn replace_pat_requires_same_github_identity() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();

    // Same identity, new token: stored value is replaced.
    let replaced = replace_pat(
        &pool,
        &store,
        &server.uri(),
        account.id,
        SecretString::new(PAT_OCTOCAT),
        None,
    )
    .await
    .unwrap();
    assert_eq!(replaced.id, account.id);

    // A PAT for a *different* GitHub identity is rejected, and the
    // stored PAT is untouched.
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("token {PAT_OTHER}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 99, "login": "someone-else", "type": "User",
        })))
        .mount(&server)
        .await;
    let err = replace_pat(
        &pool,
        &store,
        &server.uri(),
        account.id,
        SecretString::new(PAT_OTHER),
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(
        store
            .get(SERVICE_PAT, &account.id.to_string())
            .unwrap()
            .unwrap()
            .expose(),
        PAT_OCTOCAT
    );
}

#[tokio::test]
async fn remove_pat_deletes_secret_and_revokes_account() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();

    let removed = remove_pat(&pool, &store, account.id).await.unwrap();
    assert_eq!(removed.status, AccountStatus::Revoked);
    assert_eq!(
        store.get(SERVICE_PAT, &account.id.to_string()).unwrap(),
        None
    );
    // The account row itself stays (local data is never deleted).
    assert!(get_account(&pool, account.id).await.unwrap().is_some());
}

#[tokio::test]
async fn delete_pat_account_removes_row_secret_and_child_repositories() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();
    let repository = create_repository(
        &pool,
        NewRepository {
            account_id: account.id,
            provider_repository_id: "123".to_string(),
            owner: "octocat".to_string(),
            name: "hello-world".to_string(),
            visibility: Visibility::Public,
            default_branch: Some("main".to_string()),
            clone_url: "https://github.com/octocat/hello-world.git".to_string(),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap();

    assert!(delete_pat_account(&pool, &store, account.id).await.unwrap());
    assert_eq!(
        store.get(SERVICE_PAT, &account.id.to_string()).unwrap(),
        None
    );
    // The account row is gone, and its repository went with it
    // via ON DELETE CASCADE.
    assert!(get_account(&pool, account.id).await.unwrap().is_none());
    assert_eq!(list_accounts(&pool).await.unwrap().len(), 0);
    assert!(
        get_repository(&pool, repository.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn delete_pat_account_works_after_remove() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();
    // Revoke first: the PAT is already gone from the store, and
    // deleting the account must still succeed.
    let removed = remove_pat(&pool, &store, account.id).await.unwrap();
    assert_eq!(removed.status, AccountStatus::Revoked);
    assert!(delete_pat_account(&pool, &store, account.id).await.unwrap());
    assert!(get_account(&pool, account.id).await.unwrap().is_none());
}

#[tokio::test]
async fn reconnect_restores_expired_account_and_401_re_expires_it() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();

    set_account_status(&pool, account.id, AccountStatus::TokenExpired)
        .await
        .unwrap();
    let reconnected = reconnect_pat_account(&pool, &store, &server.uri(), account.id)
        .await
        .unwrap();
    assert_eq!(reconnected.status, AccountStatus::Connected);

    // Now the stored PAT is rejected by GitHub: reconnect fails with
    // AuthenticationExpired and the account is TokenExpired again.
    // A second mock server answers 401 for the same PAT.
    let server2 = MockServer::start().await;
    mock_user_status(&server2, 401, PAT_OCTOCAT).await;
    let err = reconnect_pat_account(&pool, &store, &server2.uri(), account.id)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationExpired);
    assert_eq!(
        get_account(&pool, account.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        AccountStatus::TokenExpired
    );
}

#[tokio::test]
async fn validate_refreshes_last_validated_at() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let account = add_pat_account(&pool, &store, &server.uri(), pat_input(PAT_OCTOCAT))
        .await
        .unwrap();
    let info = validate_pat_account(&pool, &store, &server.uri(), account.id)
        .await
        .unwrap();
    assert_eq!(info.username, "octocat");
    let after = get_account(&pool, account.id).await.unwrap().unwrap();
    assert!(after.last_validated_at.is_some());
}

#[tokio::test]
async fn expiry_status_is_computed_from_stored_expiry() {
    let (_dir, pool) = temp_db().await;
    let server = MockServer::start().await;
    mock_user_ok(&server).await;
    let store = memory_store();
    let mut input = pat_input(PAT_OCTOCAT);
    input.expires_at = Some(Utc::now() + Duration::days(3));
    let account = add_pat_account(&pool, &store, &server.uri(), input)
        .await
        .unwrap();
    assert_eq!(
        pat_expiry_status(&account, Utc::now()),
        PatExpiryStatus::ExpiringSoon { days_left: 3 }
    );
    // The stored row round-trips the expiry as well.
    let stored = get_account_by_provider(&pool, reponyx_core::Provider::GitHub, "42")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pat_expiry_status(&stored, Utc::now()),
        PatExpiryStatus::ExpiringSoon { days_left: 3 }
    );
}

#[tokio::test]
async fn pat_token_source_reads_from_the_store() {
    let store = memory_store();
    let account_id = Uuid::new_v4();
    let source = PatTokenSource::new(
        Arc::clone(&store),
        account_id,
        "octocat",
        Some(Utc::now() + Duration::days(30)),
    );
    // No PAT stored yet: AuthenticationExpired, not a bare None.
    assert_eq!(
        source.token().await.unwrap_err().code,
        ErrorCode::AuthenticationExpired
    );
    store
        .store(SERVICE_PAT, &account_id.to_string(), "test-pat-from-store")
        .unwrap();
    let token = source.token().await.unwrap();
    assert_eq!(token.expose_value(), "test-pat-from-store");
    assert_eq!(token.username, "octocat");
    assert_eq!(token.authorization_header(), "token test-pat-from-store");
    assert!(token.expires_in_secs.is_some());
    // The token never appears in Debug output.
    assert!(!format!("{token:?}").contains("test-pat-from-store"));
}
