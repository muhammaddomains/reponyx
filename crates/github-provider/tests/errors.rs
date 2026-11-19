//! Error-mapping tests against a mocked GitHub API (wiremock).
//!
//! Every status maps to the documented `RepoNyxError` code, permanent
//! failures are never retried, and transient failures are retried
//! exactly `max_attempts` times. All tokens are fake.

#[path = "common.rs"]
mod common;

use common::{TOKEN, provider_for};
use reponyx_core::ErrorCode;
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::credentials::{FailingTokenSource, static_installation_token};
use serde_json::json;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn error_response(status: u16, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({"message": message}))
}

#[tokio::test]
async fn unauthorized_is_authentication_expired_and_not_retried() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(error_response(401, "Bad credentials"))
        .expect(1)
        .mount(&server)
        .await;

    let err = provider.discover_repositories(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationExpired);
    assert!(!err.is_retryable());
    assert!(!format!("{err:?}").contains(TOKEN));
}

#[tokio::test]
async fn forbidden_revoked_is_authentication_revoked() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(error_response(403, "The installation has been suspended."))
        .mount(&server)
        .await;

    let err = provider.discover_repositories(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationRevoked);
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn forbidden_insufficient_is_insufficient_permissions() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(error_response(
            403,
            "Resource not accessible by integration",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = provider.discover_repositories(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InsufficientPermissions);
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn not_found_on_a_repo_is_repository_not_found() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/repos/octocat/missing"))
        .respond_with(error_response(404, "Not Found"))
        .mount(&server)
        .await;

    let err = provider
        .check_repository_access("octocat", "missing")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RepositoryNotFound);
}

#[tokio::test]
async fn server_error_is_retried_then_reported_as_internal_error() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(error_response(500, "Internal Server Error"))
        .expect(3)
        .mount(&server)
        .await;

    let err = provider.discover_repositories(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InternalError);
    assert!(err.to_string().contains("500"));
    // Exactly three attempts: the default max_attempts.
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn unreachable_api_is_network_unavailable() {
    // Port 1 is closed: connection refused immediately.
    let provider =
        GitHubProvider::new("http://127.0.0.1:1", static_installation_token(TOKEN)).unwrap();
    let err = provider.discover_repositories(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NetworkUnavailable);
}

#[tokio::test]
async fn failing_token_source_surfaces_credential_store_error() {
    let server = MockServer::start().await;
    let provider = GitHubProvider::new(
        &server.uri(),
        Arc::new(FailingTokenSource::new("test credential store unavailable")),
    )
    .unwrap();
    let err = provider.discover_repositories(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::CredentialStoreError);
    // The API was never contacted.
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn invalid_base_url_is_rejected_at_construction() {
    let err = GitHubProvider::new("api.github.com", static_installation_token(TOKEN)).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}
