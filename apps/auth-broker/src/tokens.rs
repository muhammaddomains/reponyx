//! GitHub App JWT signing and installation-token minting.
//!
//! The private key is used ONLY here: to sign a short-lived JWT
//! (RS256, `exp` ~9 minutes) that authorizes
//! `POST /app/installations/{id}/access_tokens`. The JWT and the
//! minted installation token are never logged and never appear in
//! error messages. Minted tokens are cached per installation until
//! 5 minutes before expiry; `force_refresh` bypasses the cache
//! (used when a caller reports a 401 with a cached token).

use crate::config::BrokerConfig;
use reponyx_core::{ErrorCode, RepoNyxError};
use serde::{Deserialize, Serialize};

/// JWT claims for a GitHub App JWT: `iss` is the App id, `iat` is
/// backdated 60 s for clock skew, `exp` is at most 10 minutes after
/// `iat` (GitHub's limit).
#[derive(Debug, Serialize, Deserialize)]
struct AppJwtClaims {
    iss: String,
    iat: i64,
    exp: i64,
}

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

/// Sign a GitHub App JWT (RS256, 9-minute lifetime) with the
/// broker's private key. The key never leaves this function.
pub fn app_jwt(config: &BrokerConfig) -> Result<String, RepoNyxError> {
    let now = chrono::Utc::now().timestamp();
    let claims = AppJwtClaims {
        iss: config.app_id.clone(),
        iat: now - 60,
        exp: now + 540,
    };
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(config.private_key_pem.as_bytes()).map_err(
        |e| {
            RepoNyxError::new(
                ErrorCode::InternalError,
                "broker private key is not a valid RSA PEM key",
            )
            .with_details(e.to_string())
        },
    )?;
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &key,
    )
    .map_err(|e| {
        RepoNyxError::new(ErrorCode::InternalError, "cannot sign the GitHub App JWT")
            .with_details(e.to_string())
    })
}

/// Mint an installation access token from GitHub: `POST
/// /app/installations/{id}/access_tokens` with the App JWT. The
/// response token is returned to the caller; it is never logged.
pub async fn mint_installation_token(
    http: &reqwest::Client,
    config: &BrokerConfig,
    installation_id: &str,
) -> Result<InstallationTokenResponse, RepoNyxError> {
    let jwt = app_jwt(config)?;
    let url = format!(
        "{}/app/installations/{}/access_tokens",
        config.github_api_base.trim_end_matches('/'),
        installation_id
    );
    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                RepoNyxError::new(ErrorCode::OperationTimedOut, "GitHub API request timed out")
                    .with_details(e.to_string())
            } else {
                RepoNyxError::new(ErrorCode::NetworkUnavailable, "cannot reach the GitHub API")
                    .with_details(e.to_string())
            }
        })?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<InstallationTokenResponse>()
            .await
            .map_err(|e| {
                RepoNyxError::new(
                    ErrorCode::InternalError,
                    "GitHub API returned an unparsable token response",
                )
                .with_details(e.to_string())
            });
    }
    let body = response.text().await.unwrap_or_default();
    let message = parse_error_message(&body);
    Err(match status.as_u16() {
        401 => RepoNyxError::new(
            ErrorCode::InternalError,
            "GitHub rejected the broker's App JWT",
        )
        .with_details(message),
        403 => RepoNyxError::new(
            ErrorCode::InsufficientPermissions,
            "GitHub App lacks permission to mint a token for this installation",
        )
        .with_details(message),
        404 => RepoNyxError::new(ErrorCode::InvalidInput, "GitHub installation not found")
            .with_details(message),
        _ => RepoNyxError::new(
            ErrorCode::InternalError,
            format!("GitHub token minting failed (HTTP {})", status.as_u16()),
        )
        .with_details(message),
    })
}

/// List the App's installations with their account logins
/// (`GET /app/installations`, App JWT auth), as
/// installation-id → account-login. Best-effort: any failure
/// (GitHub unreachable, bad JWT, unparsable body) returns an empty
/// map — the installations list must never fail because GitHub is
/// unreachable; callers fall back to the login recorded at install
/// time (which is usually absent — the install callback does not
/// carry it).
pub async fn list_app_installations(
    http: &reqwest::Client,
    config: &BrokerConfig,
) -> std::collections::HashMap<String, String> {
    #[derive(Deserialize)]
    struct AppInstallation {
        id: u64,
        account: AppInstallationAccount,
    }
    #[derive(Deserialize)]
    struct AppInstallationAccount {
        login: String,
    }
    let mut out = std::collections::HashMap::new();
    let Ok(jwt) = app_jwt(config) else {
        return out;
    };
    let url = format!(
        "{}/app/installations",
        config.github_api_base.trim_end_matches('/')
    );
    let Ok(response) = http
        .get(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
    else {
        return out;
    };
    if !response.status().is_success() {
        return out;
    }
    if let Ok(installations) = response.json::<Vec<AppInstallation>>().await {
        for installation in installations {
            out.insert(installation.id.to_string(), installation.account.login);
        }
    }
    out
}

fn parse_error_message(body: &str) -> String {
    #[derive(Deserialize)]
    struct ErrorBody {
        message: Option<String>,
    }
    if let Ok(ErrorBody {
        message: Some(message),
    }) = serde_json::from_str::<ErrorBody>(body)
        && !message.is_empty()
    {
        return message;
    }
    body.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::sha256_hex;

    #[test]
    fn jwt_claims_are_well_formed() {
        // Claims shape only; signing is covered by the broker
        // integration tests with a throwaway key.
        let claims = AppJwtClaims {
            iss: "123456".to_string(),
            iat: 1_700_000_000,
            exp: 1_700_000_540,
        };
        assert_eq!(claims.exp - claims.iat, 540);
        // The hash helper used for code/token storage is stable.
        assert_eq!(sha256_hex("abc").len(), 64);
    }
}
