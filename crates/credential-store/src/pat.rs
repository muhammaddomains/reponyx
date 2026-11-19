//! Fine-grained PAT account flows (ADR-004).
//!
//! A PAT lives only in the OS credential store (never in SQLite,
//! JSON, logs, or frontend state). The database holds the account
//! row (`authentication_method = fine_grained_pat`) plus optional
//! expiry *metadata* (`pat_expires_at`, V3) — never the token.
//!
//! Flows:
//! - `add_pat_account`: validate the PAT against GitHub, then store
//!   it and create the account.
//! - `replace_pat`: re-validate a new PAT (it must belong to the
//!   same GitHub account), then overwrite the stored value.
//! - `remove_pat`: delete the stored PAT and mark the account
//!   revoked (the account row and its local repositories stay —
//!   nothing is auto-deleted).
//! - `reconnect_pat_account`: re-validate the stored PAT and restore
//!   a `token_expired` / `revoked` / `error` account to `connected`
//!   via the explicit reconnect path (Phase 2 rules).
//! - `validate_pat_account`: validate and refresh
//!   `last_validated_at`; a 401 marks the account `token_expired`.
//!
//! Permanent auth failures are never retried: validation is a
//! single attempt (the Phase 4 client never retries 401/403).

use crate::SecretString;
use crate::store::{CredentialStore, SERVICE_PAT};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reponyx_core::{
    Account, AccountStatus, AccountType, AuthenticationMethod, ErrorCode, Provider, RepoNyxError,
};
use reponyx_database::accounts::{
    NewAccount, create_account, get_account, get_account_by_provider, mark_validated,
    reconnect_account, set_account_status, set_pat_expiry,
};
use reponyx_github_provider::{
    GitHubProvider,
    credentials::{CredentialSource, Token},
};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// GitHub's API base. Tests point the provider at a mock server
/// instead by passing their own `api_base`.
pub const GITHUB_API_BASE: &str = "https://api.github.com";

/// Days before a PAT's expiry when the desktop starts warning.
pub const PAT_EXPIRY_WARNING_DAYS: i64 = 7;

/// Input for adding a PAT account.
#[derive(Debug, Clone)]
pub struct PatInput {
    /// The fine-grained PAT (`github_pat_…`; classic `ghp_…` tokens
    /// are also accepted). Never logged, never stored outside the
    /// credential store.
    pub pat: SecretString,
    /// Human-readable label shown in the UI.
    pub label: Option<String>,
    pub account_type: AccountType,
    /// Optional user-supplied expiry of the PAT. `None` means the
    /// token has no known expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

/// What the desktop shows for a PAT's expiry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatExpiryStatus {
    /// No expiry is known for this account.
    Unknown,
    /// The PAT is valid and not near expiry.
    Valid,
    /// The PAT expires within [`PAT_EXPIRY_WARNING_DAYS`] days.
    ExpiringSoon { days_left: i64 },
    /// The PAT's expiry date has passed.
    Expired,
}

/// Expiry status for an account, from its stored `pat_expires_at`.
pub fn pat_expiry_status(account: &Account, now: DateTime<Utc>) -> PatExpiryStatus {
    match account.pat_expires_at {
        None => PatExpiryStatus::Unknown,
        Some(expires_at) => {
            // Round up: a token expiring in 2 days 23 hours is
            // "3 days left" to the user, not 2 (`num_days`
            // truncates, so it reported 2 a millisecond after the
            // expiry was stored).
            let days_left = ((expires_at - now).num_seconds() + 86_399) / 86_400;
            if expires_at <= now {
                PatExpiryStatus::Expired
            } else if days_left <= PAT_EXPIRY_WARNING_DAYS {
                PatExpiryStatus::ExpiringSoon { days_left }
            } else {
                PatExpiryStatus::Valid
            }
        }
    }
}

/// A PAT must look like a GitHub token: a fine-grained PAT
/// (`github_pat_…`) or a classic PAT (`ghp_…`). Anything else is
/// rejected before any network call, so a pasted username or
/// password can never be sent to GitHub as a token.
fn validate_pat_format(pat: &SecretString) -> Result<(), RepoNyxError> {
    let value = pat.expose();
    if value.is_empty()
        || value.chars().any(|c| c.is_whitespace())
        || !(value.starts_with("github_pat_") || value.starts_with("ghp_"))
    {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "PAT must start with github_pat_ (fine-grained) or ghp_ (classic)",
        ));
    }
    Ok(())
}

fn provider_for_validation(
    api_base: &str,
    pat: &SecretString,
) -> Result<GitHubProvider, RepoNyxError> {
    // The username is irrelevant for `validate_auth` (API auth uses
    // the `Authorization` header only); the real login comes back
    // from `GET /user`.
    let source =
        Arc::new(reponyx_github_provider::credentials::StaticTokenSource::pat(pat.expose(), ""));
    GitHubProvider::new(api_base, source)
}

/// Validate a PAT against GitHub (`GET /user`). A 401 comes back as
/// `AuthenticationExpired`; it is never retried.
async fn validate_pat(
    api_base: &str,
    pat: &SecretString,
) -> Result<reponyx_provider_abstractions::ProviderAccountInfo, RepoNyxError> {
    validate_pat_format(pat)?;
    provider_for_validation(api_base, pat)?
        .validate_auth()
        .await
}

/// Add a PAT account: validate first, then store the PAT and create
/// the account row. Nothing is stored when validation fails. An
/// account for the same GitHub identity already existing is an
/// error — use `replace_pat` to change the token of an account.
pub async fn add_pat_account(
    pool: &SqlitePool,
    store: &Arc<dyn CredentialStore>,
    api_base: &str,
    input: PatInput,
) -> Result<Account, RepoNyxError> {
    let info = validate_pat(api_base, &input.pat).await?;
    if get_account_by_provider(pool, Provider::GitHub, &info.provider_account_id)
        .await?
        .is_some()
    {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "an account for this GitHub identity already exists; replace its PAT instead",
        ));
    }
    let account = create_account(
        pool,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: info.provider_account_id,
            username: info.username,
            display_name: info.display_name,
            label: input.label,
            account_type: input.account_type,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await?;
    store.store(SERVICE_PAT, &account.id.to_string(), input.pat.expose())?;
    if input.expires_at.is_some() {
        return set_pat_expiry(pool, account.id, input.expires_at).await;
    }
    Ok(account)
}

/// Replace the PAT of an existing PAT account. The new PAT must
/// validate to the *same* GitHub identity — a PAT for a different
/// account is rejected, so an account can never be silently
/// re-pointed at somebody else's repositories.
pub async fn replace_pat(
    pool: &SqlitePool,
    store: &Arc<dyn CredentialStore>,
    api_base: &str,
    account_id: Uuid,
    pat: SecretString,
    expires_at: Option<DateTime<Utc>>,
) -> Result<Account, RepoNyxError> {
    let account = get_account(pool, account_id)
        .await?
        .ok_or_else(|| RepoNyxError::new(ErrorCode::InvalidInput, "account not found"))?;
    if account.authentication_method != AuthenticationMethod::FineGrainedPat {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "only PAT accounts can have their PAT replaced",
        ));
    }
    let info = validate_pat(api_base, &pat).await?;
    if info.provider_account_id != account.provider_account_id {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "the replacement PAT belongs to a different GitHub account",
        ));
    }
    store.store(SERVICE_PAT, &account.id.to_string(), pat.expose())?;
    let account = set_pat_expiry(pool, account.id, expires_at).await?;
    match account.status {
        AccountStatus::TokenExpired | AccountStatus::Revoked | AccountStatus::Error => {
            reconnect_account(pool, account.id).await
        }
        _ => mark_validated(pool, account.id).await,
    }
}

/// Remove a PAT account's credential: delete the PAT from the store
/// and mark the account revoked. The account row (and its local
/// repositories) stay — removal never deletes local data.
pub async fn remove_pat(
    pool: &SqlitePool,
    store: &Arc<dyn CredentialStore>,
    account_id: Uuid,
) -> Result<Account, RepoNyxError> {
    let account = get_account(pool, account_id)
        .await?
        .ok_or_else(|| RepoNyxError::new(ErrorCode::InvalidInput, "account not found"))?;
    if account.authentication_method != AuthenticationMethod::FineGrainedPat {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "only PAT accounts can be removed this way",
        ));
    }
    store.delete(SERVICE_PAT, &account.id.to_string())?;
    if account.status == AccountStatus::Revoked {
        return Ok(account);
    }
    set_account_status(pool, account.id, AccountStatus::Revoked).await
}

/// Reconnect a PAT account: validate the *stored* PAT and, on
/// success, restore the account to `connected` via the explicit
/// reconnect path. A missing stored PAT is `AuthenticationExpired`
/// (the user must add the PAT again); a 401 marks the account
/// `token_expired` and returns the error.
pub async fn reconnect_pat_account(
    pool: &SqlitePool,
    store: &Arc<dyn CredentialStore>,
    api_base: &str,
    account_id: Uuid,
) -> Result<Account, RepoNyxError> {
    let account = get_account(pool, account_id)
        .await?
        .ok_or_else(|| RepoNyxError::new(ErrorCode::InvalidInput, "account not found"))?;
    let pat = stored_pat(store, account_id)?;
    match validate_pat(api_base, &pat).await {
        Ok(_) => match account.status {
            AccountStatus::TokenExpired | AccountStatus::Revoked | AccountStatus::Error => {
                reconnect_account(pool, account.id).await
            }
            _ => mark_validated(pool, account.id).await,
        },
        Err(e) if e.code == ErrorCode::AuthenticationExpired => {
            if account.status != AccountStatus::TokenExpired {
                let _ = set_account_status(pool, account.id, AccountStatus::TokenExpired).await;
            }
            Err(e)
        }
        Err(e) => Err(e),
    }
}

/// Validate a PAT account's stored credential and refresh
/// `last_validated_at` on success. A 401 marks the account
/// `token_expired`. Single attempt — never retried.
pub async fn validate_pat_account(
    pool: &SqlitePool,
    store: &Arc<dyn CredentialStore>,
    api_base: &str,
    account_id: Uuid,
) -> Result<reponyx_provider_abstractions::ProviderAccountInfo, RepoNyxError> {
    let account = get_account(pool, account_id)
        .await?
        .ok_or_else(|| RepoNyxError::new(ErrorCode::InvalidInput, "account not found"))?;
    let pat = stored_pat(store, account_id)?;
    match validate_pat(api_base, &pat).await {
        Ok(info) => {
            mark_validated(pool, account.id).await?;
            Ok(info)
        }
        Err(e) if e.code == ErrorCode::AuthenticationExpired => {
            if account.status != AccountStatus::TokenExpired {
                let _ = set_account_status(pool, account.id, AccountStatus::TokenExpired).await;
            }
            Err(e)
        }
        Err(e) => Err(e),
    }
}

/// Read the stored PAT for an account. A missing entry means the
/// grant is gone: `AuthenticationExpired`, never a bare `None` that
/// callers could mistake for "no account".
fn stored_pat(
    store: &Arc<dyn CredentialStore>,
    account_id: Uuid,
) -> Result<SecretString, RepoNyxError> {
    match store.get(SERVICE_PAT, &account_id.to_string())? {
        Some(pat) => Ok(pat),
        None => Err(RepoNyxError::new(
            ErrorCode::AuthenticationExpired,
            "no PAT is stored for this account; add the PAT again",
        )),
    }
}

/// A [`CredentialSource`] that reads a PAT from the credential store
/// on every call — the Phase 4 provider's PAT source, implemented
/// for real in Phase 5. The GitHub login is supplied at
/// construction (it comes from `validate_auth` when the account is
/// added) and used as the Git HTTP username.
pub struct PatTokenSource {
    store: Arc<dyn CredentialStore>,
    account_id: Uuid,
    username: String,
    expires_at: Option<DateTime<Utc>>,
}

impl PatTokenSource {
    pub fn new(
        store: Arc<dyn CredentialStore>,
        account_id: Uuid,
        username: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            store,
            account_id,
            username: username.into(),
            expires_at,
        }
    }
}

impl std::fmt::Debug for PatTokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatTokenSource")
            .field("account_id", &self.account_id)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CredentialSource for PatTokenSource {
    async fn token(&self) -> Result<Token, RepoNyxError> {
        let pat = stored_pat(&self.store, self.account_id)?;
        let expires_in_secs = self
            .expires_at
            .map(|expires_at| (expires_at - Utc::now()).num_seconds().max(0) as u64);
        Ok(Token::new(
            pat.expose(),
            reponyx_github_provider::credentials::CredentialKind::Pat,
            self.username.clone(),
            expires_in_secs,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pat_format_accepts_both_github_shapes() {
        assert!(validate_pat_format(&SecretString::new("github_pat_test_abc")).is_ok());
        assert!(validate_pat_format(&SecretString::new("ghp_test_abc")).is_ok());
        assert!(validate_pat_format(&SecretString::new("")).is_err());
        assert!(validate_pat_format(&SecretString::new("not-a-token")).is_err());
        assert!(validate_pat_format(&SecretString::new("github_pat_with space")).is_err());
        assert!(validate_pat_format(&SecretString::new("username:password")).is_err());
    }

    #[test]
    fn expiry_status_thresholds() {
        let now = Utc::now();
        let mut account = Account {
            id: Uuid::new_v4(),
            provider: Provider::GitHub,
            provider_account_id: "42".to_string(),
            username: "octocat".to_string(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
            status: AccountStatus::Connected,
            last_validated_at: None,
            pat_expires_at: None,
            created_at: now,
            updated_at: now,
        };
        assert_eq!(pat_expiry_status(&account, now), PatExpiryStatus::Unknown);
        account.pat_expires_at = Some(now + chrono::Duration::days(30));
        assert_eq!(pat_expiry_status(&account, now), PatExpiryStatus::Valid);
        account.pat_expires_at = Some(now + chrono::Duration::days(3));
        assert_eq!(
            pat_expiry_status(&account, now),
            PatExpiryStatus::ExpiringSoon { days_left: 3 }
        );
        account.pat_expires_at = Some(now - chrono::Duration::days(1));
        assert_eq!(pat_expiry_status(&account, now), PatExpiryStatus::Expired);
    }
}
