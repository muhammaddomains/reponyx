//! Discovery tests against a mocked GitHub API (wiremock).
//!
//! Multi-page sequences use [`mount_sequence`] (wiremock-rs has no
//! scenarios): one mock answers successive requests with queued
//! responses, in order. All tokens are fake.

#[path = "common.rs"]
mod common;

use common::{TOKEN, mount_sequence, next_link, provider_for, repo_json, user_json};
use reponyx_core::Visibility;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn page_response(
    server: &MockServer,
    repos: Vec<serde_json::Value>,
    next_page: Option<u32>,
) -> ResponseTemplate {
    let mut template = ResponseTemplate::new(200).set_body_json(json!(repos));
    if let Some(page) = next_page {
        template = template.insert_header("link", next_link(server, page));
    }
    template
}

#[tokio::test]
async fn multi_page_discovery_yields_all_repos_in_order() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    mount_sequence(
        &server,
        "/user/repos",
        vec![
            page_response(
                &server,
                vec![
                    repo_json(1, "octocat", "alpha"),
                    repo_json(2, "octocat", "beta"),
                ],
                Some(2),
            ),
            page_response(&server, vec![repo_json(3, "acme", "gamma")], Some(3)),
            page_response(&server, vec![repo_json(4, "acme", "delta")], None),
        ],
    )
    .await;

    let mut stream = provider.discover_all_stream();
    let mut all = Vec::new();
    let mut batches = Vec::new();
    while let Some(page) = stream.recv().await {
        let page = page.unwrap();
        batches.push(page.len());
        all.extend(page);
    }
    assert_eq!(batches, vec![2, 1, 1]);
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].provider_repository_id, "1");
    assert_eq!(all[1].full_name, "octocat/beta");
    assert_eq!(all[2].full_name, "acme/gamma");
    assert_eq!(all[3].full_name, "acme/delta");
    assert_eq!(all[0].visibility, Visibility::Public);
    assert_eq!(all[0].default_branch.as_deref(), Some("main"));
    assert_eq!(all[0].clone_url, "https://github.com/octocat/alpha.git");
    assert_eq!(all[0].size_kb, Some(10));
    assert!(!all[0].archived);
    assert!(!all[0].fork);
}

#[tokio::test]
async fn empty_page_does_not_end_discovery() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    mount_sequence(
        &server,
        "/user/repos",
        vec![
            page_response(
                &server,
                vec![
                    repo_json(1, "octocat", "alpha"),
                    repo_json(2, "octocat", "beta"),
                ],
                Some(2),
            ),
            page_response(&server, vec![], Some(3)),
            page_response(&server, vec![repo_json(3, "acme", "gamma")], None),
        ],
    )
    .await;

    let mut stream = provider.discover_all_stream();
    let mut all = Vec::new();
    while let Some(page) = stream.recv().await {
        all.extend(page.unwrap());
    }
    assert_eq!(all.len(), 3);
    assert_eq!(all[2].provider_repository_id, "3");
}

#[tokio::test]
async fn single_page_without_link_header_is_complete() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(page_response(
            &server,
            vec![repo_json(1, "octocat", "alpha")],
            None,
        ))
        .mount(&server)
        .await;

    let page = provider.discover_repositories(None).await.unwrap();
    assert_eq!(page.repositories.len(), 1);
    assert!(!page.has_next_page);
    assert!(page.next_cursor.is_none());
}

#[tokio::test]
async fn malformed_link_header_is_treated_as_last_page() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([repo_json(1, "octocat", "alpha")]))
                .insert_header("link", "garbage, no angles here"),
        )
        .mount(&server)
        .await;

    let page = provider.discover_repositories(None).await.unwrap();
    assert_eq!(page.repositories.len(), 1);
    assert!(!page.has_next_page);
}

#[tokio::test]
async fn foreign_cursor_is_rejected() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    let err = provider
        .discover_repositories(Some("https://evil.example.com/user/repos?page=2"))
        .await
        .unwrap_err();
    assert_eq!(err.code, reponyx_core::ErrorCode::InvalidInput);
    // The mock was never hit.
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn validate_auth_returns_account_identity() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user_json()))
        .mount(&server)
        .await;

    let account = provider.validate_auth().await.unwrap();
    assert_eq!(account.provider_account_id, "42");
    assert_eq!(account.username, "octocat");
    assert_eq!(account.display_name.as_deref(), Some("The Octocat"));
    assert!(
        account
            .avatar_url
            .unwrap()
            .contains("avatars.githubusercontent.com")
    );
}

#[tokio::test]
async fn validate_auth_rejects_bad_credentials() {
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
    assert_eq!(err.code, reponyx_core::ErrorCode::AuthenticationExpired);
    // The token never appears in the error.
    assert!(!format!("{err:?}").contains(TOKEN));
    assert!(!err.to_string().contains(TOKEN));
}

#[tokio::test]
async fn auth_header_is_sent_per_kind() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {TOKEN}"),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(user_json()))
        .expect(1)
        .mount(&server)
        .await;

    provider.validate_auth().await.unwrap();
}
