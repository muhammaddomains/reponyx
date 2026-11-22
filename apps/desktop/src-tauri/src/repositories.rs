//! Tauri commands for the Phase 8 Repositories page.
//!
//! These commands are thin delegates: the rows live in
//! `reponyx-database`, discovery lives in `reponyx-github-provider`
//! (with credentials from the same resolver the background agent
//! uses), and running jobs is the agent's job — "sync now" here only
//! enqueues through the agent IPC (`agent.rs`), it never runs Git in
//! the desktop process.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works.

use crate::auth::{AuthState, CommandError, CommandResult, invalid_input, require_db};
use reponyx_core::{ArchiveStatus, Repository, SyncMode};
use reponyx_database::Database;
use reponyx_database::accounts::get_account;
use reponyx_database::repositories::{
    RepositoryFilter, delete_repository, get_repository, list_repositories, set_sync_enabled,
    set_sync_mode,
};
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::sync::discover_and_sync;
use reponyx_sync_queue::{CredentialResolver, default_resolver_from_settings};
use serde::Serialize;
use std::process::Command;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDto {
    pub id: String,
    pub account_id: String,
    pub provider_repository_id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub visibility: String,
    pub default_branch: Option<String>,
    pub clone_url: String,
    pub archive_status: String,
    pub sync_enabled: bool,
    pub sync_mode: String,
    pub local_path: Option<String>,
    pub remote_updated_at: Option<String>,
    pub last_synced_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl RepositoryDto {
    fn from_repository(repository: &Repository) -> Self {
        Self {
            id: repository.id.to_string(),
            account_id: repository.account_id.to_string(),
            provider_repository_id: repository.provider_repository_id.clone(),
            owner: repository.owner.clone(),
            name: repository.name.clone(),
            full_name: repository.full_name.clone(),
            visibility: repository.visibility.as_str().to_string(),
            default_branch: repository.default_branch.clone(),
            clone_url: repository.clone_url.clone(),
            archive_status: repository.archive_status.as_str().to_string(),
            sync_enabled: repository.sync_enabled,
            sync_mode: repository.sync_mode.as_str().to_string(),
            local_path: repository.local_path.clone(),
            remote_updated_at: repository.remote_updated_at.map(|at| at.to_rfc3339()),
            last_synced_at: repository.last_synced_at.map(|at| at.to_rfc3339()),
            created_at: repository.created_at.to_rfc3339(),
            updated_at: repository.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryOutcomeDto {
    pub created: usize,
    pub updated: usize,
    pub not_seen: Vec<String>,
}

// -- Small parsers ----------------------------------------------------

fn parse_uuid(id: &str, what: &str) -> CommandResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| invalid_input(format!("invalid {what} id: {id}")))
}

fn parse_archive_status(value: &str) -> CommandResult<ArchiveStatus> {
    match value {
        "synced" => Ok(ArchiveStatus::Synced),
        "pending" => Ok(ArchiveStatus::Pending),
        "syncing" => Ok(ArchiveStatus::Syncing),
        "failed" => Ok(ArchiveStatus::Failed),
        "paused" => Ok(ArchiveStatus::Paused),
        "disconnected" => Ok(ArchiveStatus::Disconnected),
        "remote_deleted" => Ok(ArchiveStatus::RemoteDeleted),
        _ => Err(invalid_input(format!("unknown archive status: {value}"))),
    }
}

fn parse_sync_mode(value: &str) -> CommandResult<SyncMode> {
    match value {
        "mirror" => Ok(SyncMode::Mirror),
        "working" => Ok(SyncMode::Working),
        "both" => Ok(SyncMode::Both),
        _ => Err(invalid_input(format!("unknown sync mode: {value}"))),
    }
}

/// Open `target` (a local path or an https URL) with the OS default
/// application. The target is passed as a single argument — never
/// through a shell — so it cannot inject a command. Callers only
/// ever pass values read from the database, never free user input.
pub(crate) fn open_with_os_app(target: &str) -> CommandResult<()> {
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opener = "xdg-open";
    Command::new(opener)
        .arg(target)
        .spawn()
        .map_err(|e| CommandError {
            code: "InternalError".into(),
            message: format!("could not open {target}: {e}"),
        })?;
    Ok(())
}

fn github_path_segment(value: &str) -> CommandResult<()> {
    let ok = !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if ok {
        Ok(())
    } else {
        Err(invalid_input(format!(
            "unsafe repository path segment: {value}"
        )))
    }
}

// -- Commands ---------------------------------------------------------

#[tauri::command]
pub async fn list_repositories_command(
    db: State<'_, Option<Database>>,
    account_id: Option<String>,
    archive_status: Option<String>,
    sync_enabled: Option<bool>,
    search: Option<String>,
) -> CommandResult<Vec<RepositoryDto>> {
    let database = require_db(&db)?;
    let account_id = account_id
        .map(|id| parse_uuid(&id, "account"))
        .transpose()?;
    let archive_status = archive_status
        .map(|value| parse_archive_status(&value))
        .transpose()?;
    let search = search.filter(|value| !value.trim().is_empty());
    let repositories = list_repositories(
        &database.pool,
        RepositoryFilter {
            account_id,
            archive_status,
            sync_enabled,
            full_name_search: search,
        },
    )
    .await?;
    Ok(repositories
        .iter()
        .map(RepositoryDto::from_repository)
        .collect())
}

/// Discover an account's repositories on GitHub and upsert them into
/// the database. New rows start sync-enabled in mirror mode (the
/// product defaults); existing rows keep their local state. Rows
/// discovery did not return are reported as `not_seen` — never
/// deleted.
#[tauri::command]
pub async fn discover_repositories_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
) -> CommandResult<DiscoveryOutcomeDto> {
    let database = require_db(&db)?;
    let id = parse_uuid(&account_id, "account")?;
    let account = get_account(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input("account not found"))?;
    let resolver = default_resolver_from_settings(
        &database.pool,
        Arc::clone(&auth.store),
        &auth.broker_base_url,
    )
    .await?;
    let source = resolver.resolve(&database.pool, &account).await?;
    let provider = GitHubProvider::new(&auth.github_api_base, source)?;
    let stream = provider.discover_all_stream();
    let outcome = discover_and_sync(&database.pool, id, stream).await?;
    Ok(DiscoveryOutcomeDto {
        created: outcome.created,
        updated: outcome.updated,
        not_seen: outcome.not_seen.iter().map(|id| id.to_string()).collect(),
    })
}

#[tauri::command]
pub async fn update_repository_command(
    db: State<'_, Option<Database>>,
    id: String,
    sync_enabled: Option<bool>,
    sync_mode: Option<String>,
) -> CommandResult<RepositoryDto> {
    let database = require_db(&db)?;
    let id = parse_uuid(&id, "repository")?;
    let mut repository = get_repository(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input("repository not found"))?;
    if let Some(enabled) = sync_enabled {
        repository = set_sync_enabled(&database.pool, id, enabled).await?;
    }
    if let Some(mode) = sync_mode {
        repository = set_sync_mode(&database.pool, id, parse_sync_mode(&mode)?).await?;
    }
    Ok(RepositoryDto::from_repository(&repository))
}

/// Remove a repository row from RepoNyx. This deletes the row only —
/// the local mirror/working clone on disk is kept, per the
/// never-auto-delete rule; removing the files is a separate, explicit
/// user action in the file manager.
#[tauri::command]
pub async fn delete_repository_command(
    db: State<'_, Option<Database>>,
    id: String,
) -> CommandResult<bool> {
    let database = require_db(&db)?;
    let id = parse_uuid(&id, "repository")?;
    Ok(delete_repository(&database.pool, id).await?)
}

#[tauri::command]
pub async fn open_repository_location_command(
    db: State<'_, Option<Database>>,
    id: String,
) -> CommandResult<()> {
    let database = require_db(&db)?;
    let id = parse_uuid(&id, "repository")?;
    let repository = get_repository(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input("repository not found"))?;
    let path = repository
        .local_path
        .ok_or_else(|| invalid_input("repository has no local copy yet — sync it first"))?;
    open_with_os_app(&path)
}

#[tauri::command]
pub async fn open_repository_in_browser_command(
    db: State<'_, Option<Database>>,
    id: String,
) -> CommandResult<()> {
    let database = require_db(&db)?;
    let id = parse_uuid(&id, "repository")?;
    let repository = get_repository(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input("repository not found"))?;
    github_path_segment(&repository.owner)?;
    github_path_segment(&repository.name)?;
    let url = format!(
        "https://github.com/{}/{}",
        repository.owner, repository.name
    );
    open_with_os_app(&url)
}
