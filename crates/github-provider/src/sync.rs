//! Writing discovery results into the Phase 2 repositories.
//!
//! Discovery is read-only against the API; this module is the only
//! place that turns discovered metadata into database rows. Two rules
//! hold throughout:
//!
//! - Upserts preserve local state. [`upsert_repository`] only
//!   refreshes provider metadata; `sync_enabled`, `sync_mode`,
//!   `local_path` and `archive_status` are never touched by discovery.
//! - Nothing discovered-absent is deleted. Repositories that exist
//!   locally but were not seen in discovery are returned in
//!   [`SyncDiscoveryOutcome::not_seen`] so a later phase can mark them
//!   disconnected; this module never deletes a local repository.

use crate::model::GitHubInstallation;
use reponyx_core::{Installation, InstallationStatus, RepoNyxError, SyncMode};
use reponyx_database::installations::{NewInstallation, upsert_installation};
use reponyx_database::repositories::{
    RepositoryFilter, UpsertRepository, list_repositories, upsert_repository,
};
use reponyx_provider_abstractions::DiscoveredRepository;
use sqlx::SqlitePool;
use std::collections::HashSet;
use tokio::sync::mpsc;
use uuid::Uuid;

/// The outcome of syncing one discovery run into the database.
#[derive(Debug, Clone, Default)]
pub struct SyncDiscoveryOutcome {
    /// Repositories newly inserted.
    pub created: usize,
    /// Repositories already present, refreshed from the API.
    pub updated: usize,
    /// Local repositories of this account that discovery did not
    /// return. They were NOT deleted or modified — the caller decides
    /// what to do with them (a later phase marks them disconnected).
    pub not_seen: Vec<Uuid>,
}

/// Consume a discovery stream (see `GitHubProvider::discover_all_stream`)
/// and upsert every repository into the `repositories` table for
/// `account_id`, page by page. New rows get `sync_enabled = true` and
/// `sync_mode = mirror` (the product defaults); existing rows keep
/// their local state.
///
/// The stream is consumed until it closes; a stream error aborts the
/// sync and is returned (rows already upserted stay — upserts are
/// idempotent, so a later run repairs the partial sync).
pub async fn sync_discovered_repositories(
    pool: &SqlitePool,
    account_id: Uuid,
    mut stream: mpsc::Receiver<Result<Vec<DiscoveredRepository>, RepoNyxError>>,
) -> Result<SyncDiscoveryOutcome, RepoNyxError> {
    let mut outcome = SyncDiscoveryOutcome::default();
    let mut seen: HashSet<String> = HashSet::new();
    while let Some(page) = stream.recv().await {
        let page = page?;
        for discovered in page {
            seen.insert(discovered.provider_repository_id.clone());
            let (_, created) = upsert_repository(pool, to_upsert(account_id, &discovered)).await?;
            if created {
                outcome.created += 1;
            } else {
                outcome.updated += 1;
            }
        }
    }
    let local = list_repositories(
        pool,
        RepositoryFilter {
            account_id: Some(account_id),
            ..RepositoryFilter::default()
        },
    )
    .await?;
    outcome.not_seen = local
        .into_iter()
        .filter(|repo| !seen.contains(&repo.provider_repository_id))
        .map(|repo| repo.id)
        .collect();
    Ok(outcome)
}

fn to_upsert(account_id: Uuid, discovered: &DiscoveredRepository) -> UpsertRepository {
    UpsertRepository {
        account_id,
        provider_repository_id: discovered.provider_repository_id.clone(),
        owner: discovered.owner.clone(),
        name: discovered.name.clone(),
        visibility: discovered.visibility,
        default_branch: discovered.default_branch.clone(),
        clone_url: discovered.clone_url.clone(),
        // Defaults for newly inserted rows only; the upsert leaves
        // these columns alone on existing rows.
        sync_enabled: true,
        sync_mode: SyncMode::Mirror,
        local_path: None,
        remote_updated_at: discovered.remote_updated_at,
    }
}

/// Persist the installations returned by `list_installations` for
/// `account_id`, creating or refreshing each one. Suspended
/// installations are stored as `Suspended`, the rest as `Active`.
/// Installations the API no longer returns are left alone (the API
/// never lists deleted installations, so absence is not proof of
/// deletion — a later phase reconciles them).
pub async fn sync_installations(
    pool: &SqlitePool,
    account_id: Uuid,
    installations: &[GitHubInstallation],
) -> Result<Vec<Installation>, RepoNyxError> {
    let mut out = Vec::with_capacity(installations.len());
    for installation in installations {
        let permissions_json =
            serde_json::to_string(&installation.permissions).unwrap_or_else(|_| "{}".to_string());
        let status = if installation.suspended {
            InstallationStatus::Suspended
        } else {
            InstallationStatus::Active
        };
        let stored = upsert_installation(
            pool,
            NewInstallation {
                account_id,
                installation_id: installation.installation_id.clone(),
                installation_type: installation.installation_type,
                permissions_json,
            },
            status,
        )
        .await?;
        out.push(stored);
    }
    Ok(out)
}

/// Convenience: discover everything for one account and write it to
/// the database in one call. Kept thin on purpose — the streaming
/// work lives in `sync_discovered_repositories`.
pub async fn discover_and_sync(
    pool: &SqlitePool,
    account_id: Uuid,
    stream: mpsc::Receiver<Result<Vec<DiscoveredRepository>, RepoNyxError>>,
) -> Result<SyncDiscoveryOutcome, RepoNyxError> {
    sync_discovered_repositories(pool, account_id, stream).await
}
