//! Provider abstraction (spec §7).
//!
//! The sync engine never talks to GitHub directly. It talks to a
//! [`GitProvider`]. GitHub (App + PAT) is the first implementation
//! (Phase 4); GitLab / Bitbucket / self-hosted Git follow the same trait.
//!
//! A provider supplies repository metadata and short-lived credentials.
//! It never supplies a clone URL containing a token, and it never decides
//! how a repository is stored locally — that is the sync engine's job.

use async_trait::async_trait;
use reponyx_core::{Provider, RepoNyxError};
use serde::{Deserialize, Serialize};

/// A short-lived credential for one Git operation. Issued fresh per
/// operation (or per batch); never persisted, never logged, never placed
/// in a stored clone URL.
#[derive(Debug, Clone)]
pub struct ShortLivedCredential {
    /// Username for HTTP Basic auth (`x-access-token` for GitHub App
    /// installation tokens; the GitHub username for PATs).
    pub username: String,
    /// The token itself. Handle with care; drop after use.
    pub token: String,
    /// Seconds until expiry, if known (GitHub installation tokens: ~3600).
    pub expires_in_secs: Option<u64>,
}

/// Metadata for one discoverable repository, as returned by the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredRepository {
    /// Provider-stable id (GitHub `id`).
    pub provider_repository_id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub visibility: reponyx_core::Visibility,
    pub default_branch: Option<String>,
    /// HTTPS clone URL **without** credentials.
    pub clone_url: String,
    pub archived: bool,
    pub fork: bool,
    pub size_kb: Option<i64>,
    pub remote_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One page of discovery results. Callers must paginate until
/// `has_next_page` is false — never assume the first page is complete.
#[derive(Debug, Clone)]
pub struct RepositoryPage {
    pub repositories: Vec<DiscoveredRepository>,
    pub has_next_page: bool,
    /// Opaque cursor for the next page, if any.
    pub next_cursor: Option<String>,
}

/// What a provider can do. Discovery is capability-checked so the UI can
/// hide unsupported features instead of failing at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supports_organizations: bool,
    pub supports_archived_filter: bool,
    pub supports_forks: bool,
    pub supports_lfs: bool,
}

/// Account identity as the provider reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAccountInfo {
    pub provider_account_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// The contract every Git provider implements.
#[async_trait]
pub trait GitProvider: Send + Sync {
    /// Which provider this is (GitHub, GitLab, …).
    fn provider(&self) -> Provider;

    /// What this provider supports.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Validate the stored credential and return account identity.
    /// Returns `AuthenticationExpired` / `AuthenticationRevoked` on
    /// credential failure — never a generic error.
    async fn validate_auth(&self) -> Result<ProviderAccountInfo, RepoNyxError>;

    /// Discover accessible repositories, one page at a time.
    /// `cursor` is the opaque `next_cursor` from the previous page
    /// (`None` for the first page). Implementations must handle
    /// pagination and rate limits internally.
    async fn discover_repositories(
        &self,
        cursor: Option<&str>,
    ) -> Result<RepositoryPage, RepoNyxError>;

    /// Resolve the credential-free HTTPS clone URL for a repository.
    fn clone_url(&self, owner: &str, name: &str) -> Result<String, RepoNyxError> {
        let _ = (owner, name);
        Err(RepoNyxError::new(
            reponyx_core::ErrorCode::InternalError,
            "clone_url not implemented by this provider",
        ))
    }

    /// Acquire a fresh short-lived credential for Git operations.
    /// Callers must use it promptly and discard it afterwards.
    async fn acquire_credential(&self) -> Result<ShortLivedCredential, RepoNyxError>;
}
