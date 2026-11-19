//! Rate-limit tests against a mocked GitHub API (wiremock).
//!
//! The mock returns a rate-limit response mid-pagination, then
//! succeeds; discovery must back off and resume from the same cursor
//! and still yield every repository in order. All tokens are fake.

#[path = "common.rs"]
mod common;

use chrono::Utc;
use common::{mount_sequence, next_link, provider_for, repo_json};
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::client::GitHubClient;
use reponyx_github_provider::credentials::static_installation_token;
use serde_json::json;
use std::time::Duration;
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
async fn rate_limit_mid_pagination_backs_off_and_resumes() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    let reset_epoch = Utc::now().timestamp() + 2;
    mount_sequence(
        &server,
        "/user/repos",
        vec![
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset_epoch.to_string())
                .set_body_json(json!({"message": "API rate limit exceeded"})),
            page_response(
                &server,
                vec![
                    repo_json(1, "octocat", "alpha"),
                    repo_json(2, "octocat", "beta"),
                ],
                Some(2),
            ),
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
    assert_eq!(all[0].provider_repository_id, "1");
    assert_eq!(all[1].provider_repository_id, "2");
    assert_eq!(all[2].provider_repository_id, "3");
    // The rate-limited URL was hit twice (429, then success).
    // Note: `per_page` contains the substring `page=`, so the filter
    // must look for a real `page` parameter, not the substring.
    let hits = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| {
            r.url.path() == "/user/repos"
                && r.url
                    .query()
                    .is_some_and(|q| !q.contains("&page=") && !q.starts_with("page="))
        })
        .count();
    assert_eq!(hits, 2);
}

#[tokio::test]
async fn rate_limit_state_is_exposed_to_the_caller() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([repo_json(1, "octocat", "alpha")]))
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", "4999")
                .insert_header("x-ratelimit-reset", "1800000000"),
        )
        .mount(&server)
        .await;

    provider.discover_repositories(None).await.unwrap();
    let state = provider.rate_limit().await;
    assert_eq!(state.limit, Some(5000));
    assert_eq!(state.remaining, Some(4999));
    assert!(state.reset_at.is_some());
}

#[tokio::test]
async fn wait_beyond_the_cap_returns_rate_limit_exceeded() {
    let server = MockServer::start().await;
    let client = GitHubClient::new(&server.uri(), static_installation_token(common::TOKEN))
        .unwrap()
        .with_max_rate_limit_wait(Duration::from_secs(0));
    let provider = GitHubProvider::with_client(client);
    let reset_epoch = Utc::now().timestamp() + 3600;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "60")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset_epoch.to_string())
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;

    // Must return immediately (no 60 s sleep) with the reset time.
    let started = std::time::Instant::now();
    let err = provider.discover_repositories(None).await.unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(err.code, reponyx_core::ErrorCode::RateLimitExceeded);
    let details = err.details.as_deref().unwrap_or("");
    assert!(details.contains("resets at"), "details: {details}");
}

#[tokio::test]
async fn forbidden_rate_limit_is_retried_like_429() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    mount_sequence(
        &server,
        "/user/repos",
        vec![
            ResponseTemplate::new(403)
                .insert_header("retry-after", "1")
                .insert_header("x-ratelimit-remaining", "0")
                .set_body_json(json!({"message": "API rate limit exceeded for user ID 42."})),
            page_response(&server, vec![repo_json(1, "octocat", "alpha")], None),
        ],
    )
    .await;

    let page = provider.discover_repositories(None).await.unwrap();
    assert_eq!(page.repositories.len(), 1);
}
