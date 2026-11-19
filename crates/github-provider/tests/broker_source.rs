//! `BrokerTokenSource` and `BrokerClient` tests against a mocked
//! Auth Broker (wiremock) and a mocked GitHub API. All tokens are
//! obviously fake (`test-…`); no real credentials, no network to
//! github.com or to a real broker.

use chrono::{Duration, Utc};
use reponyx_core::ErrorCode;
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::broker::{BrokerClient, BrokerTokenSource};
use reponyx_github_provider::credentials::CredentialSource;
use serde_json::json;
use std::sync::Arc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INSTALLATION_ID: &str = "777";
const DEVICE_TOKEN: &str = "test-device-token-abc";
const MINTED_TOKEN: &str = "test-installation-token-xyz";

fn mint_response(token: &str) -> ResponseTemplate {
    let expires_at = (Utc::now() + Duration::seconds(3600)).to_rfc3339();
    ResponseTemplate::new(200).set_body_json(json!({
        "token": token,
        "expires_at": expires_at,
    }))
}

async fn mock_broker_mint(server: &MockServer, token: &str) {
    Mock::given(method("POST"))
        .and(path("/installations/777/token"))
        .and(header("authorization", format!("Bearer {DEVICE_TOKEN}")))
        .respond_with(mint_response(token))
        .mount(server)
        .await;
}

fn user_response(login: &str, id: u64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": id,
        "login": login,
        "name": login,
        "type": "User",
    }))
}

#[tokio::test]
async fn broker_source_caches_until_invalidate() {
    let broker = MockServer::start().await;
    mock_broker_mint(&broker, MINTED_TOKEN).await;
    let source = BrokerTokenSource::new(&broker.uri(), INSTALLATION_ID, DEVICE_TOKEN).unwrap();

    let first = source.token().await.unwrap();
    assert_eq!(first.expose_value(), MINTED_TOKEN);
    assert_eq!(first.username, "x-access-token");
    assert_eq!(
        first.authorization_header(),
        format!("Bearer {MINTED_TOKEN}")
    );
    // Second call is served from the cache: no second broker request.
    let second = source.token().await.unwrap();
    assert_eq!(second.expose_value(), MINTED_TOKEN);
    assert_eq!(broker.received_requests().await.unwrap().len(), 1);

    // Invalidate drops the cache: the next call mints again.
    source.invalidate().await;
    let third = source.token().await.unwrap();
    assert_eq!(third.expose_value(), MINTED_TOKEN);
    assert_eq!(broker.received_requests().await.unwrap().len(), 2);
    // Debug never shows the device token.
    assert!(!format!("{source:?}").contains(DEVICE_TOKEN));
}

#[tokio::test]
async fn broker_source_surfaces_broker_errors() {
    let broker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/installations/777/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "code": "AuthenticationExpired", "message": "invalid or revoked device token" },
        })))
        .mount(&broker)
        .await;
    let source = BrokerTokenSource::new(&broker.uri(), INSTALLATION_ID, DEVICE_TOKEN).unwrap();
    let err = source.token().await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationExpired);
    assert!(!err.message.contains(DEVICE_TOKEN));
}

#[tokio::test]
async fn broker_client_rejects_a_bad_base_url() {
    assert!(BrokerClient::new("not-a-url").is_err());
    assert!(BrokerClient::new("ftp://example.invalid").is_err());
    assert!(BrokerClient::new("http://127.0.0.1:8765/").is_ok());
}

#[path = "common.rs"]
mod common;

use common::SequencedResponder;

#[tokio::test]
async fn client_refreshes_once_on_401_then_succeeds() {
    // GitHub API: the first GET /user is 401 (stale cached token),
    // the retry after refresh is 200.
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(SequencedResponder::new(vec![
            ResponseTemplate::new(401).set_body_json(json!({
                "message": "Bad credentials",
            })),
            user_response("octocat", 42),
        ]))
        .mount(&github)
        .await;
    // The broker mints a different token after the invalidate: the
    // first mint returns the stale token, the second the fresh one.
    let broker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/installations/777/token"))
        .respond_with(SequencedResponder::new(vec![
            mint_response(MINTED_TOKEN),
            mint_response("test-installation-token-refreshed"),
        ]))
        .mount(&broker)
        .await;
    let source = BrokerTokenSource::new(&broker.uri(), INSTALLATION_ID, DEVICE_TOKEN).unwrap();
    let provider = GitHubProvider::new(&github.uri(), Arc::new(source)).unwrap();
    let info = provider.validate_auth().await.unwrap();
    assert_eq!(info.username, "octocat");
    // Exactly two attempts on each side: original + one refresh.
    assert_eq!(github.received_requests().await.unwrap().len(), 2);
    assert_eq!(broker.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn client_does_not_retry_forever_on_repeated_401() {
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "message": "Bad credentials",
        })))
        .mount(&github)
        .await;
    let broker = MockServer::start().await;
    mock_broker_mint(&broker, MINTED_TOKEN).await;
    let source = BrokerTokenSource::new(&broker.uri(), INSTALLATION_ID, DEVICE_TOKEN).unwrap();
    let provider = GitHubProvider::new(&github.uri(), Arc::new(source)).unwrap();
    let err = provider.validate_auth().await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationExpired);
    // Exactly two attempts: the original plus one refresh-and-retry.
    assert_eq!(github.received_requests().await.unwrap().len(), 2);
}
