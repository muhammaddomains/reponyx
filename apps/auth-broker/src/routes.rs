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
//! - `POST /pairing/status` — pairing-code status (setup-token
//!   auth). POST with the code in the JSON body, never in a URL
//!   query: query strings land in access logs and browser history
//!   (audit B5).
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

use crate::auth::{digests_equal, sha256_bytes};
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
            ErrorCode::WebSessionExpired => StatusCode::UNAUTHORIZED,
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
///
/// The comparison is constant-time over the SHA-256 digests (audit
/// A2), and failed presentations count toward a lockout (audit B2):
/// after `setup_auth_max_failures` consecutive failures, setup-token
/// auth returns 429 for `setup_auth_lockout`. A successful
/// presentation resets the counter. Individual failures are not
/// audited — a brute-force loop would flood the audit table — only
/// the lockout transition is.
struct SetupToken;

impl SetupToken {
    async fn check(state: &BrokerState, headers: &HeaderMap) -> Result<(), ApiError> {
        if let Some(wait) = state.setup_auth_locked_out() {
            return Err(ApiError::rate_limited(wait));
        }
        let presented = headers
            .get("x-reponyx-setup-token")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        let matches = !presented.is_empty()
            && digests_equal(
                &sha256_bytes(presented),
                &sha256_bytes(state.config.setup_token.expose()),
            );
        if !matches {
            if let Some(wait) = state.note_setup_auth_failure() {
                state
                    .audit(
                        "setup-token",
                        "setup.auth_locked_out",
                        "failure",
                        format!(
                            "setup-token auth locked out for {}s after too many failed presentations",
                            wait.as_secs()
                        ),
                    )
                    .await;
                return Err(ApiError::rate_limited(wait));
            }
            return Err(ApiError::new(RepoNyxError::new(
                ErrorCode::AuthenticationExpired,
                "invalid or missing setup token",
            )));
        }
        state.reset_setup_auth_failures();
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
            None => {
                // An expired token is a different problem from a
                // wrong one: the operator pairs again instead of
                // hunting for a typo (audit A1).
                if state.device_token_expired(presented).await {
                    return Err(ApiError::new(RepoNyxError::new(
                        ErrorCode::AuthenticationExpired,
                        "device token expired; pair this device again",
                    )));
                }
                Err(ApiError::new(RepoNyxError::new(
                    ErrorCode::AuthenticationExpired,
                    "invalid or revoked device token",
                )))
            }
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<SetupStateResponse>, ApiError> {
    SetupToken::check(&state, &headers).await?;
    // Audit B6: issuing states was un-rate-limited; a local process
    // could mint states (and flood the audit table via the callback)
    // without bound.
    let ip = client_ip(ConnectInfo(addr));
    rate_limit(
        &state,
        &format!("setup-state:ip:{ip}"),
        state.config.setup_state_per_minute_per_ip,
        state.config.setup_state_per_minute_per_ip / 60.0,
    )?;
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
    // Audit B3: the callback had no rate limit, so a local process
    // could flood the audit table and record arbitrary installations
    // with an observed `state`. Rate-limit first — before any audit
    // write or state consumption — so the flood cannot even start.
    rate_limit(
        &state,
        &format!("callback:ip:{ip}"),
        state.config.callback_per_minute_per_ip,
        state.config.callback_per_minute_per_ip / 60.0,
    )?;
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
    // Audit B3: verify the installation really belongs to THIS
    // GitHub App before recording it — via the web, which holds the
    // key (W4). A web failure fails closed — a forged callback must
    // not slip through during a web outage. The web also returns
    // the account login, which the callback itself never carries.
    let account_login = match state.verify_installation(&installation_id).await {
        Ok(Some(login)) => Some(login),
        Ok(None) => {
            state
                .audit(
                    &ip,
                    "callback.failed",
                    "failure",
                    format!(
                        "installation {installation_id} is not an installation of this GitHub App"
                    ),
                )
                .await;
            return Err(ApiError::new(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "installation not found for this GitHub App",
            )));
        }
        Err(err) => return Err(ApiError::new(err)),
    };
    state
        .record_installation(&installation_id, &setup_action, account_login)
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
    /// `None`: pairing codes never expire.
    expires_at: Option<DateTime<Utc>>,
}

async fn pairing_request(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PairingRequestBody>,
) -> Result<Json<PairingRequestResponse>, ApiError> {
    SetupToken::check(&state, &headers).await?;
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
    SetupToken::check(&state, &headers).await?;
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

/// The pairing code travels in the JSON body, never in a URL
/// query (audit B5): query strings land in access logs and browser
/// history, and the code is a bearer credential for the device.
#[derive(Debug, Deserialize)]
struct PairingStatusBody {
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
    Json(body): Json<PairingStatusBody>,
) -> Result<Json<PairingStatusResponse>, ApiError> {
    SetupToken::check(&state, &headers).await?;
    let info: PairingStatusInfo = state.pairing_status(&body.code).await;
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
    SetupToken::check(&state, &headers).await?;
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
    // The device token authenticates the *device*; the gate is the
    // installation's paired device, not the installation the token
    // happened to be confirmed for. One device token covers every
    // installation paired to that device — the desktop stores one
    // token per device, overwritten on each pairing, so gating on
    // the token's installation made every earlier installation's
    // Validate fail with "device is not paired to this
    // installation" even though the installation really was paired
    // to this device.
    let paired = state
        .installation(&installation_id)
        .await
        .and_then(|record| record.paired_device)
        .is_some_and(|paired| paired == device.0.device_identifier);
    if !paired {
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
    let minted = state
        .mint_token(
            &installation_id,
            &device.0.device_identifier,
            body.force_refresh,
        )
        .await;
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
    SetupToken::check(&state, &headers).await?;
    // The recorded login stands: the install callback records the
    // web-verified login (W4), so no GitHub call is needed here.
    //
    // This list deliberately records NOTHING: the install callback
    // is the only way an installation enters the store, because it
    // is the only path with proof the user owns it — it verifies
    // the installation belongs to this App and consumes the
    // one-time setup state from the browser flow the user just
    // completed. Recording from a list would pull in accounts the
    // user never chose — the 2026-11-28 sync-79 discovery did
    // exactly that and was reverted for it. An installation the
    // user installs or configures through the Install button
    // arrives via the callback (setup_action install or update);
    // the desktop polls this list after the click until it appears.
    Ok(Json(
        state
            .list_installations()
            .await
            .into_iter()
            .map(|record| InstallationResponse {
                installation_id: record.installation_id.clone(),
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
    SetupToken::check(&state, &headers).await?;
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
        .route("/pairing/status", post(pairing_status))
        .route("/pairing/revoke", post(pairing_revoke))
        .route("/installations/{id}/token", post(mint_token))
        .route("/installations", get(list_installations))
        .route("/audit", get(list_audit))
        .with_state(state)
}
