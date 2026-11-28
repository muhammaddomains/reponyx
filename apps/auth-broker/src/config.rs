//! Auth Broker configuration (environment only).
//!
//! The broker holds NO GitHub App private key (W4): the web is the
//! single holder of the key and mints installation tokens, and the
//! broker asks the web for them. The broker needs only the web's
//! base URL (`REPONYX_WEB_URL`, default `https://reponyx.bond`)
//! plus the setup token (which authenticates pairing requests from
//! the desktop operator), required from
//! `REPONYX_BROKER_SETUP_TOKEN`.
//!
//! Nothing in this module ever logs a secret: `Debug` for
//! [`BrokerConfig`] redacts the setup token.

use reponyx_core::SecretString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Default bind address (loopback only — the broker is a local
/// service; exposing it is a deployment decision, Phase 10).
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8765";
/// Default web base URL (the control plane that mints tokens).
pub const DEFAULT_WEB_BASE_URL: &str = "https://reponyx.bond";
/// GitHub-callback `state` values live 10 minutes.
pub const DEFAULT_STATE_TTL: Duration = Duration::from_secs(600);
/// Wrong-code attempts allowed per pairing code before it is
/// invalidated.
pub const DEFAULT_MAX_PAIRING_ATTEMPTS: u32 = 5;
/// Device tokens live 90 days, then minting requires re-pairing
/// (audit A1).
pub const DEFAULT_DEVICE_TOKEN_TTL: Duration = Duration::from_secs(90 * 24 * 60 * 60);
/// Consecutive failed setup-token presentations before setup-token
/// auth locks out (audit B2).
pub const DEFAULT_SETUP_AUTH_MAX_FAILURES: u32 = 10;
/// How long setup-token auth stays locked out after the failure
/// threshold (audit B2).
pub const DEFAULT_SETUP_AUTH_LOCKOUT: Duration = Duration::from_secs(15 * 60);
/// GitHub-callback rate limit: requests per minute per IP (audit B3).
pub const DEFAULT_CALLBACK_PER_MINUTE_PER_IP: f64 = 10.0;
/// Setup-state rate limit: requests per minute per IP (audit B6).
pub const DEFAULT_SETUP_STATE_PER_MINUTE_PER_IP: f64 = 10.0;

pub struct BrokerConfig {
    /// GitHub App slug (public, used for the install URL).
    pub app_slug: String,
    /// Web base URL (default `https://reponyx.bond`; tests point it
    /// at a mock server). The broker mints tokens and verifies
    /// installations through this URL.
    pub web_base_url: String,
    pub bind_addr: SocketAddr,
    /// Token that authenticates pairing/setup requests from the
    /// desktop operator. A [`SecretString`]: redacted in `Debug`,
    /// zeroized on drop, never logged.
    pub setup_token: SecretString,
    pub state_ttl: Duration,
    pub max_pairing_attempts: u32,
    /// Device-token lifetime (audit A1): a token minted more than
    /// this long ago no longer mints; the device must pair again.
    pub device_token_ttl: Duration,
    /// Consecutive failed setup-token presentations before
    /// setup-token auth locks out (audit B2).
    pub setup_auth_max_failures: u32,
    /// How long setup-token auth stays locked out (audit B2).
    pub setup_auth_lockout: Duration,
    /// GitHub-callback rate limit: requests per minute per IP
    /// (audit B3).
    pub callback_per_minute_per_ip: f64,
    /// Setup-state rate limit: requests per minute per IP (audit B6).
    pub setup_state_per_minute_per_ip: f64,
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
            .field("app_slug", &self.app_slug)
            .field("web_base_url", &self.web_base_url)
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
        let app_slug =
            std::env::var("REPONYX_BROKER_APP_SLUG").unwrap_or_else(|_| "reponyx".to_string());
        let setup_token = std::env::var("REPONYX_BROKER_SETUP_TOKEN").map_err(|_| {
            "REPONYX_BROKER_SETUP_TOKEN is not set; generate one with: openssl rand -hex 32"
                .to_string()
        })?;
        if setup_token.trim().len() < 16 {
            return Err("REPONYX_BROKER_SETUP_TOKEN must be at least 16 characters".to_string());
        }
        let web_base_url =
            std::env::var("REPONYX_WEB_URL").unwrap_or_else(|_| DEFAULT_WEB_BASE_URL.to_string());
        let bind_addr: SocketAddr = std::env::var("REPONYX_BROKER_BIND")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
            .parse()
            .map_err(|e| format!("REPONYX_BROKER_BIND is not a valid socket address: {e}"))?;
        // A non-loopback bind exposes the broker (and its setup-token
        // auth) to the LAN: it needs an explicit opt-in, not just a
        // changed address (audit B2). Loopback (`127.0.0.1`, `::1`)
        // needs nothing.
        let allow_non_loopback = std::env::var("REPONYX_BROKER_ALLOW_NON_LOOPBACK")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !bind_addr.ip().is_loopback() && !allow_non_loopback {
            return Err(format!(
                "REPONYX_BROKER_BIND={bind_addr} is not loopback; set REPONYX_BROKER_ALLOW_NON_LOOPBACK=true to expose the broker on the network (anyone on the network can then reach it — keep the setup token secret)"
            ));
        }
        let data_dir = std::env::var("REPONYX_BROKER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("broker-data"));
        Ok(Self {
            app_slug,
            web_base_url,
            bind_addr,
            setup_token: SecretString::new(setup_token),
            state_ttl: DEFAULT_STATE_TTL,
            max_pairing_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            device_token_ttl: DEFAULT_DEVICE_TOKEN_TTL,
            setup_auth_max_failures: DEFAULT_SETUP_AUTH_MAX_FAILURES,
            setup_auth_lockout: DEFAULT_SETUP_AUTH_LOCKOUT,
            callback_per_minute_per_ip: DEFAULT_CALLBACK_PER_MINUTE_PER_IP,
            setup_state_per_minute_per_ip: DEFAULT_SETUP_STATE_PER_MINUTE_PER_IP,
            mint_per_minute_per_device: 30.0,
            pairing_per_minute_per_ip: 10.0,
            data_dir,
        })
    }

    /// Test-only constructor: callers supply the web base URL (a
    /// mock server). Production code must use [`BrokerConfig::from_env`].
    pub fn for_tests(web_base_url: impl Into<String>) -> Self {
        Self {
            app_slug: "reponyx-test".to_string(),
            web_base_url: web_base_url.into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            setup_token: SecretString::new("test-setup-token-do-not-use"),
            state_ttl: DEFAULT_STATE_TTL,
            max_pairing_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            device_token_ttl: DEFAULT_DEVICE_TOKEN_TTL,
            setup_auth_max_failures: DEFAULT_SETUP_AUTH_MAX_FAILURES,
            setup_auth_lockout: DEFAULT_SETUP_AUTH_LOCKOUT,
            callback_per_minute_per_ip: DEFAULT_CALLBACK_PER_MINUTE_PER_IP,
            setup_state_per_minute_per_ip: DEFAULT_SETUP_STATE_PER_MINUTE_PER_IP,
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
            app_slug: "reponyx".to_string(),
            web_base_url: "http://localhost".to_string(),
            bind_addr: DEFAULT_BIND_ADDR.parse().unwrap(),
            setup_token: SecretString::new("test-setup-token-secret"),
            state_ttl: DEFAULT_STATE_TTL,
            max_pairing_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            device_token_ttl: DEFAULT_DEVICE_TOKEN_TTL,
            setup_auth_max_failures: DEFAULT_SETUP_AUTH_MAX_FAILURES,
            setup_auth_lockout: DEFAULT_SETUP_AUTH_LOCKOUT,
            callback_per_minute_per_ip: DEFAULT_CALLBACK_PER_MINUTE_PER_IP,
            setup_state_per_minute_per_ip: DEFAULT_SETUP_STATE_PER_MINUTE_PER_IP,
            mint_per_minute_per_device: 30.0,
            pairing_per_minute_per_ip: 10.0,
            data_dir: PathBuf::from("broker-data"),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("test-setup-token-secret"));
        assert!(debug.contains("http://localhost"));
    }
}
