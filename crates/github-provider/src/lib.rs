//! GitHub provider (spec §7, Phase 4): the [`GitProvider`]
//! implementation for GitHub.
//!
//! The provider talks to the GitHub REST API only. It never runs Git
//! (that is `reponyx-git-engine`'s job, and the engine stays
//! provider-independent) and it never owns real credentials: tokens
//! come from a pluggable [`CredentialSource`], which Phase 5
//! (Authentication) fills with the Auth Broker and OS credential
//! store sources. Tests use [`StaticTokenSource`] with fake tokens.
//!
//! Safety rules:
//! - Tokens live only in `Authorization` headers, never in URLs,
//!   logs, error messages, or `Debug` output.
//! - Discovery paginates with `Link: rel="next"` until exhausted;
//!   the first page is never assumed complete.
//! - Rate limits back off (capped) and resume from the same cursor.
//! - Discovery writes to the database only via the Phase 2
//!   repositories, preserving local state, and never deletes a local
//!   repository that discovery did not return.

pub mod client;
pub mod credentials;
pub mod model;
pub mod pagination;
pub mod provider;
pub mod rate_limit;
pub mod sync;

pub use client::{ApiResponse, GitHubClient};
pub use credentials::{
    CredentialKind, CredentialSource, FailingTokenSource, StaticTokenSource, Token,
    static_installation_token, static_pat,
};
pub use model::{GitHubInstallation, PermissionLevel, RepositoryAccess};
pub use provider::GitHubProvider;
pub use rate_limit::RateLimitState;
pub use sync::{
    SyncDiscoveryOutcome, discover_and_sync, sync_discovered_repositories, sync_installations,
};
