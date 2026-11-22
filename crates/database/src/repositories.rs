//! Repository registry: upsert by provider id, filters, counts,
//! rename / ownership-change handling with collision detection.
//!
//! Stable identity is `(account_id, provider_repository_id)` (V1
//! unique key). `full_name` is always derived as `owner/name` and is
//! unique per account (V2 unique index); a rename or upsert that would
//! collide returns `InvalidInput` and never overwrites the other row.

use crate::common::{
    col, col_bool, col_uuid, invalid, map_error, now_iso, opt_col, require_non_empty,
};
use reponyx_core::{ArchiveStatus, RepoNyxError, Repository, SyncMode, Visibility};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

/// Fields for creating a repository. `full_name` is derived from
/// `owner`/`name`, not supplied.
#[derive(Debug, Clone)]
pub struct NewRepository {
    pub account_id: Uuid,
    pub provider_repository_id: String,
    pub owner: String,
    pub name: String,
    pub visibility: Visibility,
    pub default_branch: Option<String>,
    pub clone_url: String,
    pub sync_enabled: bool,
    pub sync_mode: SyncMode,
    pub local_path: Option<String>,
    pub remote_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Upsert input: the same provider-owned fields as `NewRepository`.
/// On conflict only provider metadata is refreshed (owner, name,
/// full_name, visibility, default_branch, clone_url,
/// remote_updated_at); local state (`sync_enabled`, `sync_mode`,
/// `local_path`, `archive_status`) is never touched by an upsert.
pub type UpsertRepository = NewRepository;

fn row_to_repository(row: &SqliteRow) -> Result<Repository, sqlx::Error> {
    Ok(Repository {
        id: col_uuid(row, "id")?,
        account_id: col_uuid(row, "account_id")?,
        provider_repository_id: col(row, "provider_repository_id")?,
        owner: col(row, "owner")?,
        name: col(row, "name")?,
        full_name: col(row, "full_name")?,
        visibility: Visibility::from_str(&col::<String>(row, "visibility")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        default_branch: opt_col(row, "default_branch")?,
        clone_url: col(row, "clone_url")?,
        archive_status: ArchiveStatus::from_str(&col::<String>(row, "archive_status")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        sync_enabled: col_bool(row, "sync_enabled")?,
        sync_mode: SyncMode::from_str(&col::<String>(row, "sync_mode")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        local_path: opt_col(row, "local_path")?,
        remote_updated_at: opt_col(row, "remote_updated_at")?,
        last_synced_at: opt_col(row, "last_synced_at")?,
        created_at: col(row, "created_at")?,
        updated_at: col(row, "updated_at")?,
    })
}

fn full_name(owner: &str, name: &str) -> String {
    format!("{owner}/{name}")
}

fn validate_new(input: &NewRepository) -> Result<(), RepoNyxError> {
    require_non_empty("provider_repository_id", &input.provider_repository_id)?;
    require_non_empty("owner", &input.owner)?;
    require_non_empty("name", &input.name)?;
    require_non_empty("clone_url", &input.clone_url)?;
    Ok(())
}

pub async fn create_repository(
    pool: &SqlitePool,
    input: NewRepository,
) -> Result<Repository, RepoNyxError> {
    validate_new(&input)?;
    let id = Uuid::new_v4();
    let now = now_iso();
    let full_name = full_name(input.owner.trim(), input.name.trim());
    sqlx::query(
        r#"
        INSERT INTO repositories
            (id, account_id, provider_repository_id, owner, name, full_name,
             visibility, default_branch, clone_url, archive_status,
             sync_enabled, sync_mode, local_path, remote_updated_at,
             created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.account_id.to_string())
    .bind(input.provider_repository_id.trim())
    .bind(input.owner.trim())
    .bind(input.name.trim())
    .bind(full_name)
    .bind(input.visibility.as_str())
    .bind(input.default_branch)
    .bind(input.clone_url.trim())
    .bind(i64::from(input.sync_enabled))
    .bind(input.sync_mode.as_str())
    .bind(input.local_path)
    .bind(input.remote_updated_at)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_repository(pool, id)
        .await?
        .ok_or_else(|| invalid("repository just created but not found"))
}

pub async fn get_repository(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Option<Repository>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM repositories WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_repository(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn get_repository_by_provider_id(
    pool: &SqlitePool,
    account_id: Uuid,
    provider_repository_id: &str,
) -> Result<Option<Repository>, RepoNyxError> {
    let row = sqlx::query(
        "SELECT * FROM repositories WHERE account_id = ? AND provider_repository_id = ?",
    )
    .bind(account_id.to_string())
    .bind(provider_repository_id)
    .fetch_optional(pool)
    .await
    .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_repository(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

/// Insert or update by `(account_id, provider_repository_id)` in a
/// single atomic statement (`ON CONFLICT DO UPDATE`), so concurrent
/// upserts of the same repository cannot create duplicates. Returns
/// the row and whether it was newly created. A full_name collision
/// with a *different* repository returns `InvalidInput`.
pub async fn upsert_repository(
    pool: &SqlitePool,
    input: UpsertRepository,
) -> Result<(Repository, bool), RepoNyxError> {
    validate_new(&input)?;
    let existing =
        get_repository_by_provider_id(pool, input.account_id, input.provider_repository_id.trim())
            .await?;
    let id = Uuid::new_v4();
    let now = now_iso();
    let full_name = full_name(input.owner.trim(), input.name.trim());
    sqlx::query(
        r#"
        INSERT INTO repositories
            (id, account_id, provider_repository_id, owner, name, full_name,
             visibility, default_branch, clone_url, archive_status,
             sync_enabled, sync_mode, local_path, remote_updated_at,
             created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?)
        ON CONFLICT (account_id, provider_repository_id) DO UPDATE SET
            owner = excluded.owner,
            name = excluded.name,
            full_name = excluded.full_name,
            visibility = excluded.visibility,
            default_branch = excluded.default_branch,
            clone_url = excluded.clone_url,
            remote_updated_at = excluded.remote_updated_at,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(id.to_string())
    .bind(input.account_id.to_string())
    .bind(input.provider_repository_id.trim())
    .bind(input.owner.trim())
    .bind(input.name.trim())
    .bind(full_name)
    .bind(input.visibility.as_str())
    .bind(input.default_branch)
    .bind(input.clone_url.trim())
    .bind(i64::from(input.sync_enabled))
    .bind(input.sync_mode.as_str())
    .bind(input.local_path)
    .bind(input.remote_updated_at)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    let repo =
        get_repository_by_provider_id(pool, input.account_id, input.provider_repository_id.trim())
            .await?
            .ok_or_else(|| invalid("repository just upserted but not found"))?;
    Ok((repo, existing.is_none()))
}

/// Rename (or transfer ownership of) a repository, identified by its
/// provider id. Updates `owner`, `name`, `full_name`, and — when the
/// stored URL is a `github.com` HTTPS URL — `clone_url` to match.
/// A second repository in the same account must not end up with the
/// same `full_name`: that returns `InvalidInput` (enforced by the V2
/// unique index as well as the check below).
pub async fn rename_repository(
    pool: &SqlitePool,
    account_id: Uuid,
    provider_repository_id: &str,
    new_owner: &str,
    new_name: &str,
) -> Result<Repository, RepoNyxError> {
    require_non_empty("new owner", new_owner)?;
    require_non_empty("new name", new_name)?;
    let current = get_repository_by_provider_id(pool, account_id, provider_repository_id)
        .await?
        .ok_or_else(|| {
            invalid(format!(
                "repository {provider_repository_id} not found for account {account_id}"
            ))
        })?;
    let new_owner = new_owner.trim();
    let new_name = new_name.trim();
    let new_full_name = full_name(new_owner, new_name);
    if new_full_name != current.full_name {
        let clash = sqlx::query(
            "SELECT id FROM repositories WHERE account_id = ? AND full_name = ? AND id != ?",
        )
        .bind(account_id.to_string())
        .bind(&new_full_name)
        .bind(current.id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
        if clash.is_some() {
            return Err(invalid(format!(
                "account already has a repository named {new_full_name}"
            )));
        }
    }
    // Keep the clone URL in step with the rename for github.com URLs;
    // other hosts are left alone (the caller can update them).
    let new_clone_url = if current.clone_url.starts_with("https://github.com/") {
        format!("https://github.com/{new_full_name}.git")
    } else {
        current.clone_url.clone()
    };
    let now = now_iso();
    let affected = sqlx::query(
        r#"
        UPDATE repositories
        SET owner = ?, name = ?, full_name = ?, clone_url = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(new_owner)
    .bind(new_name)
    .bind(new_full_name)
    .bind(new_clone_url)
    .bind(now)
    .bind(current.id.to_string())
    .execute(pool)
    .await
    .map_err(map_error)?
    .rows_affected();
    if affected == 0 {
        return Err(invalid(format!("repository {} not found", current.id)));
    }
    get_repository(pool, current.id)
        .await?
        .ok_or_else(|| invalid("repository just renamed but not found"))
}

/// Filters for listing / counting repositories. All fields are
/// optional; `full_name_search` matches a substring (LIKE `%q%`).
/// `limit` / `offset` page the list: `limit` caps the rows at
/// 1..=500 (a safety net, not a page size — the Tauri command
/// clamps tighter), `offset` is only applied with a limit. `None`
/// for both means no paging, for internal callers that need every
/// row (the config export, the scheduler); the Tauri command
/// always pages.
#[derive(Debug, Clone, Default)]
pub struct RepositoryFilter {
    pub account_id: Option<Uuid>,
    pub archive_status: Option<ArchiveStatus>,
    pub sync_enabled: Option<bool>,
    pub full_name_search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn apply_filter(builder: &mut QueryBuilder<Sqlite>, filter: &RepositoryFilter) {
    let mut first = true;
    let mut push_condition = |builder: &mut QueryBuilder<Sqlite>| {
        if first {
            builder.push(" WHERE ");
            first = false;
        } else {
            builder.push(" AND ");
        }
    };
    if let Some(account_id) = filter.account_id {
        push_condition(builder);
        builder.push("account_id = ");
        builder.push_bind(account_id.to_string());
    }
    if let Some(status) = filter.archive_status {
        push_condition(builder);
        builder.push("archive_status = ");
        builder.push_bind(status.as_str());
    }
    if let Some(sync_enabled) = filter.sync_enabled {
        push_condition(builder);
        builder.push("sync_enabled = ");
        builder.push_bind(i64::from(sync_enabled));
    }
    if let Some(search) = &filter.full_name_search {
        push_condition(builder);
        builder.push("full_name LIKE ");
        builder.push_bind(format!("%{search}%"));
    }
}

pub async fn list_repositories(
    pool: &SqlitePool,
    filter: RepositoryFilter,
) -> Result<Vec<Repository>, RepoNyxError> {
    let mut builder = QueryBuilder::new("SELECT * FROM repositories");
    apply_filter(&mut builder, &filter);
    builder.push(" ORDER BY full_name ASC");
    if let Some(limit) = filter.limit {
        builder.push(" LIMIT ");
        builder.push_bind(limit.clamp(1, 500));
        builder.push(" OFFSET ");
        builder.push_bind(filter.offset.unwrap_or(0).max(0));
    }
    let rows = builder.build().fetch_all(pool).await.map_err(map_error)?;
    rows.iter()
        .map(row_to_repository)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

/// Full names for a bounded list of repository ids, for pages that
/// show rows from another table (jobs, history) and only need the
/// names of the repositories actually displayed. The id list is
/// capped at 500; an empty list returns an empty map without
/// touching the database. Ids with no row are simply absent.
pub async fn repository_names(
    pool: &SqlitePool,
    ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>, RepoNyxError> {
    let mut out = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let mut builder = QueryBuilder::new("SELECT id, full_name FROM repositories WHERE id IN (");
    {
        let mut separated = builder.separated(", ");
        for id in ids.iter().take(500) {
            separated.push_bind(id.to_string());
        }
        separated.push_unseparated(")");
    }
    let rows = builder.build().fetch_all(pool).await.map_err(map_error)?;
    for row in &rows {
        let id = col_uuid(row, "id").map_err(map_error)?;
        let name = col::<String>(row, "full_name").map_err(map_error)?;
        out.insert(id, name);
    }
    Ok(out)
}

pub async fn count_repositories(
    pool: &SqlitePool,
    filter: RepositoryFilter,
) -> Result<i64, RepoNyxError> {
    let mut builder = QueryBuilder::new("SELECT COUNT(*) AS n FROM repositories");
    apply_filter(&mut builder, &filter);
    let row = builder.build().fetch_one(pool).await.map_err(map_error)?;
    Ok(row.get("n"))
}

pub async fn set_archive_status(
    pool: &SqlitePool,
    id: Uuid,
    status: ArchiveStatus,
) -> Result<Repository, RepoNyxError> {
    let now = now_iso();
    sqlx::query("UPDATE repositories SET archive_status = ?, updated_at = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_repository(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("repository {id} not found")))
}

pub async fn set_sync_enabled(
    pool: &SqlitePool,
    id: Uuid,
    enabled: bool,
) -> Result<Repository, RepoNyxError> {
    let now = now_iso();
    sqlx::query("UPDATE repositories SET sync_enabled = ?, updated_at = ? WHERE id = ?")
        .bind(i64::from(enabled))
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_repository(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("repository {id} not found")))
}

pub async fn set_sync_mode(
    pool: &SqlitePool,
    id: Uuid,
    mode: SyncMode,
) -> Result<Repository, RepoNyxError> {
    let now = now_iso();
    sqlx::query("UPDATE repositories SET sync_mode = ?, updated_at = ? WHERE id = ?")
        .bind(mode.as_str())
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_repository(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("repository {id} not found")))
}

/// Record a successful sync: status `synced` + `last_synced_at`.
pub async fn mark_synced(
    pool: &SqlitePool,
    id: Uuid,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<Repository, RepoNyxError> {
    let now = now_iso();
    sqlx::query(
        "UPDATE repositories SET archive_status = 'synced', last_synced_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(at)
    .bind(now)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_repository(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("repository {id} not found")))
}

pub async fn delete_repository(pool: &SqlitePool, id: Uuid) -> Result<bool, RepoNyxError> {
    let result = sqlx::query("DELETE FROM repositories WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    Ok(result.rows_affected() > 0)
}

/// Persist (or clear, with `None`) a repository's local archive
/// path. This is the helper the sync-queue runner's local copy was
/// waiting for: clone jobs, the rename move, and archive deletion
/// all go through it instead of issuing their own SQL.
pub async fn set_local_path(
    pool: &SqlitePool,
    id: Uuid,
    path: Option<&Path>,
) -> Result<Repository, RepoNyxError> {
    let now = now_iso();
    let affected =
        sqlx::query("UPDATE repositories SET local_path = ?, updated_at = ? WHERE id = ?")
            .bind(path.map(|p| p.to_string_lossy().into_owned()))
            .bind(now)
            .bind(id.to_string())
            .execute(pool)
            .await
            .map_err(map_error)?
            .rows_affected();
    if affected == 0 {
        return Err(invalid(format!("repository {id} not found")));
    }
    get_repository(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("repository {id} not found")))
}
