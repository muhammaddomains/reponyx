//! Auth Broker configuration (environment only).
//!
//! The GitHub App private key lives ONLY here: it is loaded at
//! startup from `REPONYX_BROKER_PRIVATE_KEY` (PEM text) or
//! `REPONYX_BROKER_PRIVATE_KEY_PATH` (a file path, read once at
//! startup). There is no default key, no committed key file, and no
//! key in any config file — the broker refuses to start its token
//! endpoints without one. The setup token (which authenticates
//! pairing requests from the desktop operator) is likewise required
//! from `REPONYX_BROKER_SETUP_TOKEN`.
//!
//! Nothing in this module ever logs a secret: `Debug` for
//! [`BrokerConfig`] redacts the key and the setup token.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Default bind address (loopback only — the broker is a local
/// service; exposing it is a deployment decision, Phase 10).
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8765";
/// Default GitHub API base.
pub const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
/// GitHub-callback `state` values live 10 minutes.
pub const DEFAULT_STATE_TTL: Duration = Duration::from_secs(600);
/// Wrong-code attempts allowed per pairing code before it is
/// invalidated.
pub const DEFAULT_MAX_PAIRING_ATTEMPTS: u32 = 5;

pub struct BrokerConfig {
    /// GitHub App id (public, from the App's settings page).
    pub app_id: String,
    /// GitHub App slug (public, used for the install URL).
    pub app_slug: String,
    /// The App's private key, PEM text. Secret — redacted in `Debug`,
    /// never logged, never returned by any endpoint.
    pub private_key_pem: String,
    /// GitHub API base (default `https://api.github.com`; tests point
    /// it at a mock server).
    pub github_api_base: String,
    pub bind_addr: SocketAddr,
    /// Token that authenticates pairing/setup requests from the
    /// desktop operator. Secret — redacted in `Debug`, never logged.
    pub setup_token: String,
    pub state_ttl: Duration,
    pub max_pairing_attempts: u32,
    /// Token-mint rate limit: requests per minute per device.
    pub mint_per_minute_per_device: f64,
    /// Pairing-request rate limit: requests per minute per IP.
    pub pairing_per_minute_per_ip: f64,
    /// Directory holding the durable broker state
    /// (`broker.sqlite`). Default `broker-data` under the working
    /// directory; override with `REPONYX_BROKER_DATA_DIR`.
    pub data_dir: PathBuf,
}

impl std::fmt::Debug for BrokerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerConfig")
            .field("app_id", &self.app_id)
            .field("app_slug", &self.app_slug)
            .field("private_key_pem", &"<redacted>")
            .field("github_api_base", &self.github_api_base)
            .field("bind_addr", &self.bind_addr)
            .field("setup_token", &"<redacted>")
            .field("state_ttl", &self.state_ttl)
            .field("max_pairing_attempts", &self.max_pairing_attempts)
            .field("data_dir", &self.data_dir)
            .finish_non_exhaustive()
    }
}

impl BrokerConfig {
    /// Load configuration from the environment. Fails fast with a
    /// plain-text message (never containing a secret) when a required
    /// value is missing, so a misconfigured broker never starts in a
    /// half-working state.
    pub fn from_env() -> Result<Self, String> {
        let app_id = std::env::var("REPONYX_BROKER_APP_ID")
            .map_err(|_| "REPONYX_BROKER_APP_ID is not set".to_string())?;
        if app_id.trim().is_empty() {
            return Err("REPONYX_BROKER_APP_ID is empty".to_string());
        }
        let app_slug =
            std::env::var("REPONYX_BROKER_APP_SLUG").unwrap_or_else(|_| "reponyx".to_string());
        let private_key_pem = match std::env::var("REPONYX_BROKER_PRIVATE_KEY") {
            Ok(pem) if !pem.trim().is_empty() => pem,
            _ => {
                let path = std::env::var("REPONYX_BROKER_PRIVATE_KEY_PATH").map_err(|_| {
                    "set REPONYX_BROKER_PRIVATE_KEY or REPONYX_BROKER_PRIVATE_KEY_PATH".to_string()
                })?;
                std::fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read private key file {path}: {e}"))?
            }
        };
        if !private_key_pem.contains("PRIVATE KEY") {
            return Err("private key does not look like a PEM private key".to_string());
        }
        let setup_token = std::env::var("REPONYX_BROKER_SETUP_TOKEN").map_err(|_| {
            "REPONYX_BROKER_SETUP_TOKEN is not set; generate one with: openssl rand -hex 32"
                .to_string()
        })?;
        if setup_token.trim().len() < 16 {
            return Err("REPONYX_BROKER_SETUP_TOKEN must be at least 16 characters".to_string());
        }
        let github_api_base = std::env::var("REPONYX_BROKER_GITHUB_API_URL")
            .unwrap_or_else(|_| DEFAULT_GITHUB_API_BASE.to_string());
        let bind_addr: SocketAddr = std::env::var("REPONYX_BROKER_BIND")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
            .parse()
            .map_err(|e| format!("REPONYX_BROKER_BIND is not a valid socket address: {e}"))?;
        let data_dir = std::env::var("REPONYX_BROKER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("broker-data"));
        Ok(Self {
            app_id,
            app_slug,
            private_key_pem,
            github_api_base,
            bind_addr,
            setup_token,
            state_ttl: DEFAULT_STATE_TTL,
            max_pairing_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            mint_per_minute_per_device: 30.0,
            pairing_per_minute_per_ip: 10.0,
            data_dir,
        })
    }

    /// Test-only constructor: callers supply every value, including
    /// a throwaway key generated in-memory. Production code must use
    /// [`BrokerConfig::from_env`].
    pub fn for_tests(
        app_id: impl Into<String>,
        private_key_pem: impl Into<String>,
        github_api_base: impl Into<String>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_slug: "reponyx-test".to_string(),
            private_key_pem: private_key_pem.into(),
            github_api_base: github_api_base.into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            setup_token: "test-setup-token-do-not-use".to_string(),
            state_ttl: DEFAULT_STATE_TTL,
            max_pairing_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            mint_per_minute_per_device: 30.0,
            pairing_per_minute_per_ip: 10.0,
            // Never a temp_dir() call here: Semgrep's temp-dir rule
            // flags ANY use of std::env::temp_dir() in scanned code
            // (proven 2026-11-23 — a UUID-suffixed join did not
            // help; Security audit #24 failed on it). The path is
            // never used by tests — `BrokerState::new` runs
            // in-memory; persistence tests open the store explicitly.
            data_dir: PathBuf::from("broker-data"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_debug_redacts_secrets() {
        let config = BrokerConfig {
            app_id: "123456".to_string(),
            app_slug: "reponyx".to_string(),
            private_key_pem:
                "-----BEGIN RSA PRIVATE KEY-----\ntest-key-material\n-----END RSA PRIVATE KEY-----"
                    .to_string(),
            github_api_base: DEFAULT_GITHUB_API_BASE.to_string(),
            bind_addr: DEFAULT_BIND_ADDR.parse().unwrap(),
            setup_token: "test-setup-token-secret".to_string(),
            state_ttl: DEFAULT_STATE_TTL,
            max_pairing_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            mint_per_minute_per_device: 30.0,
            pairing_per_minute_per_ip: 10.0,
            data_dir: PathBuf::from("broker-data"),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("test-key-material"));
        assert!(!debug.contains("test-setup-token-secret"));
        assert!(debug.contains("123456"));
    }
}
