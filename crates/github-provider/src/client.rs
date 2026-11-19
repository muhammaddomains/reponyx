//! The GitHub API client: one `reqwest` client, auth injection,
//! retries with exponential backoff + jitter, and error mapping.
//!
//! Safety rules, enforced here and nowhere else in the provider:
//! - The token is fetched from the [`CredentialSource`] per request
//!   and placed only in the `Authorization` header. It is never put
//!   in a URL, never logged, and never copied into an error message
//!   or `Debug` output. Log lines carry method + path + status only.
//! - Retries happen only for transient failures (5xx, network errors,
//!   rate limits). Permanent auth/permission errors (401/403) are
//!   never retried.
//! - Rate-limit waits are capped: the client sleeps at most
//!   `max_rate_limit_wait` (default 300 s) per wait and at most
//!   `max_rate_limit_waits` times per request; beyond that it returns
//!   `RateLimitExceeded` with the reset time in `details`, so the
//!   caller can resume later from the same pagination cursor. Library
//!   code never sleeps unboundedly.

use crate::credentials::CredentialSource;
use crate::pagination::parse_next_link;
use crate::rate_limit::RateLimitState;
use reponyx_core::{ErrorCode, RepoNyxError};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Default per-request timeout (connect + read).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default connect timeout.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default number of attempts for transient (5xx / network) failures.
const DEFAULT_MAX_ATTEMPTS: usize = 3;
/// Longest single sleep while waiting out a rate limit.
const DEFAULT_MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(300);
/// Most rate-limit waits per request before giving up to the caller.
const DEFAULT_MAX_RATE_LIMIT_WAITS: usize = 3;
/// First backoff step for transient retries; doubles each attempt.
const BACKOFF_BASE: Duration = Duration::from_millis(500);
/// Backoff never exceeds this, jitter included.
const BACKOFF_CAP: Duration = Duration::from_secs(8);

/// A parsed API response: the body plus pagination / rate-limit
/// metadata the caller needs to continue.
#[derive(Debug)]
pub struct ApiResponse<T> {
    pub body: T,
    /// The `rel="next"` URL from the `Link` header, if any.
    pub next_link: Option<String>,
    /// The rate-limit state observed on this response.
    pub rate_limit: RateLimitState,
}

/// The error shape GitHub returns (`{"message": ..., ...}`). Only the
/// message is read; unknown fields are ignored.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

/// The GitHub API client. Cheap to clone (`Arc` internals); the
/// `CredentialSource` is consulted once per request.
#[derive(Clone)]
pub struct GitHubClient {
    base_url: String,
    http: reqwest::Client,
    source: Arc<dyn CredentialSource>,
    rate_limit: Arc<Mutex<RateLimitState>>,
    max_attempts: usize,
    max_rate_limit_wait: Duration,
    max_rate_limit_waits: usize,
}

impl GitHubClient {
    /// Build a client for `base_url` (default
    /// `https://api.github.com`; tests point it at a mock server).
    /// The base URL must be `http(s)://...` with no trailing slash
    /// (a trailing slash is tolerated and stripped).
    pub fn new(base_url: &str, source: Arc<dyn CredentialSource>) -> Result<Self, RepoNyxError> {
        let base = base_url.trim_end_matches('/').to_string();
        if !(base.starts_with("https://") || base.starts_with("http://")) {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "GitHub API base URL must start with http:// or https://",
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .user_agent(format!("RepoNyx/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| {
                RepoNyxError::new(ErrorCode::InternalError, "cannot build HTTP client")
                    .with_details(e.to_string())
            })?;
        Ok(Self {
            base_url: base,
            http,
            source,
            rate_limit: Arc::new(Mutex::new(RateLimitState::default())),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            max_rate_limit_wait: DEFAULT_MAX_RATE_LIMIT_WAIT,
            max_rate_limit_waits: DEFAULT_MAX_RATE_LIMIT_WAITS,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetch a fresh token from the configured source. This is the
    /// same path the request loop uses, so credential acquisition and
    /// API auth can never disagree about which token is current.
    pub(crate) async fn token(&self) -> Result<crate::credentials::Token, RepoNyxError> {
        self.source.token().await
    }

    /// Snapshot of the latest observed rate-limit state.
    pub async fn rate_limit(&self) -> RateLimitState {
        *self.rate_limit.lock().await
    }

    pub fn with_max_attempts(mut self, max_attempts: usize) -> Self {
        self.max_attempts = max_attempts.max(1);
        self
    }

    pub fn with_max_rate_limit_wait(mut self, wait: Duration) -> Self {
        self.max_rate_limit_wait = wait;
        self
    }

    /// GET `url` (absolute, or a path relative to the base URL) and
    /// parse the JSON body, with retries for transient failures.
    /// `url` must never contain a credential.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<ApiResponse<T>, RepoNyxError> {
        let url = self.resolve_url(url);
        let mut attempts = 0usize;
        let mut rate_limit_waits = 0usize;
        loop {
            let token = self.source.token().await?;
            let result = self
                .http
                .get(&url)
                .header("Authorization", token.authorization_header())
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await;
            match result {
                Err(e) => {
                    attempts += 1;
                    if attempts >= self.max_attempts || !is_transient_transport(&e) {
                        return Err(transport_error(&e));
                    }
                    tracing::debug!(
                        status = "transport-error",
                        attempts,
                        "GitHub request failed, retrying"
                    );
                    tokio::time::sleep(backoff_delay(attempts)).await;
                }
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let mut state = self.rate_limit.lock().await;
                    state.update_from_headers(&headers);
                    let observed = *state;
                    drop(state);
                    let next_link =
                        parse_next_link(headers.get("link").and_then(|v| v.to_str().ok()));
                    if status.is_success() {
                        let body = response.json::<T>().await.map_err(|e| {
                            RepoNyxError::new(
                                ErrorCode::InternalError,
                                "GitHub API returned unparsable JSON",
                            )
                            .with_details(e.to_string())
                        })?;
                        return Ok(ApiResponse {
                            body,
                            next_link,
                            rate_limit: observed,
                        });
                    }
                    let body_text = response.text().await.unwrap_or_default();
                    let message = parse_error_message(&body_text);
                    if status.as_u16() == 429 || is_rate_limit_response(status, &headers, &message)
                    {
                        let wait = observed.wait_duration(
                            headers.get("retry-after").and_then(|v| v.to_str().ok()),
                        );
                        match wait {
                            Some(wait)
                                if wait <= self.max_rate_limit_wait
                                    && rate_limit_waits < self.max_rate_limit_waits =>
                            {
                                rate_limit_waits += 1;
                                tracing::debug!(
                                    wait_secs = wait.as_secs(),
                                    "GitHub rate limit hit, waiting then retrying"
                                );
                                tokio::time::sleep(wait).await;
                                continue;
                            }
                            _ => {
                                return Err(rate_limit_exceeded_with_reset(observed, message));
                            }
                        }
                    }
                    if status.is_server_error() {
                        attempts += 1;
                        if attempts >= self.max_attempts {
                            return Err(RepoNyxError::new(
                                ErrorCode::InternalError,
                                format!(
                                    "GitHub API returned {} after {} attempts",
                                    status.as_u16(),
                                    attempts
                                ),
                            )
                            .with_details(message));
                        }
                        tracing::debug!(
                            status = status.as_u16(),
                            attempts,
                            "GitHub server error, retrying"
                        );
                        tokio::time::sleep(backoff_delay(attempts)).await;
                        continue;
                    }
                    return Err(map_status_error(status, &message));
                }
            }
        }
    }

    fn resolve_url(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{}{}", self.base_url, url)
        }
    }
}

impl std::fmt::Debug for GitHubClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubClient")
            .field("base_url", &self.base_url)
            .field("max_attempts", &self.max_attempts)
            .finish_non_exhaustive()
    }
}

fn rate_limit_exceeded_with_reset(state: RateLimitState, message: String) -> RepoNyxError {
    let details = match state.reset_at {
        Some(reset_at) => {
            format!("GitHub rate limit exceeded; window resets at {reset_at}. {message}")
        }
        None => format!("GitHub rate limit exceeded. {message}"),
    };
    RepoNyxError::new(ErrorCode::RateLimitExceeded, "GitHub rate limit exceeded")
        .with_details(details)
}

/// Whether a transport error is worth retrying: connection failures
/// and timeouts are transient; anything else (bad URL, TLS config)
/// is not.
fn is_transient_transport(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout()
}

fn transport_error(error: &reqwest::Error) -> RepoNyxError {
    if error.is_timeout() {
        RepoNyxError::new(ErrorCode::OperationTimedOut, "GitHub API request timed out")
            .with_details(error.to_string())
    } else {
        RepoNyxError::new(ErrorCode::NetworkUnavailable, "cannot reach the GitHub API")
            .with_details(error.to_string())
    }
}

/// A 403 (or 429) is a rate-limit response when the server says so:
/// remaining quota is 0, or the message mentions the rate limit.
fn is_rate_limit_response(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    message: &str,
) -> bool {
    if status.as_u16() == 429 {
        return true;
    }
    if headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        == Some(0)
    {
        return true;
    }
    message.to_lowercase().contains("rate limit")
}

/// Map a non-retryable HTTP status to a RepoNyx error code.
/// 401 → the credential is expired/invalid; 403 → revoked when the
/// message says the grant was suspended/revoked, otherwise the token
/// lacks the permission; 404 → the resource does not exist (or is
/// not visible to this token).
fn map_status_error(status: reqwest::StatusCode, message: &str) -> RepoNyxError {
    match status.as_u16() {
        401 => RepoNyxError::new(
            ErrorCode::AuthenticationExpired,
            "GitHub credential rejected (401)",
        )
        .with_details(message.to_string()),
        403 => {
            let lower = message.to_lowercase();
            if lower.contains("revok") || lower.contains("suspend") {
                RepoNyxError::new(
                    ErrorCode::AuthenticationRevoked,
                    "GitHub authorization revoked or suspended (403)",
                )
                .with_details(message.to_string())
            } else {
                RepoNyxError::new(
                    ErrorCode::InsufficientPermissions,
                    "GitHub token lacks the required permission (403)",
                )
                .with_details(message.to_string())
            }
        }
        404 => RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            "GitHub resource not found (404)",
        )
        .with_details(message.to_string()),
        code => RepoNyxError::new(
            ErrorCode::InternalError,
            format!("GitHub API returned unexpected status {code}"),
        )
        .with_details(message.to_string()),
    }
}

/// Extract the `message` field from a GitHub error body; fall back to
/// the raw body truncated to 500 chars (never more — error bodies
/// must stay out of logs in full).
fn parse_error_message(body: &str) -> String {
    if let Ok(ErrorBody {
        message: Some(message),
    }) = serde_json::from_str::<ErrorBody>(body)
        && !message.is_empty()
    {
        return message;
    }
    body.chars().take(500).collect()
}

/// Exponential backoff with jitter: 500 ms, 1 s, 2 s, 4 s, capped at
/// 8 s. Jitter comes from the system clock's sub-second nanos (no
/// extra dependency); it only decorrelates retries, it need not be
/// cryptographic.
fn backoff_delay(attempt: usize) -> Duration {
    let shift = attempt.saturating_sub(1).min(4) as u32;
    let base = BACKOFF_BASE.saturating_mul(1 << shift).min(BACKOFF_CAP);
    let jitter_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() % 250_000_000)
        .unwrap_or(0);
    base.saturating_add(Duration::from_nanos(u64::from(jitter_nanos)))
        .min(BACKOFF_CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        assert!(backoff_delay(1) >= Duration::from_millis(500));
        assert!(backoff_delay(2) >= Duration::from_millis(1000));
        assert!(backoff_delay(10) <= BACKOFF_CAP);
    }

    #[test]
    fn error_message_prefers_message_field() {
        assert_eq!(
            parse_error_message(
                r#"{"message":"Bad credentials","documentation_url":"https://docs.github.com"}"#
            ),
            "Bad credentials"
        );
        assert_eq!(parse_error_message("plain text"), "plain text");
    }
}
