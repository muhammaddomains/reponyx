//! Installation-token minting via the web (W4).
//!
//! The broker no longer holds the GitHub App private key: the web is
//! the single holder of the key and mints installation tokens. The
//! broker asks the web (`POST /api/broker/mint`) with the user's web
//! session (read from the OS credential store) and the device
//! identifier, and verifies installations the same way (`GET
//! /api/broker/installations/{id}`). The session token and the minted
//! GitHub token are never logged and never appear in error messages.
//! Minted tokens are cached per installation until 5 minutes before
//! expiry; `force_refresh` bypasses the cache (used when a caller
//! reports a 401 with a cached token).

use reponyx_core::{ErrorCode, RepoNyxError};
use serde::Deserialize;

/// What GitHub returns for an installation access token.
#[derive(Debug, Deserialize)]
pub struct InstallationTokenResponse {
    pub token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub permissions: serde_json::Value,
    #[serde(default)]
    pub repository_selection: Option<String>,
}

/// What the web returns for `POST /api/broker/mint`.
#[derive(Debug, Deserialize)]
struct WebMintResponse {
    token: String,
    #[serde(rename = "expiresAt")]
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// What the web returns for `GET /api/broker/installations/{id}`.
#[derive(Debug, Deserialize)]
struct WebInstallationResponse {
    #[serde(rename = "accountLogin")]
    account_login: String,
}

/// The web's error shape: `{ "error": { "code": "...", "data": {} } }`.
#[derive(Debug, Deserialize)]
struct WebErrorBody {
    error: WebError,
}

#[derive(Debug, Deserialize)]
struct WebError {
    code: String,
}

async fn parse_web_error(response: reqwest::Response) -> (u16, Option<String>) {
    let status = response.status().as_u16();
    let code = response
        .json::<WebErrorBody>()
        .await
        .ok()
        .map(|body| body.error.code);
    (status, code)
}

fn transport_error(err: reqwest::Error) -> RepoNyxError {
    if err.is_timeout() {
        RepoNyxError::new(ErrorCode::OperationTimedOut, "web request timed out")
            .with_details(err.to_string())
    } else {
        RepoNyxError::new(
            ErrorCode::NetworkUnavailable,
            "cannot reach the web (reponyx.bond) to mint a token",
        )
        .with_details(err.to_string())
    }
}

/// Mint an installation access token via the web: `POST
/// {web_base}/api/broker/mint` with the user's session as a Bearer
/// token. The web checks the installation is linked to that user,
/// the device is not revoked, and the linked account is within the
/// package limit — the broker trusts the web's answer and never sees
/// the App key. The response token is returned to the caller; it is
/// never logged.
pub async fn mint_installation_token_via_web(
    http: &reqwest::Client,
    web_base_url: &str,
    session: &str,
    installation_id: &str,
    device_identifier: &str,
) -> Result<InstallationTokenResponse, RepoNyxError> {
    let url = format!("{}/api/broker/mint", web_base_url.trim_end_matches('/'));
    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({
            "installationId": installation_id,
            "deviceIdentifier": device_identifier,
        }))
        .send()
        .await
        .map_err(transport_error)?;
    let status = response.status();
    if status.is_success() {
        return response.json::<WebMintResponse>().await.map_or_else(
            |e| {
                Err(RepoNyxError::new(
                    ErrorCode::InternalError,
                    "web returned an unparsable token response",
                )
                .with_details(e.to_string()))
            },
            |minted| {
                Ok(InstallationTokenResponse {
                    token: minted.token,
                    expires_at: minted.expires_at,
                    permissions: serde_json::Value::Object(Default::default()),
                    repository_selection: None,
                })
            },
        );
    }
    let (status_code, code) = parse_web_error(response).await;
    Err(match (status_code, code.as_deref()) {
        (401, _) => RepoNyxError::new(
            ErrorCode::WebSessionExpired,
            "web session expired — log in again on the desktop",
        ),
        (403, Some("device_revoked")) => RepoNyxError::new(
            ErrorCode::AuthenticationRevoked,
            "device was revoked on the web — pair this device again",
        ),
        (403, Some("limit_exceeded")) => RepoNyxError::new(
            ErrorCode::InsufficientPermissions,
            "package limit reached for linked accounts — upgrade or unlink an account",
        ),
        (404, _) => RepoNyxError::new(
            ErrorCode::InvalidInput,
            "GitHub App installation is not linked to your web account",
        ),
        (503, _) => RepoNyxError::new(
            ErrorCode::InternalError,
            "the web is not configured to mint tokens",
        ),
        _ => RepoNyxError::new(
            ErrorCode::InternalError,
            format!("web token minting failed (HTTP {status_code})"),
        ),
    })
}

/// Verify an installation id against the web: `Some(login)` only
/// when the id is an installation of the shared App (audit B3 — the
/// install callback must not record an arbitrary `installation_id`
/// handed to it with a valid `state`). `None` when the web says the
/// installation does not exist. A web failure is an error, not a
/// `None`: the callback must fail closed when it cannot verify, so
/// a forged callback cannot slip through during a web outage. A
/// missing session fails the same way — the callback cannot verify
/// without it.
pub async fn verify_installation_via_web(
    http: &reqwest::Client,
    web_base_url: &str,
    session: Option<&str>,
    installation_id: &str,
) -> Result<Option<String>, RepoNyxError> {
    let Some(session) = session else {
        return Err(RepoNyxError::new(
            ErrorCode::WebSessionExpired,
            "no web session — log in again on the desktop to verify the installation",
        ));
    };
    let url = format!(
        "{}/api/broker/installations/{installation_id}",
        web_base_url.trim_end_matches('/')
    );
    let response = http
        .get(&url)
        .header("Authorization", format!("Bearer {session}"))
        .send()
        .await
        .map_err(transport_error)?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<WebInstallationResponse>()
            .await
            .map_or_else(
                |e| {
                    Err(RepoNyxError::new(
                        ErrorCode::InternalError,
                        "web returned an unparsable installation response",
                    )
                    .with_details(e.to_string()))
                },
                |body| Ok(Some(body.account_login)),
            );
    }
    if status.as_u16() == 404 {
        return Ok(None);
    }
    let (status_code, _) = parse_web_error(response).await;
    Err(match status_code {
        401 => RepoNyxError::new(
            ErrorCode::WebSessionExpired,
            "web session expired — log in again on the desktop",
        ),
        503 => RepoNyxError::new(
            ErrorCode::InternalError,
            "the web is not configured to verify installations",
        ),
        _ => RepoNyxError::new(
            ErrorCode::InternalError,
            format!("web installation lookup failed (HTTP {status_code})"),
        ),
    })
}
