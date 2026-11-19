//! Mixed auth: a PAT account and a GitHub App (broker) account
//! coexist — both validate against the same mocked GitHub API, and
//! the sync engine sees only the `GitProvider` trait. All tokens
//! are obviously fake (`test-…`); no real credentials, no network
//! to github.com.

use reponyx_credential_store::{
    PatInput, PatTokenSource, SecretString, add_pat_account, memory_store,
};
use reponyx_database::open_database;
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::broker::BrokerTokenSource;
use reponyx_github_provider::credentials::CredentialSource;
use reponyx_provider_abstractions::GitProvider;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn pat_and_broker_accounts_validate_side_by_side() {
    let dir = TempDir::new().unwrap();
    let db = open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    let pool = db.pool;
    let github = MockServer::start().await;

    // The PAT identity answers to its `token` scheme…
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header(
            "authorization",
            "token ghp_000000000000000000000000000000000001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "login": "octocat", "name": "The Octocat", "type": "User",
        })))
        .mount(&github)
        .await;
    // …and the installation-token identity answers to `Bearer`.
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header(
            "authorization",
            "Bearer test-installation-token-xyz",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 123, "login": "reponyx-app", "name": "RepoNyx App", "type": "Bot",
        })))
        .mount(&github)
        .await;

    // The broker (mocked) mints installation tokens.
    let broker = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/installations/777/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "test-installation-token-xyz",
            "expires_at": "2027-01-01T00:00:00Z",
        })))
        .mount(&broker)
        .await;

    // PAT side: add the account through the real flow.
    let store = memory_store();
    let pat_account = add_pat_account(
        &pool,
        &store,
        &github.uri(),
        PatInput {
            pat: SecretString::new("ghp_000000000000000000000000000000000001"),
            label: None,
            account_type: reponyx_core::AccountType::Personal,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(pat_account.username, "octocat");

    // Broker side: the source mints from the mock broker.
    let broker_source =
        BrokerTokenSource::new(&broker.uri(), "777", "test-device-token-abc").unwrap();
    let token = broker_source.token().await.unwrap();
    assert_eq!(token.username, "x-access-token");
    assert_eq!(
        token.authorization_header(),
        "Bearer test-installation-token-xyz"
    );

    // Both validate through the `GitProvider` trait — the only
    // interface the sync engine ever sees.
    let pat_provider = GitHubProvider::new(
        &github.uri(),
        Arc::new(PatTokenSource::new(
            Arc::clone(&store),
            pat_account.id,
            "octocat",
            None,
        )),
    )
    .unwrap();
    let broker_provider = GitHubProvider::new(
        &github.uri(),
        Arc::new(BrokerTokenSource::new(&broker.uri(), "777", "test-device-token-abc").unwrap()),
    )
    .unwrap();
    let pat_info = GitProvider::validate_auth(&pat_provider).await.unwrap();
    let broker_info = GitProvider::validate_auth(&broker_provider).await.unwrap();
    assert_eq!(pat_info.username, "octocat");
    assert_eq!(pat_info.provider_account_id, "42");
    assert_eq!(broker_info.username, "reponyx-app");
    assert_eq!(broker_info.provider_account_id, "123");
    assert_ne!(
        pat_info.provider_account_id,
        broker_info.provider_account_id
    );
}
