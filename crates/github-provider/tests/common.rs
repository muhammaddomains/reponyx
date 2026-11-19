#![allow(dead_code)]
//! Shared helpers for github-provider integration tests: a mocked
//! GitHub API (wiremock) plus JSON fixture builders. All tokens are
//! obviously fake (`test-token-...`); no real credentials, no network
//! access to github.com.
//!
//! wiremock-rs has no scenarios, so stateful multi-response sequences
//! (pagination, rate-limit-then-success) use [`SequencedResponder`]:
//! one mock that answers successive requests with queued responses,
//! in order.

use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::credentials::static_installation_token;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const TOKEN: &str = "test-token-do-not-use";

pub fn provider_for(server: &MockServer) -> GitHubProvider {
    GitHubProvider::new(&server.uri(), static_installation_token(TOKEN)).unwrap()
}

/// A stateful responder that returns a queued response per request,
/// in order. Once the queue is down to its last response it keeps
/// returning that one, so an unexpected extra request fails loudly
/// in the test assertions instead of hanging.
#[derive(Clone)]
pub struct SequencedResponder {
    responses: Arc<Mutex<VecDeque<ResponseTemplate>>>,
}

impl SequencedResponder {
    pub fn new(responses: Vec<ResponseTemplate>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
}

impl Respond for SequencedResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let mut queue = self.responses.lock().unwrap();
        if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            queue.front().cloned().unwrap()
        }
    }
}

/// Mount one mock on `path_str` that answers successive GET requests
/// with `responses` in order.
pub async fn mount_sequence(server: &MockServer, path_str: &str, responses: Vec<ResponseTemplate>) {
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(SequencedResponder::new(responses))
        .mount(server)
        .await;
}

/// One repository as the GitHub API returns it.
pub fn repo_json(id: u64, owner: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "full_name": format!("{owner}/{name}"),
        "owner": {"login": owner, "type": "User"},
        "private": false,
        "visibility": "public",
        "default_branch": "main",
        "clone_url": format!("https://github.com/{owner}/{name}.git"),
        "size": 10,
        "updated_at": "2026-11-18T00:00:00Z",
        "archived": false,
        "fork": false,
    })
}

pub fn user_json() -> serde_json::Value {
    json!({
        "id": 42,
        "login": "octocat",
        "name": "The Octocat",
        "avatar_url": "https://avatars.githubusercontent.com/u/42",
        "type": "User",
    })
}

pub fn installation_json(
    id: u64,
    login: &str,
    account_type: &str,
    suspended_at: Option<&str>,
) -> serde_json::Value {
    json!({
        "id": id,
        "account": {"login": login, "type": account_type},
        "permissions": {"contents": "read", "metadata": "read"},
        "repository_selection": "all",
        "suspended_at": suspended_at,
        "app_id": 99,
        "app_slug": "reponyx",
    })
}

/// A `Link` header pointing at the next discovery page of the mock.
pub fn next_link(server: &MockServer, page: u32) -> String {
    format!(
        "<{}/user/repos?page={}&per_page=100&sort=full_name&direction=asc>; rel=\"next\"",
        server.uri(),
        page
    )
}

/// A `Link` header pointing at the next installation-repositories
/// page of the mock.
pub fn next_installation_repos_link(
    server: &MockServer,
    installation_id: u64,
    page: u32,
) -> String {
    format!(
        "<{}/user/installations/{}/repositories?page={}&per_page=100>; rel=\"next\"",
        server.uri(),
        installation_id,
        page
    )
}
