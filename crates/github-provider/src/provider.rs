//! The GitHub provider: [`GitProvider`] over the GitHub REST API.
//!
//! Discovery uses `GET /user/repos` for user credentials (PATs)
//! and `GET /installation/repositories` for installation tokens —
//! each endpoint refuses the other credential class (`GET /user`
//! and `/user/repos` have no user behind an installation token),
//! so discovery follows the token kind. Pages are sorted by full
//! name so pagination order is deterministic. Installation
//! endpoints use only what a user/installation token can call:
//!
//! - `GET /user/installations` — installations visible to the token.
//! - `GET /user/installations/{id}/repositories` — repositories of
//!   one installation, for a user token.
//!
//! What needs Phase 5 (and is therefore NOT here):
//! - `GET /app/installations` and `POST
//!   /app/installations/{id}/access_tokens` need an App JWT, which
//!   only the Auth Broker may sign (the private key must never be in
//!   this crate). Phase 5 supplies a broker-backed
//!   [`CredentialSource`] that hands out installation tokens; this
//!   provider then works unchanged.

use crate::client::GitHubClient;
use crate::credentials::{CredentialKind, CredentialSource};
use crate::model::{
    ApiInstallation, ApiPermissions, ApiRepository, ApiUser, GitHubInstallation, PermissionLevel,
    RepositoryAccess, map_account, map_installation, map_permissions, map_repository,
};
use crate::pagination::resolve_cursor;
use crate::rate_limit::RateLimitState;
use async_trait::async_trait;
use reponyx_core::{ErrorCode, Provider, RepoNyxError};
use reponyx_provider_abstractions::{
    DiscoveredRepository, GitProvider, ProviderAccountInfo, ProviderCapabilities, RepositoryPage,
    ShortLivedCredential,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Repositories per discovery page. GitHub's maximum is 100.
const PER_PAGE: u32 = 100;

/// The GitHub provider. Holds only a [`GitHubClient`]; credentials
/// come from the client's token source, one token per request.
pub struct GitHubProvider {
    client: Arc<GitHubClient>,
}

impl GitHubProvider {
    /// Build a provider for `base_url` (default
    /// `https://api.github.com`; tests point it at a mock server)
    /// with the given token source.
    pub fn new(base_url: &str, source: Arc<dyn CredentialSource>) -> Result<Self, RepoNyxError> {
        Ok(Self {
            client: Arc::new(GitHubClient::new(base_url, source)?),
        })
    }

    /// Build a provider around an already-configured client (tests
    /// use this to tune retry / rate-limit behaviour).
    pub fn with_client(client: GitHubClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Snapshot of the latest observed rate-limit state.
    pub async fn rate_limit(&self) -> RateLimitState {
        self.client.rate_limit().await
    }

    /// Validate the credential and return account identity (`GET
    /// /user`). Only for user credentials (PATs, user access
    /// tokens) — an installation token has no user to return, so
    /// `GET /user` answers 403 whatever the App holds; use
    /// [`Self::validate_installation`] for those. 401 →
    /// `AuthenticationExpired`; a suspended/revoked grant →
    /// `AuthenticationRevoked`.
    pub async fn validate_auth(&self) -> Result<ProviderAccountInfo, RepoNyxError> {
        let response = self.client.get_json::<ApiUser>("/user").await?;
        Ok(map_account(response.body))
    }

    /// Validate an installation token and return the account it is
    /// installed on. `GET /user` refuses installation tokens — an
    /// installation belongs to an App on an account, not to a
    /// person, so there is no user to return and GitHub answers 403
    /// before permissions are consulted. This reads the owner of
    /// the first accessible repository instead
    /// (`GET /installation/repositories?per_page=1`), which also
    /// proves the token is alive.
    ///
    /// The account key is namespaced (`github-app:{installation_id}`):
    /// `accounts` has `UNIQUE (provider, provider_account_id)` and
    /// a PAT account for the same GitHub user already owns the bare
    /// numeric id, so the installation id is the stable per-account
    /// key (one App install per GitHub account).
    pub async fn validate_installation(
        &self,
        installation_id: &str,
    ) -> Result<ProviderAccountInfo, RepoNyxError> {
        let response = self
            .client
            .get_json::<InstallationRepositoriesResponse>("/installation/repositories?per_page=1")
            .await?;
        let Some(first) = response.body.repositories.into_iter().next() else {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "installation has no accessible repositories; grant the App access to at least one repository",
            ));
        };
        Ok(ProviderAccountInfo {
            provider_account_id: format!("github-app:{installation_id}"),
            username: first.owner.login,
            display_name: None,
            avatar_url: None,
        })
    }

    /// One page of discovery. `cursor` is the opaque `next_cursor`
    /// from the previous page (`None` for the first page); it is the
    /// next-page URL from the `Link` header, validated against the
    /// API base before use.
    ///
    /// The endpoint and the response shape follow the token kind:
    /// `/user/repos` (a JSON array) refuses installation tokens and
    /// `/installation/repositories` (an object with a `repositories`
    /// array) refuses PATs, so there is no endpoint both accept. A
    /// later page's URL comes from the previous page's `Link`
    /// header, so it already points at the right endpoint — only
    /// the parse stays kind-dependent.
    pub async fn discover_repositories(
        &self,
        cursor: Option<&str>,
    ) -> Result<RepositoryPage, RepoNyxError> {
        let installation = self.client.token().await?.kind == CredentialKind::InstallationToken;
        let url = match cursor {
            Some(cursor) => resolve_cursor(self.client.base_url(), cursor)?,
            None => {
                if installation {
                    format!(
                        "{}/installation/repositories?per_page={PER_PAGE}",
                        self.client.base_url()
                    )
                } else {
                    format!(
                        "{}/user/repos?per_page={PER_PAGE}&sort=full_name&direction=asc",
                        self.client.base_url()
                    )
                }
            }
        };
        let mut repositories = Vec::new();
        let next_link = if installation {
            let response = self
                .client
                .get_json::<InstallationRepositoriesResponse>(&url)
                .await?;
            for api in response.body.repositories {
                repositories.push(map_repository(api)?);
            }
            response.next_link
        } else {
            let response = self.client.get_json::<Vec<ApiRepository>>(&url).await?;
            for api in response.body {
                repositories.push(map_repository(api)?);
            }
            response.next_link
        };
        Ok(RepositoryPage {
            has_next_page: next_link.is_some(),
            next_cursor: next_link,
            repositories,
        })
    }

    /// Stream all discoverable repositories in API order, one message
    /// per API page.
    ///
    /// Batching contract: each `Ok` message is exactly one page (up
    /// to `PER_PAGE` repos, in the order the API returned them); the
    /// stream ends when the channel closes. An `Err` message is
    /// terminal — the stream stops after it, and the error is the
    /// first failure encountered (a later page is never silently
    /// skipped). Dropping the receiver stops pagination. The caller
    /// never holds all repositories in memory at once.
    pub fn discover_all_stream(
        &self,
    ) -> mpsc::Receiver<Result<Vec<DiscoveredRepository>, RepoNyxError>> {
        let (tx, rx) = mpsc::channel::<Result<Vec<DiscoveredRepository>, RepoNyxError>>(4);
        let client = Arc::clone(&self.client);
        tokio::spawn(async move {
            let mut cursor: Option<String> = None;
            loop {
                let page = {
                    let provider = GitHubProvider {
                        client: Arc::clone(&client),
                    };
                    provider.discover_repositories(cursor.as_deref()).await
                };
                match page {
                    Ok(page) => {
                        let has_next = page.has_next_page;
                        let next = page.next_cursor;
                        if tx.send(Ok(page.repositories)).await.is_err() {
                            return;
                        }
                        if !has_next {
                            return;
                        }
                        cursor = next;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                }
            }
        });
        rx
    }

    /// List the GitHub App installations visible to the token (`GET
    /// /user/installations`). `GET /app/installations` is App-JWT
    /// only and is deliberately not used here.
    pub async fn list_installations(&self) -> Result<Vec<GitHubInstallation>, RepoNyxError> {
        #[derive(serde::Deserialize)]
        struct InstallationsResponse {
            installations: Vec<ApiInstallation>,
        }
        let response = self
            .client
            .get_json::<InstallationsResponse>("/user/installations")
            .await?;
        Ok(response
            .body
            .installations
            .into_iter()
            .map(map_installation)
            .collect())
    }

    /// List the repositories of one installation (`GET
    /// /user/installations/{id}/repositories`), paginated the same
    /// way as discovery.
    pub async fn list_installation_repositories(
        &self,
        installation_id: &str,
    ) -> Result<Vec<DiscoveredRepository>, RepoNyxError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let url = match &cursor {
                Some(cursor) => resolve_cursor(self.client.base_url(), cursor)?,
                None => format!(
                    "{}/user/installations/{installation_id}/repositories?per_page={PER_PAGE}",
                    self.client.base_url()
                ),
            };
            let response = self
                .client
                .get_json::<InstallationRepositoriesResponse>(&url)
                .await?;
            for api in response.body.repositories {
                out.push(map_repository(api)?);
            }
            match response.next_link {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(out)
    }

    /// Check what the token may do on one repository (`GET
    /// /repos/{owner}/{name}`). 404 → `RepositoryNotFound`; 403 →
    /// `InsufficientPermissions` (or `AuthenticationRevoked` when the
    /// grant was suspended/revoked).
    pub async fn check_repository_access(
        &self,
        owner: &str,
        name: &str,
    ) -> Result<RepositoryAccess, RepoNyxError> {
        validate_owner_or_name(owner)?;
        validate_owner_or_name(name)?;
        let url = format!("{}/repos/{owner}/{name}", self.client.base_url());
        let response = self.client.get_json::<ApiRepository>(&url).await?;
        let api = response.body;
        let permissions = api.permissions.unwrap_or(ApiPermissions {
            admin: false,
            maintain: false,
            push: false,
            triage: false,
            pull: false,
        });
        let permission = map_permissions(permissions);
        Ok(RepositoryAccess {
            full_name: api.full_name,
            can_read_contents: matches!(
                permission,
                PermissionLevel::Admin | PermissionLevel::Write | PermissionLevel::Read
            ),
            permission,
            default_branch: api.default_branch,
            private: api.private,
        })
    }

    /// Resolve the credential-free HTTPS clone URL for a repository.
    /// Always `https://github.com/{owner}/{name}.git` (GitHub.com).
    /// GitHub Enterprise hosts are a future extension: discovery
    /// already returns the API's `clone_url` per repository, which is
    /// what sync uses.
    pub fn clone_url(&self, owner: &str, name: &str) -> Result<String, RepoNyxError> {
        validate_owner_or_name(owner)?;
        validate_owner_or_name(name)?;
        Ok(format!("https://github.com/{owner}/{name}.git"))
    }

    /// Acquire a fresh short-lived credential for Git operations.
    /// The token comes from the configured [`CredentialSource`]
    /// (Phase 5 supplies the broker/PAT sources); this method only
    /// shapes it into a [`ShortLivedCredential`].
    pub async fn acquire_credential(&self) -> Result<ShortLivedCredential, RepoNyxError> {
        let token = self.client.token().await?;
        Ok(ShortLivedCredential {
            username: token.username.clone(),
            token: token.expose_value().to_string(),
            expires_in_secs: token.expires_in_secs,
        })
    }
}

/// `GET /installation/repositories` returns an object, not the
/// bare array `GET /user/repos` returns. Only the `repositories`
/// array is modelled; `total_count` is ignored.
#[derive(serde::Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<ApiRepository>,
}

/// Validate one path component of a repository identity (`owner` or
/// `name`) for use in a URL. Rejects empty values, separators,
/// traversal (`.` / `..`), whitespace and control characters. The
/// provider never trusts a repo name as a path.
fn validate_owner_or_name(value: &str) -> Result<(), RepoNyxError> {
    if value.is_empty() || value == "." || value == ".." {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "repository owner/name must not be empty",
        ));
    }
    if value
        .chars()
        .any(|c| c == '/' || c == '\\' || c.is_whitespace() || c.is_control())
    {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "repository owner/name contains an illegal character",
        ));
    }
    Ok(())
}

#[async_trait]
impl GitProvider for GitHubProvider {
    fn provider(&self) -> Provider {
        Provider::GitHub
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_organizations: true,
            supports_archived_filter: true,
            supports_forks: true,
            supports_lfs: true,
        }
    }

    async fn validate_auth(&self) -> Result<ProviderAccountInfo, RepoNyxError> {
        self.validate_auth().await
    }

    async fn discover_repositories(
        &self,
        cursor: Option<&str>,
    ) -> Result<RepositoryPage, RepoNyxError> {
        self.discover_repositories(cursor).await
    }

    fn clone_url(&self, owner: &str, name: &str) -> Result<String, RepoNyxError> {
        self.clone_url(owner, name)
    }

    async fn acquire_credential(&self) -> Result<ShortLivedCredential, RepoNyxError> {
        self.acquire_credential().await
    }
}

impl std::fmt::Debug for GitHubProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubProvider")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_url_validates_components() {
        let provider = GitHubProvider::new(
            "https://api.github.com",
            crate::credentials::static_installation_token("test-token"),
        )
        .unwrap();
        assert_eq!(
            provider.clone_url("octocat", "reponyx").unwrap(),
            "https://github.com/octocat/reponyx.git"
        );
        assert!(provider.clone_url("", "reponyx").is_err());
        assert!(provider.clone_url("octocat", "..").is_err());
        assert!(provider.clone_url("octo/cat", "reponyx").is_err());
        assert!(provider.clone_url("octocat", "repo name").is_err());
    }

    #[test]
    fn capabilities_cover_organizations_and_lfs() {
        let provider = GitHubProvider::new(
            "https://api.github.com",
            crate::credentials::static_installation_token("test-token"),
        )
        .unwrap();
        let caps = provider.capabilities();
        assert!(caps.supports_organizations);
        assert!(caps.supports_archived_filter);
        assert!(caps.supports_forks);
        assert!(caps.supports_lfs);
        assert_eq!(provider.provider(), Provider::GitHub);
    }

    #[test]
    fn provider_debug_does_not_leak_the_token() {
        let provider = GitHubProvider::new(
            "https://api.github.com",
            crate::credentials::static_installation_token("test-token-super-secret"),
        )
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("test-token-super-secret"));
    }
}
