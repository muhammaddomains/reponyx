//! Auth Broker client and broker-backed token source.
//!
//! The Auth Broker (`apps/auth-broker`) is the only component that
//! holds the GitHub App private key. This module is the desktop's
//! side of that relationship:
//!
//! - [`BrokerClient`]: pairing flows (setup-token auth) and token
//!   minting (device-token auth) over HTTP. It holds no secrets
//!   itself — callers pass the setup token / device token per call,
//!   straight from the OS credential store.
//! - [`BrokerTokenSource`]: a [`CredentialSource`] that mints
//!   installation tokens via the broker, caches them until shortly
//!   before expiry, and refreshes on 401 (via `invalidate()`).
//!
//! The broker base URL defaults to `http://127.0.0.1:8765` (local
//! dev); the desktop configures it. Tokens are never logged and
//! never appear in error messages or `Debug` output.

use crate::credentials::{CredentialKind, CredentialSource, Token};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reponyx_core::{ErrorCode, RepoNyxError};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::Mutex;

/// Default Auth Broker base URL (local dev).
pub const DEFAULT_BROKER_BASE_URL: &str = "http://127.0.0.1:8765";

/// The error shape the broker returns:
/// `{ "error": { "code": "...", "message": "..." } }`.
#[derive(Debug, Deserialize)]
struct BrokerErrorEnvelope {
    error: BrokerErrorBody,
}

#[derive(Debug, Deserialize)]
struct BrokerErrorBody {
    code: String,
    message: String,
}

/// Map a broker error code string to a RepoNyx error code. Unknown
/// codes become `InternalError` — the broker never puts a secret in
/// its message, so passing it through is safe.
fn error_from_code(code: &str, message: String) -> RepoNyxError {
    let mapped = match code {
        "AuthenticationExpired" => ErrorCode::AuthenticationExpired,
        "AuthenticationRevoked" => ErrorCode::AuthenticationRevoked,
        "InsufficientPermissions" => ErrorCode::InsufficientPermissions,
        "InvalidInput" => ErrorCode::InvalidInput,
        "RateLimitExceeded" => ErrorCode::RateLimitExceeded,
        "OperationTimedOut" => ErrorCode::OperationTimedOut,
        "NetworkUnavailable" => ErrorCode::NetworkUnavailable,
        "CredentialStoreError" => ErrorCode::CredentialStoreError,
        _ => ErrorCode::InternalError,
    };
    RepoNyxError::new(mapped, message)
}

/// One recorded installation, as the broker reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct BrokerInstallation {
    pub installation_id: String,
    pub setup_action: String,
    pub account_login: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub paired_device: Option<String>,
}

/// Pairing-code status, as the broker reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct PairingStatus {
    pub status: String,
    pub installation_id: Option<String>,
    pub device_identifier: Option<String>,
}

/// The Auth Broker HTTP client. Stateless and secret-free: the
/// setup token and device token are passed per call.
#[derive(Debug, Clone)]
pub struct BrokerClient {
    base_url: String,
    http: reqwest::Client,
}

impl BrokerClient {
    /// Build a client for `base_url` (default
    /// [`DEFAULT_BROKER_BASE_URL`]). The URL must be `http(s)://...`
    /// with no trailing slash (a trailing slash is tolerated).
    pub fn new(base_url: &str) -> Result<Self, RepoNyxError> {
        let base = base_url.trim_end_matches('/').to_string();
        if !(base.starts_with("https://") || base.starts_with("http://")) {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "Auth Broker base URL must start with http:// or https://",
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!("RepoNyx/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| {
                RepoNyxError::new(ErrorCode::InternalError, "cannot build HTTP client")
                    .with_details(e.to_string())
            })?;
        Ok(Self {
            base_url: base,
            http,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Liveness check. `Ok(true)` when the broker answers `/health`
    /// with 200; any other outcome is an error (never a silent
    /// "down" that a caller could mistake for "not installed").
    pub async fn health(&self) -> Result<bool, RepoNyxError> {
        let response = self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(transport_error)?;
        Ok(response.status().is_success())
    }

    /// Issue a one-time CSRF `state` for the GitHub App install flow
    /// and the install URL to send the user to. Requires the broker
    /// setup token.
    pub async fn request_setup_state(
        &self,
        setup_token: &str,
    ) -> Result<(String, String, DateTime<Utc>), RepoNyxError> {
        #[derive(Deserialize)]
        struct SetupStateResponse {
            state: String,
            install_url: String,
            expires_at: DateTime<Utc>,
        }
        let response = self
            .http
            .post(format!("{}/setup/state", self.base_url))
            .header("x-reponyx-setup-token", setup_token)
            .send()
            .await
            .map_err(transport_error)?;
        let body: SetupStateResponse = parse_response(response).await?;
        Ok((body.state, body.install_url, body.expires_at))
    }

    /// Request a pairing code for an installation. Requires the
    /// broker setup token. Returns the code (show it to the user)
    /// and its expiry (`None`: pairing codes never expire).
    pub async fn request_pairing_code(
        &self,
        setup_token: &str,
        installation_id: &str,
        device_name: &str,
        device_identifier: &str,
    ) -> Result<(String, Option<DateTime<Utc>>), RepoNyxError> {
        #[derive(Deserialize)]
        struct PairingRequestResponse {
            pairing_code: String,
            expires_at: Option<DateTime<Utc>>,
        }
        let response = self
            .http
            .post(format!("{}/pairing/request", self.base_url))
            .header("x-reponyx-setup-token", setup_token)
            .json(&serde_json::json!({
                "installation_id": installation_id,
                "device_name": device_name,
                "device_identifier": device_identifier,
            }))
            .send()
            .await
            .map_err(transport_error)?;
        let body: PairingRequestResponse = parse_response(response).await?;
        Ok((body.pairing_code, body.expires_at))
    }

    /// Confirm a pairing code. Requires the broker setup token.
    /// Returns the device token (store it in the OS credential
    /// store; it is shown once) and the installation id.
    pub async fn confirm_pairing(
        &self,
        setup_token: &str,
        pairing_code: &str,
        device_identifier: &str,
    ) -> Result<(String, String), RepoNyxError> {
        #[derive(Deserialize)]
        struct PairingConfirmResponse {
            device_token: String,
            installation_id: String,
        }
        let response = self
            .http
            .post(format!("{}/pairing/confirm", self.base_url))
            .header("x-reponyx-setup-token", setup_token)
            .json(&serde_json::json!({
                "pairing_code": pairing_code,
                "device_identifier": device_identifier,
            }))
            .send()
            .await
            .map_err(transport_error)?;
        let body: PairingConfirmResponse = parse_response(response).await?;
        Ok((body.device_token, body.installation_id))
    }

    /// Pairing-code status. Requires the broker setup token.
    pub async fn pairing_status(
        &self,
        setup_token: &str,
        code: &str,
    ) -> Result<PairingStatus, RepoNyxError> {
        let response = self
            .http
            .get(format!("{}/pairing/status", self.base_url))
            .header("x-reponyx-setup-token", setup_token)
            .query(&[("code", code)])
            .send()
            .await
            .map_err(transport_error)?;
        parse_response(response).await
    }

    /// Revoke a device's codes and tokens. Requires the broker setup
    /// token. Returns true when anything was revoked.
    pub async fn revoke_device(
        &self,
        setup_token: &str,
        device_identifier: &str,
    ) -> Result<bool, RepoNyxError> {
        #[derive(Deserialize)]
        struct RevokeResponse {
            revoked: bool,
        }
        let response = self
            .http
            .post(format!("{}/pairing/revoke", self.base_url))
            .header("x-reponyx-setup-token", setup_token)
            .json(&serde_json::json!({ "device_identifier": device_identifier }))
            .send()
            .await
            .map_err(transport_error)?;
        let body: RevokeResponse = parse_response(response).await?;
        Ok(body.revoked)
    }

    /// List installations recorded by the broker. Requires the
    /// broker setup token.
    pub async fn list_installations(
        &self,
        setup_token: &str,
    ) -> Result<Vec<BrokerInstallation>, RepoNyxError> {
        let response = self
            .http
            .get(format!("{}/installations", self.base_url))
            .header("x-reponyx-setup-token", setup_token)
            .send()
            .await
            .map_err(transport_error)?;
        parse_response(response).await
    }

    /// Mint an installation access token from the broker. Requires
    /// the device token issued at pairing. `force_refresh` bypasses
    /// the broker's cache (use after a 401 with a cached token).
    /// Returns the token and its expiry.
    pub async fn mint_installation_token(
        &self,
        device_token: &str,
        installation_id: &str,
        force_refresh: bool,
    ) -> Result<(String, DateTime<Utc>), RepoNyxError> {
        #[derive(Deserialize)]
        struct MintResponse {
            token: String,
            expires_at: DateTime<Utc>,
        }
        let response = self
            .http
            .post(format!(
                "{}/installations/{}/token",
                self.base_url, installation_id
            ))
            .header("Authorization", format!("Bearer {device_token}"))
            .json(&serde_json::json!({ "force_refresh": force_refresh }))
            .send()
            .await
            .map_err(transport_error)?;
        let body: MintResponse = parse_response(response).await?;
        Ok((body.token, body.expires_at))
    }
}

/// Parse a broker response: success → the JSON body; error status →
/// the broker's error envelope (code + message), never the raw body
/// verbatim (it could contain anything).
async fn parse_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, RepoNyxError> {
    let status = response.status();
    if status.is_success() {
        return response.json::<T>().await.map_err(|e| {
            RepoNyxError::new(
                ErrorCode::InternalError,
                "Auth Broker returned unparsable JSON",
            )
            .with_details(e.to_string())
        });
    }
    let body_text = response.text().await.unwrap_or_default();
    if let Ok(BrokerErrorEnvelope { error }) =
        serde_json::from_str::<BrokerErrorEnvelope>(&body_text)
    {
        return Err(error_from_code(&error.code, error.message));
    }
    Err(RepoNyxError::new(
        ErrorCode::InternalError,
        format!("Auth Broker returned HTTP {}", status.as_u16()),
    )
    .with_details(body_text.chars().take(300).collect::<String>()))
}

fn transport_error(error: reqwest::Error) -> RepoNyxError {
    if error.is_timeout() {
        RepoNyxError::new(
            ErrorCode::OperationTimedOut,
            "Auth Broker request timed out",
        )
        .with_details(error.to_string())
    } else {
        RepoNyxError::new(
            ErrorCode::NetworkUnavailable,
            "cannot reach the Auth Broker",
        )
        .with_details(error.to_string())
    }
}

/// A cached installation token: the value and when GitHub says it
/// expires. The value is never printed (`Token`'s `Debug` redacts
/// it, and this struct is never debug-printed with the value).
struct CachedToken {
    value: String,
    expires_at: DateTime<Utc>,
}

/// A [`CredentialSource`] backed by the Auth Broker: mints
/// installation tokens for a paired device, caches each until 5
/// minutes before expiry, and drops the cache on `invalidate()`
/// (called by the client after a 401). The device token is held in
/// memory only; the desktop loads it from the OS credential store
/// when constructing the source.
pub struct BrokerTokenSource {
    client: BrokerClient,
    installation_id: String,
    /// The device token. Secret — redacted in `Debug`.
    device_token: String,
    cache: Mutex<Option<CachedToken>>,
}

impl BrokerTokenSource {
    pub fn new(
        broker_base_url: &str,
        installation_id: impl Into<String>,
        device_token: impl Into<String>,
    ) -> Result<Self, RepoNyxError> {
        Ok(Self {
            client: BrokerClient::new(broker_base_url)?,
            installation_id: installation_id.into(),
            device_token: device_token.into(),
            cache: Mutex::new(None),
        })
    }

    pub fn installation_id(&self) -> &str {
        &self.installation_id
    }

    pub fn broker_base_url(&self) -> &str {
        self.client.base_url()
    }

    /// Mint a fresh installation token from the broker (bypassing
    /// the cache) and store it. Shared by `token()` and `invalidate()`
    /// recovery.
    async fn refresh(&self) -> Result<Token, RepoNyxError> {
        let (value, expires_at) = self
            .client
            .mint_installation_token(&self.device_token, &self.installation_id, false)
            .await?;
        let expires_in_secs = expires_at
            .signed_duration_since(Utc::now())
            .num_seconds()
            .max(0) as u64;
        *self.cache.lock().await = Some(CachedToken {
            value: value.clone(),
            expires_at,
        });
        Ok(Token::new(
            value,
            CredentialKind::InstallationToken,
            "x-access-token",
            Some(expires_in_secs),
        ))
    }
}

impl std::fmt::Debug for BrokerTokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerTokenSource")
            .field("broker_base_url", &self.client.base_url())
            .field("installation_id", &self.installation_id)
            .field("device_token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CredentialSource for BrokerTokenSource {
    async fn token(&self) -> Result<Token, RepoNyxError> {
        if let Some(cached) = self.cache.lock().await.as_ref() {
            // Serve from the cache until 5 minutes before expiry.
            if Utc::now() + Duration::from_secs(300) < cached.expires_at {
                let expires_in_secs = cached
                    .expires_at
                    .signed_duration_since(Utc::now())
                    .num_seconds()
                    .max(0) as u64;
                return Ok(Token::new(
                    cached.value.clone(),
                    CredentialKind::InstallationToken,
                    "x-access-token",
                    Some(expires_in_secs),
                ));
            }
        }
        self.refresh().await
    }

    async fn invalidate(&self) {
        *self.cache.lock().await = None;
    }
}
