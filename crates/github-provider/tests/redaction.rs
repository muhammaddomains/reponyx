//! Token-redaction tests: a token must never appear in `Debug`
//! output, error messages, or log-shaped strings. All tokens here are
//! obviously fake.

#[path = "common.rs"]
mod common;

use common::{TOKEN, provider_for};
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::credentials::{
    CredentialKind, Token, static_installation_token, static_pat,
};
use reponyx_provider_abstractions::ShortLivedCredential;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn short_lived_credential_debug_redacts_the_token() {
    let credential = ShortLivedCredential {
        username: "x-access-token".to_string(),
        token: "test-token-secret-value".to_string(),
        expires_in_secs: Some(3600),
    };
    let debug = format!("{credential:?}");
    assert!(!debug.contains("test-token-secret-value"));
    assert!(debug.contains("<redacted>"));
    assert!(debug.contains("x-access-token"));
}

#[test]
fn token_debug_redacts_the_value_for_both_kinds() {
    for (kind, value) in [
        (CredentialKind::InstallationToken, "test-installation-token"),
        (CredentialKind::Pat, "test-pat-token"),
    ] {
        let token = Token::new(value, kind, "octocat", None);
        let debug = format!("{token:?}");
        assert!(!debug.contains(value), "{kind:?} leaked in Debug");
        assert!(debug.contains("<redacted>"));
    }
}

#[tokio::test]
async fn acquire_credential_debug_redacts_the_token() {
    let provider = GitHubProvider::new(
        "https://api.github.com",
        static_installation_token("test-token-acquire-me-not"),
    )
    .unwrap();
    let credential = provider.acquire_credential().await.unwrap();
    assert_eq!(credential.username, "x-access-token");
    assert_eq!(credential.expires_in_secs, Some(3600));
    assert!(!format!("{credential:?}").contains("test-token-acquire-me-not"));
}

#[tokio::test]
async fn pat_acquire_credential_uses_the_github_login() {
    let provider = GitHubProvider::new(
        "https://api.github.com",
        static_pat("test-pat-value", "octocat"),
    )
    .unwrap();
    let credential = provider.acquire_credential().await.unwrap();
    assert_eq!(credential.username, "octocat");
    assert_eq!(credential.token, "test-pat-value");
    assert_eq!(credential.expires_in_secs, None);
}

#[tokio::test]
async fn error_from_a_failed_request_never_contains_the_token() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;

    let err = provider.validate_auth().await.unwrap_err();
    let debug = format!("{err:?}");
    let display = err.to_string();
    let details = err.details.as_deref().unwrap_or("");
    assert!(!debug.contains(TOKEN));
    assert!(!display.contains(TOKEN));
    assert!(!details.contains(TOKEN));
}

#[tokio::test]
async fn client_debug_does_not_contain_the_token() {
    let provider = GitHubProvider::new(
        "https://api.github.com",
        static_installation_token("test-token-in-client-debug"),
    )
    .unwrap();
    assert!(!format!("{provider:?}").contains("test-token-in-client-debug"));
}
