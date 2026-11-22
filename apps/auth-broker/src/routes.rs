//! HTTP routes for the Auth Broker (axum).
//!
//! Endpoints:
//! - `GET /health` — liveness, no secrets, no auth.
//! - `POST /setup/state` — issue a CSRF `state` for the GitHub App
//!   install flow + the install URL (setup-token auth).
//! - `GET /github/callback` — GitHub App install callback; validates
//!   `state` (single-use, expiring), records the installation.
//! - `POST /pairing/request` — request a pairing code (setup-token
//!   auth, per-IP rate limit).
//! - `POST /pairing/confirm` — confirm a code, receive a device
//!   token (setup-token auth, per-IP rate limit).
//! - `GET /pairing/status` — pairing-code status (setup-token auth).
//! - `POST /pairing/revoke` — revoke a device's codes and tokens
//!   (setup-token auth).
//! - `POST /installations/{id}/token` — mint an installation token
//!   (device-token auth, per-device rate limit).
//! - `GET /installations` — list recorded installations
//!   (setup-token auth).
//! - `GET /audit` — audit events (setup-token auth).
//!
//! Authentication:
//! - Setup-token auth: `X-RepoNyx-Setup-Token` header, compared by
//!   SHA-256 hash against the token supplied at broker startup.
//! - Device-token auth: `Authorization: Bearer <device_token>`,
//!   compared by hash against stored device tokens.
//!
//! Error shape: `{ "error": { "code": "...", "message": "..." } }`
//! using `RepoNyxError` codes. Responses and logs never contain a
//! token, code, or key.

use crate::auth::sha256_hex;
use crate::state::{BrokerState, DeviceTokenRecord, PairingStatusInfo};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use reponyx_core::{ErrorCode, RepoNyxError};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

type SharedState = Arc<BrokerState>;

// -- shared shapes ---------------------------------------------------

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

/// An error that renders as `{ "error": { "code", "message" } }`
/// with the matching HTTP status. `retry_after_secs` is set for
/// rate-limit errors and rendered as a `Retry-After` header.
#[derive(Debug)]
struct ApiError {
    error: RepoNyxError,
    retry_after_secs: Option<u64>,
}

impl ApiError {
    fn new(error: RepoNyxError) -> Self {
        Self {
            error,
            retry_after_secs: None,
        }
    }

    fn rate_limited(wait: std::time::Duration) -> Self {
        Self {
            error: RepoNyxError::new(
                ErrorCode::RateLimitExceeded,
                "rate limit exceeded; try again shortly",
            ),
            retry_after_secs: Some(wait.as_secs().max(1)),
        }
    }

    fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self.error.code {
            ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorCode::AuthenticationExpired => StatusCode::UNAUTHORIZED,
            ErrorCode::AuthenticationRevoked => StatusCode::FORBIDDEN,
            ErrorCode::InsufficientPermissions => StatusCode::FORBIDDEN,
            ErrorCode::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::OperationTimedOut => StatusCode::GATEWAY_TIMEOUT,
            ErrorCode::NetworkUnavailable => StatusCode::BAD_GATEWAY,
            ErrorCode::RepositoryNotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<RepoNyxError> for ApiError {
    fn from(err: RepoNyxError) -> Self {
        Self::new(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.error.code.to_string(),
                message: self.error.message.clone(),
            },
        };
        let mut response = (status, Json(body)).into_response();
        if let Some(secs) = self.retry_after_secs
            && let Ok(value) = axum::http::HeaderValue::from_str(&secs.to_string())
        {
            response.headers_mut().insert("retry-after", value);
        }
        response
    }
}

// -- authentication extractors ---------------------------------------

/// Setup-token authentication: the operator's token, supplied at
/// broker startup, presented as `X-RepoNyx-Setup-Token`.
struct SetupToken;

impl SetupToken {
    fn from_headers(state: &BrokerState, headers: &HeaderMap) -> Result<(), ApiError> {
        let presented = headers
            .get("x-reponyx-setup-token")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if presented.is_empty() || sha256_hex(presented) != sha256_hex(&state.config.setup_token) {
            return Err(ApiError::new(RepoNyxError::new(
                ErrorCode::AuthenticationExpired,
                "invalid or missing setup token",
            )));
        }
        Ok(())
    }
}

/// Device-token authentication: `Authorization: Bearer <token>`.
struct DeviceAuth(DeviceTokenRecord);

impl DeviceAuth {
    async fn from_headers(state: &BrokerState, headers: &HeaderMap) -> Result<Self, ApiError> {
        let presented = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .strip_prefix("Bearer ")
            .unwrap_or("")
            .trim();
        if presented.is_empty() {
            return Err(ApiError::new(RepoNyxError::new(
                ErrorCode::AuthenticationExpired,
                "missing device token",
            )));
        }
        match state.device_for_token(presented).await {
            Some(record) => Ok(Self(record)),
            None => Err(ApiError::new(RepoNyxError::new(
                ErrorCode::AuthenticationExpired,
                "invalid or revoked device token",
            ))),
        }
    }
}

fn client_ip(addr: ConnectInfo<SocketAddr>) -> String {
    addr.0.ip().to_string()
}

/// Enforce a token-bucket limit; on exhaustion returns 429 with a
/// `Retry-After` header.
fn rate_limit(
    state: &BrokerState,
    key: &str,
    capacity: f64,
    refill_per_sec: f64,
) -> Result<(), ApiError> {
    match state.rate_limiter.check(key, capacity, refill_per_sec) {
        Ok(()) => Ok(()),
        Err(wait) => Err(ApiError::rate_limited(wait)),
    }
}

// -- handlers ---------------------------------------------------------

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Debug, Serialize)]
struct SetupStateResponse {
    state: String,
    install_url: String,
    expires_at: DateTime<Utc>,
}

async fn setup_state(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<SetupStateResponse>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    let (state_value, expires_at) = state.issue_setup_state().await.map_err(ApiError::new)?;
    let install_url = format!(
        "https://github.com/apps/{}/installations/new?state={}",
        state.config.app_slug, state_value
    );
    state
        .audit(
            "setup",
            "setup.state_issued",
            "success",
            "install state issued",
        )
        .await;
    Ok(Json(SetupStateResponse {
        state: state_value,
        install_url,
        expires_at,
    }))
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    installation_id: Option<String>,
    setup_action: Option<String>,
    state: Option<String>,
    account_login: Option<String>,
}

#[derive(Debug, Serialize)]
struct CallbackResponse {
    status: &'static str,
    installation_id: String,
    setup_action: String,
}

async fn github_callback(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<CallbackQuery>,
) -> Result<Json<CallbackResponse>, ApiError> {
    let ip = client_ip(ConnectInfo(addr));
    let installation_id = query.installation_id.unwrap_or_default();
    let setup_action = query.setup_action.unwrap_or_else(|| "install".to_string());
    let state_value = query.state.unwrap_or_default();
    if installation_id.is_empty() || state_value.is_empty() {
        state
            .audit(
                &ip,
                "callback.failed",
                "failure",
                "missing installation_id or state",
            )
            .await;
        return Err(ApiError::new(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "installation_id and state are required",
        )));
    }
    match state.consume_setup_state(&state_value).await {
        Ok(true) => {}
        Ok(false) => {
            state
                .audit(
                    &ip,
                    "callback.failed",
                    "failure",
                    "invalid, reused, or expired state",
                )
                .await;
            return Err(ApiError::new(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "invalid, reused, or expired state",
            )));
        }
        Err(err) => return Err(ApiError::new(err)),
    }
    state
        .record_installation(&installation_id, &setup_action, query.account_login)
        .await
        .map_err(ApiError::new)?;
    state
        .audit(
            &ip,
            "callback.received",
            "success",
            format!("installation {installation_id} ({setup_action})"),
        )
        .await;
    Ok(Json(CallbackResponse {
        status: "installed",
        installation_id,
        setup_action,
    }))
}

#[derive(Debug, Deserialize)]
struct PairingRequestBody {
    installation_id: String,
    device_name: String,
    device_identifier: String,
}

#[derive(Debug, Serialize)]
struct PairingRequestResponse {
    pairing_code: String,
    expires_at: DateTime<Utc>,
}

async fn pairing_request(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PairingRequestBody>,
) -> Result<Json<PairingRequestResponse>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    let ip = client_ip(ConnectInfo(addr));
    rate_limit(
        &state,
        &format!("pairing:ip:{ip}"),
        state.config.pairing_per_minute_per_ip,
        state.config.pairing_per_minute_per_ip / 60.0,
    )?;
    let (code, expires_at) = state
        .request_pairing(
            &body.installation_id,
            &body.device_name,
            &body.device_identifier,
        )
        .await?;
    state
        .audit(
            &body.device_identifier,
            "pairing.requested",
            "success",
            format!("installation {}", body.installation_id),
        )
        .await;
    Ok(Json(PairingRequestResponse {
        pairing_code: code,
        expires_at,
    }))
}

#[derive(Debug, Deserialize)]
struct PairingConfirmBody {
    pairing_code: String,
    device_identifier: String,
}

#[derive(Debug, Serialize)]
struct PairingConfirmResponse {
    device_token: String,
    installation_id: String,
}

async fn pairing_confirm(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PairingConfirmBody>,
) -> Result<Json<PairingConfirmResponse>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    let ip = client_ip(ConnectInfo(addr));
    rate_limit(
        &state,
        &format!("pairing-confirm:ip:{ip}"),
        20.0,
        20.0 / 60.0,
    )?;
    let (device_token, installation_id) = state
        .confirm_pairing(&body.pairing_code, &body.device_identifier)
        .await?;
    // Safe by construction: ids only, never the code or the token.
    tracing::info!(
        device = %body.device_identifier,
        installation_id = %installation_id,
        "pairing confirmed"
    );
    Ok(Json(PairingConfirmResponse {
        device_token,
        installation_id,
    }))
}

#[derive(Debug, Deserialize)]
struct PairingStatusQuery {
    code: String,
}

#[derive(Debug, Serialize)]
struct PairingStatusResponse {
    status: String,
    installation_id: Option<String>,
    device_identifier: Option<String>,
}

async fn pairing_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<PairingStatusQuery>,
) -> Result<Json<PairingStatusResponse>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    let info: PairingStatusInfo = state.pairing_status(&query.code).await;
    Ok(Json(PairingStatusResponse {
        status: info.status,
        installation_id: info.installation_id,
        device_identifier: info.device_identifier,
    }))
}

#[derive(Debug, Deserialize)]
struct PairingRevokeBody {
    device_identifier: String,
}

#[derive(Debug, Serialize)]
struct PairingRevokeResponse {
    revoked: bool,
}

async fn pairing_revoke(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<PairingRevokeBody>,
) -> Result<Json<PairingRevokeResponse>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    let revoked = state
        .revoke_device(&body.device_identifier)
        .await
        .map_err(ApiError::new)?;
    Ok(Json(PairingRevokeResponse { revoked }))
}

#[derive(Debug, Deserialize)]
struct MintTokenBody {
    #[serde(default)]
    force_refresh: bool,
}

#[derive(Debug, Serialize)]
struct MintTokenResponse {
    token: String,
    expires_at: DateTime<Utc>,
}

async fn mint_token(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(installation_id): Path<String>,
    Json(body): Json<MintTokenBody>,
) -> Result<Json<MintTokenResponse>, ApiError> {
    let device = DeviceAuth::from_headers(&state, &headers).await?;
    if device.0.installation_id != installation_id {
        state
            .audit(
                &device.0.device_identifier,
                "token.mint_failed",
                "failure",
                format!("device not paired to installation {installation_id}"),
            )
            .await;
        return Err(ApiError::new(RepoNyxError::new(
            ErrorCode::InsufficientPermissions,
            "device is not paired to this installation",
        )));
    }
    rate_limit(
        &state,
        &format!("mint:device:{}", device.0.device_identifier),
        state.config.mint_per_minute_per_device,
        state.config.mint_per_minute_per_device / 60.0,
    )?;
    let minted = state.mint_token(&installation_id, body.force_refresh).await;
    match minted {
        Ok(token) => {
            // Safe by construction: ids only, never the token.
            tracing::info!(
                installation_id = %installation_id,
                device = %device.0.device_identifier,
                "installation token minted"
            );
            state
                .audit(
                    &device.0.device_identifier,
                    "token.minted",
                    "success",
                    format!("installation {installation_id}"),
                )
                .await;
            Ok(Json(MintTokenResponse {
                token: token.token,
                expires_at: token.expires_at,
            }))
        }
        Err(err) => {
            state
                .audit(
                    &device.0.device_identifier,
                    "token.mint_failed",
                    "failure",
                    format!("installation {installation_id}: {}", err.code),
                )
                .await;
            Err(ApiError::new(err))
        }
    }
}

#[derive(Debug, Serialize)]
struct InstallationResponse {
    installation_id: String,
    setup_action: String,
    account_login: Option<String>,
    installed_at: DateTime<Utc>,
    paired_device: Option<String>,
}

async fn list_installations(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<InstallationResponse>>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    Ok(Json(
        state
            .list_installations()
            .await
            .into_iter()
            .map(|record| InstallationResponse {
                installation_id: record.installation_id,
                setup_action: record.setup_action,
                account_login: record.account_login,
                installed_at: record.installed_at,
                paired_device: record.paired_device,
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize)]
struct AuditResponse {
    events: Vec<crate::audit::AuditEvent>,
}

async fn list_audit(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<AuditResponse>, ApiError> {
    SetupToken::from_headers(&state, &headers)?;
    Ok(Json(AuditResponse {
        events: state.audit_events(),
    }))
}

/// Build the broker router. `ConnectInfo` requires the server to
/// use `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn app(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/setup/state", post(setup_state))
        .route("/github/callback", get(github_callback))
        .route("/pairing/request", post(pairing_request))
        .route("/pairing/confirm", post(pairing_confirm))
        .route("/pairing/status", get(pairing_status))
        .route("/pairing/revoke", post(pairing_revoke))
        .route("/installations/{id}/token", post(mint_token))
        .route("/installations", get(list_installations))
        .route("/audit", get(list_audit))
        .with_state(state)
}
