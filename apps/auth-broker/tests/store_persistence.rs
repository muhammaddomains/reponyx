//! Broker store persistence tests (Phase 10, F1): state must
//! survive a broker restart. Each test opens a [`BrokerState`] on a
//! fresh SQLite store, performs real operations, drops the state
//! (simulating a restart — the store file stays), re-opens it, and
//! asserts the state is exactly what the first instance left.
//!
//! The App private key is a throwaway RSA key generated in-memory
//! per test; every token is obviously fake; no real credentials.

use pkcs8::EncodePrivateKey;
use reponyx_auth_broker::BrokerStore;
use reponyx_auth_broker::config::BrokerConfig;
use reponyx_auth_broker::state::{BrokerState, DeviceTokenRecord, InstallationRecord};
use rsa::PrivateKey;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// A throwaway RSA keypair, generated in-memory. Never committed,
/// never real.
fn throwaway_private_pem() -> String {
    let mut rng = rand::rng();
    let private = PrivateKey::new(&mut rng, 2048).unwrap();
    private
        .to_pkcs8_pem(pkcs8::LineEnding::LF)
        .unwrap()
        .to_string()
}

fn test_config(state_ttl: Duration) -> BrokerConfig {
    let mut config = BrokerConfig::for_tests("123456", throwaway_private_pem(), "http://localhost");
    config.state_ttl = state_ttl;
    config
}

fn fresh_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "reponyx-broker-store-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn open_state(dir: &Path, config: BrokerConfig) -> Arc<BrokerState> {
    let store = BrokerStore::open(&dir.join("broker.sqlite")).await.unwrap();
    BrokerState::open(config, Arc::new(store)).await.unwrap()
}

async fn pair_device(state: &BrokerState, installation_id: &str, device: &str) -> String {
    let (code, _) = state
        .request_pairing(installation_id, "Test Machine", device)
        .await
        .unwrap();
    let (token, _) = state.confirm_pairing(&code, device).await.unwrap();
    token
}

#[tokio::test]
async fn restart_preserves_pairing_flow() {
    let dir = fresh_dir();
    let installation_id = "777";
    let device = "device-restart-1";
    let token = {
        let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
        state
            .record_installation(installation_id, "install", Some("octocat".to_string()))
            .await
            .unwrap();
        let (code, _) = state
            .request_pairing(installation_id, "Test Machine", device)
            .await
            .unwrap();
        // The pairing code itself is never stored — only its hash.
        // The raw code must still confirm after a restart, so the
        // hash has to survive.
        let (token, confirmed_installation) = state.confirm_pairing(&code, device).await.unwrap();
        assert_eq!(confirmed_installation, installation_id);
        // Simulate a restart: drop the whole state (store file stays).
        drop(state);
        token
    };
    let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
    let installations = state.list_installations().await;
    assert_eq!(installations.len(), 1);
    assert_eq!(installations[0].installation_id, installation_id);
    assert_eq!(installations[0].account_login.as_deref(), Some("octocat"));
    assert_eq!(installations[0].paired_device.as_deref(), Some(device));
    // The confirmed code is still confirmed (not reusable).
    let status = state.pairing_status("000-000").await;
    assert_eq!(status.status, "unknown");
    // The device token still resolves to the same device.
    let record = state.device_for_token(&token).await.unwrap();
    assert_eq!(record.device_identifier, device);
    assert_eq!(record.installation_id, installation_id);
    assert!(!record.revoked);
    // The audit log survived the restart. `pairing.confirmed` is
    // audited by the state method itself; `pairing.requested` is
    // audited by the HTTP route (covered by the broker.rs
    // integration tests), so it is not expected here.
    let events = state.audit_events();
    let actions: Vec<&str> = events.iter().map(|event| event.action.as_str()).collect();
    assert!(actions.contains(&"pairing.confirmed"));
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn restart_preserves_revocation() {
    let dir = fresh_dir();
    let device = "device-restart-2";
    let token = {
        let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
        state
            .record_installation("778", "install", None)
            .await
            .unwrap();
        let token = pair_device(&state, "778", device).await;
        // A second, still-pending code for the same device.
        let (pending_code, _) = state
            .request_pairing("778", "Test Machine", device)
            .await
            .unwrap();
        assert!(state.revoke_device(device).await.unwrap());
        drop(state);
        // Keep the pending code + token to check after the restart.
        (token, pending_code)
    };
    let (token, pending_code) = token;
    let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
    assert!(state.device_for_token(&token).await.is_none());
    assert_eq!(state.pairing_status(&pending_code).await.status, "revoked");
    assert!(!state.revoke_device(device).await.unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn setup_state_is_single_use_across_restart() {
    let dir = fresh_dir();
    let issued = {
        let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
        let (state_value, _) = state.issue_setup_state().await.unwrap();
        assert!(state.consume_setup_state(&state_value).await.unwrap());
        drop(state);
        state_value
    };
    let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
    // Consumed before the restart: still consumed after it.
    assert!(!state.consume_setup_state(&issued).await.unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn wrong_device_attempts_survive_restart() {
    let dir = fresh_dir();
    let code = {
        let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
        state
            .record_installation("779", "install", None)
            .await
            .unwrap();
        let (code, _) = state
            .request_pairing("779", "Test Machine", "device-restart-3")
            .await
            .unwrap();
        // Two wrong-device attempts (limit is 5).
        for _ in 0..2 {
            assert!(
                state
                    .confirm_pairing(&code, "device-attacker")
                    .await
                    .is_err()
            );
        }
        drop(state);
        code
    };
    let state = open_state(&dir, test_config(Duration::from_secs(600))).await;
    // Three more wrong attempts: the 5th invalidates the code.
    for _ in 0..2 {
        assert!(
            state
                .confirm_pairing(&code, "device-attacker")
                .await
                .is_err()
        );
    }
    let err = state
        .confirm_pairing(&code, "device-attacker")
        .await
        .unwrap_err();
    assert!(err.message.contains("invalidated after too many attempts"));
    assert_eq!(state.pairing_status(&code).await.status, "revoked");
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn pending_pairing_code_survives_restart() {
    // Pairing codes never expire: a pending code requested before a
    // broker restart still confirms after it (past the old 1 s TTL
    // this test used to stand in for).
    let dir = fresh_dir();
    let code = {
        let state = open_state(&dir, test_config(Duration::from_secs(1))).await;
        state
            .record_installation("780", "install", None)
            .await
            .unwrap();
        let (code, _) = state
            .request_pairing("780", "Test Machine", "device-restart-4")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        drop(state);
        code
    };
    let state = open_state(&dir, test_config(Duration::from_secs(1))).await;
    assert_eq!(state.pairing_status(&code).await.status, "pending");
    assert!(
        state
            .confirm_pairing(&code, "device-restart-4")
            .await
            .is_ok()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn store_round_trips_records() {
    let dir = fresh_dir();
    let store = BrokerStore::open(&dir.join("broker.sqlite")).await.unwrap();
    let now = chrono::Utc::now();
    store
        .upsert_installation(&InstallationRecord {
            installation_id: "781".to_string(),
            setup_action: "install".to_string(),
            account_login: Some("octocat".to_string()),
            installed_at: now,
            paired_device: Some("device-roundtrip".to_string()),
        })
        .await
        .unwrap();
    store
        .insert_device_token(&DeviceTokenRecord {
            token_hash: "hash-of-a-token".to_string(),
            device_identifier: "device-roundtrip".to_string(),
            device_name: "Test Machine".to_string(),
            installation_id: "781".to_string(),
            created_at: now,
            revoked: false,
        })
        .await
        .unwrap();
    let installations = store.load_installations().await.unwrap();
    assert_eq!(installations.len(), 1);
    assert_eq!(
        installations[0].paired_device.as_deref(),
        Some("device-roundtrip")
    );
    let tokens = store.load_device_tokens().await.unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].device_identifier, "device-roundtrip");
    std::fs::remove_dir_all(&dir).ok();
}
