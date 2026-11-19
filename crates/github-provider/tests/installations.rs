//! Installation metadata + permission-check tests against a mocked
//! GitHub API (wiremock). All tokens are fake.

#[path = "common.rs"]
mod common;

use common::{
    installation_json, mount_sequence, next_installation_repos_link, provider_for, repo_json,
};
use reponyx_github_provider::PermissionLevel;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn repo_with_permissions(
    id: u64,
    owner: &str,
    name: &str,
    admin: bool,
    push: bool,
    pull: bool,
) -> serde_json::Value {
    let mut repo = repo_json(id, owner, name);
    repo["permissions"] = json!({
        "admin": admin,
        "maintain": false,
        "push": push,
        "triage": false,
        "pull": pull,
    });
    repo
}

#[tokio::test]
async fn list_installations_returns_mapped_installations() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/user/installations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 2,
            "installations": [
                installation_json(11, "octocat", "User", None),
                installation_json(22, "acme", "Organization", Some("2026-11-18T00:00:00Z")),
            ],
        })))
        .mount(&server)
        .await;

    let installations = provider.list_installations().await.unwrap();
    assert_eq!(installations.len(), 2);
    assert_eq!(installations[0].installation_id, "11");
    assert_eq!(installations[0].account_login, "octocat");
    assert!(!installations[0].suspended);
    assert_eq!(installations[0].permissions["contents"], "read");
    assert_eq!(installations[1].installation_id, "22");
    assert!(installations[1].suspended);
    assert_eq!(installations[1].app_slug.as_deref(), Some("reponyx"));
}

#[tokio::test]
async fn list_installation_repositories_paginates() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    mount_sequence(
        &server,
        "/user/installations/11/repositories",
        vec![
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "total_count": 3,
                    "repositories": [repo_json(1, "octocat", "alpha"), repo_json(2, "octocat", "beta")],
                }))
                .insert_header("link", next_installation_repos_link(&server, 11, 2)),
            ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 3,
                "repositories": [repo_json(3, "octocat", "gamma")],
            })),
        ],
    )
    .await;

    let repos = provider.list_installation_repositories("11").await.unwrap();
    assert_eq!(repos.len(), 3);
    assert_eq!(repos[2].provider_repository_id, "3");
}

#[tokio::test]
async fn check_repository_access_reports_permission_levels() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    for (name, admin, push, pull, expected) in [
        ("admin-repo", true, true, true, PermissionLevel::Admin),
        ("write-repo", false, true, true, PermissionLevel::Write),
        ("read-repo", false, false, true, PermissionLevel::Read),
        ("none-repo", false, false, false, PermissionLevel::None),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/repos/octocat/{name}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(repo_with_permissions(1, "octocat", name, admin, push, pull)),
            )
            .mount(&server)
            .await;
        let access = provider
            .check_repository_access("octocat", name)
            .await
            .unwrap();
        assert_eq!(access.permission, expected, "{name}");
        assert_eq!(
            access.can_read_contents,
            expected != PermissionLevel::None,
            "{name}"
        );
        assert_eq!(access.full_name, format!("octocat/{name}"));
    }
}

#[tokio::test]
async fn check_repository_access_forbidden_is_insufficient_permissions() {
    let server = MockServer::start().await;
    let provider = provider_for(&server);
    Mock::given(method("GET"))
        .and(path("/repos/octocat/private-repo"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"message": "Resource not accessible by integration"})),
        )
        .mount(&server)
        .await;

    let err = provider
        .check_repository_access("octocat", "private-repo")
        .await
        .unwrap_err();
    assert_eq!(err.code, reponyx_core::ErrorCode::InsufficientPermissions);
}
