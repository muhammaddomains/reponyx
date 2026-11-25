//! Auth Broker integration tests: the full HTTP surface against a
//! mocked GitHub API (wiremock). The App private key is a throwaway
//! RSA key generated in-memory per test; every token is obviously
//! fake (`test-…`); no real credentials, no network to github.com.

use pkcs8::{EncodePrivateKey, EncodePublicKey};
use reponyx_auth_broker::config::BrokerConfig;
use reponyx_auth_broker::{BrokerState, app};
use rsa::PrivateKey;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SETUP_TOKEN: &str = "test-setup-token-do-not-use";
const INSTALLATION_ID: &str = "777";
const DEVICE_ID: &str = "device-test-1";
const DEVICE_NAME: &str = "Test Machine";
const MINTED_TOKEN: &str = "test-installation-token-abc";

/// A throwaway RSA keypair, generated in-memory. Never committed,
/// never real.
fn throwaway_keypair() -> (String, String) {
    let mut rng = rand::rng();
    let private = PrivateKey::new(&mut rng, 2048).unwrap();
    let private_pem = private
        .to_pkcs8_pem(pkcs8::LineEnding::LF)
        .unwrap()
        .to_string();
    let public_pem = private
        .to_public_key()
        .to_public_key_pem(pkcs8::LineEnding::LF)
        .unwrap();
    (private_pem, public_pem)
}

struct Fixture {
    base: String,
    client: reqwest::Client,
    github: MockServer,
    public_pem: String,
}

async fn start_broker(mint_per_minute: f64) -> Fixture {
    let github = MockServer::start().await;
    let (private_pem, public_pem) = throwaway_keypair();
    let mut config = BrokerConfig::for_tests("123456", private_pem, github.uri());
    // Short TTL so the expired-state test does not wait 10 minutes.
    config.state_ttl = Duration::from_secs(2);
    config.mint_per_minute_per_device = mint_per_minute;
    let state = BrokerState::new(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = app(Arc::clone(&state)).into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Fixture {
        base: format!("http://{addr}"),
        client: reqwest::Client::new(),
        github,
        public_pem,
    }
}

/// Mock GitHub's token-minting endpoint for one installation.
async fn mock_mint_for(github: &MockServer, installation_id: &str) {
    Mock::given(method("POST"))
        .and(path(format!(
            "/app/installations/{installation_id}/access_tokens"
        )))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "token": MINTED_TOKEN,
            "expires_at": "2027-01-01T00:00:00Z",
            "permissions": {},
            "repository_selection": "all",
        })))
        .mount(github)
        .await;
}

/// Mock GitHub's token-minting endpoint for installation 777.
async fn mock_mint(github: &MockServer) {
    mock_mint_for(github, INSTALLATION_ID).await;
}

impl Fixture {
    /// Install the GitHub App for one installation via the real
    /// state + callback flow. Returns the recorded installation id.
    async fn install_app_with(&self, installation_id: &str) -> String {
        let state: serde_json::Value = self
            .client
            .post(format!("{}/setup/state", self.base))
            .header("x-reponyx-setup-token", SETUP_TOKEN)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let state_value = state["state"].as_str().unwrap();
        let response = self
            .client
            .get(format!("{}/github/callback", self.base))
            .query(&[
                ("installation_id", installation_id),
                ("setup_action", "install"),
                ("state", state_value),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            200,
            "callback: {}",
            response.text().await.unwrap()
        );
        installation_id.to_string()
    }

    /// Install the GitHub App for installation 777 via the real
    /// state + callback flow. Returns the recorded installation id.
    async fn install_app(&self) -> String {
        self.install_app_with(INSTALLATION_ID).await
    }

    /// Request a pairing code and confirm it for one installation.
    /// Returns the device token.
    async fn pair_device_for(&self, device_id: &str, installation_id: &str) -> String {
        let code: serde_json::Value = self
            .client
            .post(format!("{}/pairing/request", self.base))
            .header("x-reponyx-setup-token", SETUP_TOKEN)
            .json(&json!({
                "installation_id": installation_id,
                "device_name": DEVICE_NAME,
                "device_identifier": device_id,
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let pairing_code = code["pairing_code"].as_str().unwrap().to_string();
        let confirmed: serde_json::Value = self
            .client
            .post(format!("{}/pairing/confirm", self.base))
            .header("x-reponyx-setup-token", SETUP_TOKEN)
            .json(&json!({
                "pairing_code": pairing_code,
                "device_identifier": device_id,
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        confirmed["device_token"].as_str().unwrap().to_string()
    }

    /// Request a pairing code and confirm it. Returns the device token.
    async fn pair_device(&self, device_id: &str) -> String {
        self.pair_device_for(device_id, INSTALLATION_ID).await
    }

    async fn request_code(&self, device_id: &str) -> serde_json::Value {
        self.client
            .post(format!("{}/pairing/request", self.base))
            .header("x-reponyx-setup-token", SETUP_TOKEN)
            .json(&json!({
                "installation_id": INSTALLATION_ID,
                "device_name": DEVICE_NAME,
                "device_identifier": device_id,
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn health_is_public_and_secret_free() {
    let fixture = start_broker(30.0).await;
    let response = fixture
        .client
        .get(format!("{}/health", fixture.base))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn setup_endpoints_require_the_setup_token() {
    let fixture = start_broker(30.0).await;
    let no_token = fixture
        .client
        .post(format!("{}/setup/state", fixture.base))
        .send()
        .await
        .unwrap();
    assert_eq!(no_token.status(), 401);
    let wrong_token = fixture
        .client
        .post(format!("{}/setup/state", fixture.base))
        .header("x-reponyx-setup-token", "wrong-token")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_token.status(), 401);
    // The right token works, and the install URL names the App slug.
    let response = fixture
        .client
        .post(format!("{}/setup/state", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(
        body["install_url"]
            .as_str()
            .unwrap()
            .contains("github.com/apps/reponyx-test/installations/new")
    );
}

#[tokio::test]
async fn callback_records_installation_and_state_is_single_use() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    // The installation is listed.
    let installations: serde_json::Value = fixture
        .client
        .get(format!("{}/installations", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(installations.as_array().unwrap().len(), 1);
    assert_eq!(installations[0]["installation_id"], INSTALLATION_ID);

    // Reusing the same state value fails: single-use.
    let state: serde_json::Value = fixture
        .client
        .post(format!("{}/setup/state", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let state_value = state["state"].as_str().unwrap();
    let callback = format!(
        "{}/github/callback?installation_id=888&setup_action=install&state={state_value}",
        fixture.base
    );
    assert_eq!(
        fixture.client.get(&callback).send().await.unwrap().status(),
        200
    );
    assert_eq!(
        fixture.client.get(&callback).send().await.unwrap().status(),
        400
    );
}

#[tokio::test]
async fn callback_rejects_expired_state_and_missing_params() {
    let fixture = start_broker(30.0).await;
    let state: serde_json::Value = fixture
        .client
        .post(format!("{}/setup/state", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let state_value = state["state"].as_str().unwrap().to_string();
    // The state TTL in tests is 2 s.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let response = fixture
        .client
        .get(format!("{}/github/callback", fixture.base))
        .query(&[
            ("installation_id", INSTALLATION_ID),
            ("setup_action", "install"),
            ("state", state_value.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    // Missing installation_id.
    let response = fixture
        .client
        .get(format!("{}/github/callback", fixture.base))
        .query(&[("setup_action", "install"), ("state", "whatever")])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn pairing_happy_path_and_reuse_rejected() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    let code = fixture.request_code(DEVICE_ID).await;
    let pairing_code = code["pairing_code"].as_str().unwrap();
    assert_eq!(pairing_code.len(), 11); // XXX-XXX-XXX
    let confirmed = fixture
        .client
        .post(format!("{}/pairing/confirm", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "pairing_code": pairing_code,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(confirmed.status(), 200);
    let body: serde_json::Value = confirmed.json().await.unwrap();
    assert_eq!(body["installation_id"], INSTALLATION_ID);
    assert!(!body["device_token"].as_str().unwrap().is_empty());
    // Reuse is rejected.
    let reuse = fixture
        .client
        .post(format!("{}/pairing/confirm", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "pairing_code": pairing_code,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reuse.status(), 400);
}

#[tokio::test]
async fn pairing_requires_a_recorded_installation() {
    let fixture = start_broker(30.0).await;
    // No install callback happened: no installation is recorded.
    let response = fixture
        .client
        .post(format!("{}/pairing/request", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "installation_id": "999",
            "device_name": DEVICE_NAME,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn pairing_code_does_not_expire() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    let code = fixture.request_code(DEVICE_ID).await;
    // The response carries no expiry: pairing codes never expire.
    assert!(code["expires_at"].is_null());
    let pairing_code = code["pairing_code"].as_str().unwrap().to_string();
    // Past the old TTL's test stand-in (2 s): the code still confirms.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let response = fixture
        .client
        .post(format!("{}/pairing/confirm", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "pairing_code": pairing_code,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn pairing_wrong_device_is_rejected_and_locks_out() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    let code = fixture.request_code(DEVICE_ID).await;
    let pairing_code = code["pairing_code"].as_str().unwrap().to_string();
    // Five wrong-device attempts: the first four are "different
    // device", the fifth invalidates the code.
    for attempt in 1..=5 {
        let response = fixture
            .client
            .post(format!("{}/pairing/confirm", fixture.base))
            .header("x-reponyx-setup-token", SETUP_TOKEN)
            .json(&json!({
                "pairing_code": pairing_code,
                "device_identifier": "device-other",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        let body: serde_json::Value = response.json().await.unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        if attempt < 5 {
            assert!(
                message.contains("different device"),
                "attempt {attempt}: {message}"
            );
        } else {
            assert!(
                message.contains("too many attempts"),
                "attempt {attempt}: {message}"
            );
        }
    }
    // Even the right device cannot use the invalidated code now.
    let response = fixture
        .client
        .post(format!("{}/pairing/confirm", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "pairing_code": pairing_code,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let status: serde_json::Value = fixture
        .client
        .get(format!("{}/pairing/status", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .query(&[("code", pairing_code.as_str())])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["status"], "revoked");
}

#[tokio::test]
async fn pairing_revoke_invalidates_device_token_and_pending_codes() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    mock_mint(&fixture.github).await;
    let device_token = fixture.pair_device(DEVICE_ID).await;
    // The device token mints fine…
    let mint = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(mint.status(), 200);
    // …until the device is revoked.
    let revoked: serde_json::Value = fixture
        .client
        .post(format!("{}/pairing/revoke", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({ "device_identifier": DEVICE_ID }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(revoked["revoked"], true);
    let mint = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(mint.status(), 401);
    // A pending code for the same device is revoked too.
    let code = fixture.request_code(DEVICE_ID).await;
    let pairing_code = code["pairing_code"].as_str().unwrap().to_string();
    let _ = fixture
        .client
        .post(format!("{}/pairing/revoke", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({ "device_identifier": DEVICE_ID }))
        .send()
        .await
        .unwrap();
    let confirm = fixture
        .client
        .post(format!("{}/pairing/confirm", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "pairing_code": pairing_code,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(confirm.status(), 400);
}

#[derive(Debug, Deserialize)]
struct AppJwtClaims {
    iss: String,
    iat: i64,
    exp: i64,
}

#[tokio::test]
async fn mint_uses_a_real_signed_jwt_and_caches() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    mock_mint(&fixture.github).await;
    let device_token = fixture.pair_device(DEVICE_ID).await;

    let first = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let body: serde_json::Value = first.json().await.unwrap();
    assert_eq!(body["token"], MINTED_TOKEN);

    // The broker presented a real RS256 JWT to GitHub: three parts,
    // signature verifiable with the throwaway public key, claims
    // correct (iss = App id, 9-minute lifetime, iat backdated).
    let requests = fixture.github.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let auth = requests[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap()
        .to_string();
    assert_eq!(auth.split('.').count(), 3);
    let key = jsonwebtoken::DecodingKey::from_rsa_pem(fixture.public_pem.as_bytes()).unwrap();
    let decoded = jsonwebtoken::decode::<AppJwtClaims>(
        &auth,
        &key,
        &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256),
    )
    .unwrap();
    assert_eq!(decoded.claims.iss, "123456");
    assert_eq!(decoded.claims.exp - decoded.claims.iat, 600);

    // A second mint is served from the cache: no second GitHub call.
    let second = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    assert_eq!(fixture.github.received_requests().await.unwrap().len(), 1);

    // force_refresh bypasses the cache.
    let third = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(third.status(), 200);
    assert_eq!(fixture.github.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn mint_rejects_wrong_device_and_wrong_installation() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    mock_mint(&fixture.github).await;
    let device_token = fixture.pair_device(DEVICE_ID).await;
    // No token at all.
    let response = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    // A device token cannot mint for an installation that is not
    // paired to this device (888 was never installed at all).
    let response = fixture
        .client
        .post(format!("{}/installations/888/token", fixture.base))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn mint_allows_any_installation_paired_to_the_same_device() {
    // Regression test for the user's Validate failure: the desktop
    // stores one device token per device, overwritten on each
    // pairing, so the mint gate must be the installation's paired
    // device — not the installation the token was confirmed for.
    // Pairing 777, then 888, then validating 777 used to 403.
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    fixture.install_app_with("888").await;
    mock_mint(&fixture.github).await;
    mock_mint_for(&fixture.github, "888").await;
    let token_for_777 = fixture.pair_device(DEVICE_ID).await;
    let token_for_888 = fixture.pair_device_for(DEVICE_ID, "888").await;
    // The token confirmed for 777 mints for 888 (same device).
    let response = fixture
        .client
        .post(format!("{}/installations/888/token", fixture.base))
        .header("authorization", format!("Bearer {token_for_777}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    // And the other way round.
    let response = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {token_for_888}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    // But a token from a different device still cannot mint for 777.
    let other_token = fixture.pair_device_for("device-other", "888").await;
    let response = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {other_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn list_installations_fills_account_login_from_github() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    let list_url = format!("{}/installations", fixture.base);
    // GitHub has no mock yet: the list still succeeds, login stays null.
    let before: serde_json::Value = fixture
        .client
        .get(&list_url)
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(before[0]["account_login"], serde_json::Value::Null);
    // Now GitHub answers with the account login.
    Mock::given(method("GET"))
        .and(path("/app/installations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 777, "account": { "login": "octocat" } }
        ])))
        .mount(&fixture.github)
        .await;
    let after: serde_json::Value = fixture
        .client
        .get(&list_url)
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after[0]["account_login"], "octocat");
}

#[tokio::test]
async fn mint_rate_limit_returns_429_with_retry_after() {
    // Two mints per minute per device.
    let fixture = start_broker(2.0).await;
    fixture.install_app().await;
    mock_mint(&fixture.github).await;
    let device_token = fixture.pair_device(DEVICE_ID).await;
    let url = format!("{}/installations/{}/token", fixture.base, INSTALLATION_ID);
    for _ in 0..2 {
        let response = fixture
            .client
            .post(&url)
            .header("authorization", format!("Bearer {device_token}"))
            .json(&json!({ "force_refresh": true }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
    let limited = fixture
        .client
        .post(&url)
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(limited.status(), 429);
    assert!(limited.headers().contains_key("retry-after"));
}

#[tokio::test]
async fn audit_events_never_contain_a_token_or_code() {
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    mock_mint(&fixture.github).await;
    let code = fixture.request_code(DEVICE_ID).await;
    let pairing_code = code["pairing_code"].as_str().unwrap().to_string();
    let confirmed = fixture
        .client
        .post(format!("{}/pairing/confirm", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .json(&json!({
            "pairing_code": pairing_code,
            "device_identifier": DEVICE_ID,
        }))
        .send()
        .await
        .unwrap();
    let device_token: String = confirmed.json::<serde_json::Value>().await.unwrap()["device_token"]
        .as_str()
        .unwrap()
        .to_string();
    let mint = fixture
        .client
        .post(format!(
            "{}/installations/{}/token",
            fixture.base, INSTALLATION_ID
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(mint.status(), 200);

    let audit: serde_json::Value = fixture
        .client
        .get(format!("{}/audit", fixture.base))
        .header("x-reponyx-setup-token", SETUP_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let serialized = serde_json::to_string(&audit).unwrap();
    assert!(
        !serialized.contains(&pairing_code),
        "pairing code leaked into audit"
    );
    assert!(
        !serialized.contains(&device_token),
        "device token leaked into audit"
    );
    assert!(
        !serialized.contains(MINTED_TOKEN),
        "installation token leaked into audit"
    );
    assert!(
        !serialized.contains("PRIVATE KEY"),
        "key material leaked into audit"
    );
    // But the events themselves are there.
    let actions: Vec<&str> = audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"pairing.requested"));
    assert!(actions.contains(&"pairing.confirmed"));
    assert!(actions.contains(&"token.minted"));
}

/// A writer that captures log output in memory.
#[derive(Clone)]
struct CapturingWriter(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Log redaction, deterministic by construction.
///
/// The old version installed the capturing subscriber as a
/// thread-local dispatcher (`tracing::dispatcher::set_default`):
/// it only saw events on the test thread, so under the default
/// multi-threaded runtime the mint line was lost whenever the
/// handler ran on another worker — and the `current_thread` fix
/// still flaked in full-suite runs (the line was missing about
/// once in every few parallel runs, mechanism never pinned down:
/// the server, sqlx worker threads, and reqwest all emit on
/// threads a thread-local dispatcher cannot see). This version
/// installs the capture as the PROCESS-GLOBAL default, so every
/// thread's events land in the buffer, and mints for a UNIQUE
/// installation id so the assertion names this test's own mint
/// line even though other tests mint concurrently in the same
/// binary. No other broker test installs a global default, so
/// `set_global_default` must succeed — a failure fails loudly
/// instead of passing vacuously on an empty buffer.
#[tokio::test]
async fn broker_logs_never_contain_a_token() {
    const LOG_TEST_INSTALLATION: &str = "424242";
    const LOG_TEST_DEVICE: &str = "device-log-test";
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = CapturingWriter(Arc::clone(&buf));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::dispatcher::set_global_default(tracing::Dispatch::new(subscriber))
        .expect("no other test may install a global tracing default");

    // Drive the full flow for this test's own installation:
    // install, pair, mint.
    let fixture = start_broker(60.0).await;
    mock_mint_for(&fixture.github, LOG_TEST_INSTALLATION).await;
    fixture.install_app_with(LOG_TEST_INSTALLATION).await;
    let device_token = fixture
        .pair_device_for(LOG_TEST_DEVICE, LOG_TEST_INSTALLATION)
        .await;
    let mint = fixture
        .client
        .post(format!(
            "{}/installations/{LOG_TEST_INSTALLATION}/token",
            fixture.base
        ))
        .header("authorization", format!("Bearer {device_token}"))
        .json(&json!({ "force_refresh": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(mint.status(), 200);

    let logs = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    // This test's own mint line — the message AND this test's
    // installation id on the same line — so the assertion cannot
    // pass on another test's concurrent mint.
    assert!(
        logs.lines()
            .any(|line| line.contains("installation token minted")
                && line.contains(LOG_TEST_INSTALLATION)),
        "this test's mint line is missing from the logs:\n{logs}"
    );
    assert!(
        !logs.contains(MINTED_TOKEN),
        "installation token leaked into logs:\n{logs}"
    );
    assert!(
        !logs.contains(&device_token),
        "device token leaked into logs:\n{logs}"
    );
}

#[tokio::test]
async fn provider_validates_through_the_real_broker() {
    // End to end: a real broker (throwaway key) mints a real
    // installation token, `BrokerTokenSource` fetches it, and
    // `GitHubProvider` validates against the mocked GitHub API.
    // The mint endpoint and `/user` both live on the same mock
    // server, which the broker and the provider both point at.
    let fixture = start_broker(30.0).await;
    fixture.install_app().await;
    mock_mint(&fixture.github).await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("Bearer {MINTED_TOKEN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "login": "octocat", "name": "The Octocat", "type": "User",
        })))
        .mount(&fixture.github)
        .await;
    let device_token = fixture.pair_device(DEVICE_ID).await;
    let source = reponyx_github_provider::broker::BrokerTokenSource::new(
        &fixture.base,
        INSTALLATION_ID,
        device_token,
    )
    .unwrap();
    let provider =
        reponyx_github_provider::GitHubProvider::new(&fixture.github.uri(), Arc::new(source))
            .unwrap();
    let info = provider.validate_auth().await.unwrap();
    assert_eq!(info.username, "octocat");
    assert_eq!(info.provider_account_id, "42");
    // Exactly one mint for the whole validation (the token is
    // cached by the source; the provider asked once).
    let mint_requests = fixture.github.received_requests().await.unwrap();
    assert_eq!(
        mint_requests
            .iter()
            .filter(|request| request.url.path() == "/app/installations/777/access_tokens")
            .count(),
        1
    );
}
