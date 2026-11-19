//! Token sources for the GitHub provider.
//!
//! Phase 5 (Authentication) fills this seam with real sources
//! (see [`broker::BrokerTokenSource`], and `PatTokenSource` in the
//! `reponyx-credential-store` crate): a broker-backed
//! installation-token source and a PAT source that reads from the OS
//! credential store. A source hands out a [`Token`] on demand; it
//! never persists anything itself, and the provider never logs a
//! token value.
//!
//! Tests use [`StaticTokenSource`] with obviously fake tokens only.

use async_trait::async_trait;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::sync::Arc;

/// What kind of GitHub credential a token is. The kind decides the
/// `Authorization` scheme and the Git username (see [`Token`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    /// A GitHub App installation token (`ghs_...`, ~1 h lifetime).
    /// Sent as `Authorization: Bearer <token>`; Git username is
    /// `x-access-token`.
    InstallationToken,
    /// A fine-grained personal access token (`github_pat_...`).
    /// Sent as `Authorization: token <token>` (the classic PAT
    /// scheme; GitHub also accepts `Bearer`); Git username is the
    /// GitHub login, supplied by the source.
    Pat,
}

/// One credential value handed out by a [`CredentialSource`].
///
/// The token value is secret: `Debug` never prints it (see the manual
/// impl below), it is never put in a URL, and it is never included in
/// an error message. Hold it only as long as needed.
#[derive(Clone)]
pub struct Token {
    /// The token value itself. Never log, never print.
    value: String,
    /// What kind of credential this is.
    pub kind: CredentialKind,
    /// Username for Git HTTP Basic auth (`x-access-token` for
    /// installation tokens; the GitHub login for PATs).
    pub username: String,
    /// Seconds until expiry, when the source knows it (installation
    /// tokens: ~3600). `None` means unknown (PATs do not expire on a
    /// schedule the provider can observe).
    pub expires_in_secs: Option<u64>,
}

impl Token {
    pub fn new(
        value: impl Into<String>,
        kind: CredentialKind,
        username: impl Into<String>,
        expires_in_secs: Option<u64>,
    ) -> Self {
        Self {
            value: value.into(),
            kind,
            username: username.into(),
            expires_in_secs,
        }
    }

    /// The `Authorization` header value for this token, per kind.
    pub fn authorization_header(&self) -> String {
        match self.kind {
            CredentialKind::InstallationToken => format!("Bearer {}", self.value),
            CredentialKind::Pat => format!("token {}", self.value),
        }
    }

    /// The raw token value. Callers must treat the return as secret:
    /// use it only to build an `Authorization` header or to hand it to
    /// the short-lived Git credential, then drop it.
    pub fn expose_value(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("kind", &self.kind)
            .field("username", &self.username)
            .field("value", &"<redacted>")
            .field("expires_in_secs", &self.expires_in_secs)
            .finish()
    }
}

/// A pluggable source of GitHub tokens. The provider calls `token()`
/// once per API request, so a source may refresh on every call or
/// cache internally until shortly before expiry — either is fine, but
/// a source must never return an expired token knowingly, and must
/// never log the value it returns.
///
/// Sources are `Send + Sync` and cheap to clone (`Arc` indirection is
/// the provider's job, not the source's).
#[async_trait]
pub trait CredentialSource: Send + Sync {
    /// Hand out a token for the next request. On failure return
    /// `CredentialStoreError` (source unavailable) or
    /// `AuthenticationExpired` (the stored grant is gone and the user
    /// must reconnect) — never a generic error, and never an error
    /// whose message contains the token.
    async fn token(&self) -> Result<Token, RepoNyxError>;

    /// Drop any cached token, so the next `token()` call fetches a
    /// fresh one. The client calls this once after a 401 and retries
    /// the request a single time. Default: no-op (stateless sources
    /// have nothing to drop).
    async fn invalidate(&self) {}
}

/// A token source that always returns the same token. For tests and
/// for Phase 5 to replace with a real source. The token value is
/// redacted in `Debug`.
pub struct StaticTokenSource {
    token: Token,
}

impl StaticTokenSource {
    pub fn new(token: Token) -> Self {
        Self { token }
    }

    /// Convenience constructor for tests: an installation token with
    /// the conventional `x-access-token` username.
    pub fn installation_token(value: impl Into<String>) -> Self {
        Self::new(Token::new(
            value,
            CredentialKind::InstallationToken,
            "x-access-token",
            Some(3600),
        ))
    }

    /// Convenience constructor for tests: a PAT with an explicit
    /// GitHub login as the username.
    pub fn pat(value: impl Into<String>, username: impl Into<String>) -> Self {
        Self::new(Token::new(value, CredentialKind::Pat, username, None))
    }
}

impl std::fmt::Debug for StaticTokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticTokenSource")
            .field("token", &self.token)
            .finish()
    }
}

#[async_trait]
impl CredentialSource for StaticTokenSource {
    async fn token(&self) -> Result<Token, RepoNyxError> {
        Ok(self.token.clone())
    }
}

/// A token source that always fails. Useful in tests that assert the
/// provider surfaces source failures as `CredentialStoreError` and
/// never retries them as if they were transient network errors.
pub struct FailingTokenSource {
    message: String,
}

impl FailingTokenSource {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Debug for FailingTokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FailingTokenSource")
            .field("message", &self.message)
            .finish()
    }
}

#[async_trait]
impl CredentialSource for FailingTokenSource {
    async fn token(&self) -> Result<Token, RepoNyxError> {
        Err(RepoNyxError::new(
            ErrorCode::CredentialStoreError,
            self.message.clone(),
        ))
    }
}

/// Convenience: an `Arc`-wrapped static installation-token source.
pub fn static_installation_token(value: impl Into<String>) -> Arc<dyn CredentialSource> {
    Arc::new(StaticTokenSource::installation_token(value))
}

/// Convenience: an `Arc`-wrapped static PAT source.
pub fn static_pat(
    value: impl Into<String>,
    username: impl Into<String>,
) -> Arc<dyn CredentialSource> {
    Arc::new(StaticTokenSource::pat(value, username))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_debug_redacts_the_value() {
        let token = Token::new(
            "test-token-super-secret-value",
            CredentialKind::InstallationToken,
            "x-access-token",
            Some(3600),
        );
        let debug = format!("{token:?}");
        assert!(!debug.contains("test-token-super-secret-value"));
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("x-access-token"));
    }

    #[test]
    fn authorization_header_depends_on_kind() {
        let installation = Token::new(
            "test-installation-token",
            CredentialKind::InstallationToken,
            "x-access-token",
            Some(3600),
        );
        assert_eq!(
            installation.authorization_header(),
            "Bearer test-installation-token"
        );
        let pat = Token::new("test-pat-value", CredentialKind::Pat, "octocat", None);
        assert_eq!(pat.authorization_header(), "token test-pat-value");
    }

    #[test]
    fn static_source_debug_redacts_the_value() {
        let source = StaticTokenSource::installation_token("test-token-do-not-leak");
        let debug = format!("{source:?}");
        assert!(!debug.contains("test-token-do-not-leak"));
    }
}
