//! Tauri commands for the Storage page (Phase 8 page; Phase 9
//! adds management on top).
//!
//! The overview shows where the archives live, how much space they
//! take, and how full the volume is — plus, since Phase 9, what is
//! wrong with the storage: orphan archives (directories on disk
//! with no repository row), rows whose archive is missing from
//! disk, and rows whose archive directory still carries a pre-rename
//! name. Sizes are computed on demand (a walk of every stored
//! repository), so a first load can take a moment on a large
//! archive; the walk runs on a blocking thread, off the async
//! runtime.
//!
//! Destructive actions are explicit and re-verified server-side:
//! the UI confirms first, and every delete proves its target
//! again at delete time (under the storage root, really an
//! archive, and — for orphans — matching no repository row), so
//! the page never deletes on the UI's word alone. Deleting a
//! stored repository's archive keeps the row (its status returns
//! to Pending, so the next sync re-clones it); deleting an orphan
//! removes only the directory. Moving a renamed archive renames
//! each side on disk and updates `local_path` only after every
//! move succeeded, rolling back on failure.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works, and the CI windows-build job compiles this crate.

use crate::auth::{CommandError, CommandResult, require_db};
use crate::repositories::open_with_os_app;
use reponyx_core::{ArchiveStatus, Repository, SyncMode};
use reponyx_database::Database;
use reponyx_database::accounts::get_account;
use reponyx_database::repositories::{
    RepositoryFilter, get_repository, list_repositories, set_archive_status, set_local_path,
};
use reponyx_database::settings::get_setting;
use reponyx_git_engine::owner_segment;
use reponyx_git_engine::storage::{
    ArchiveKind, delete_archive_dir, dir_size, disk_usage, scan_archive_dirs,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRepositoryDto {
    pub repository_id: String,
    pub account_id: String,
    pub full_name: String,
    pub local_path: String,
    pub bytes: u64,
    pub last_synced_at: Option<String>,
}

/// An archive directory on disk with no repository row.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanDto {
    pub path: String,
    pub kind: String,
    pub bytes: u64,
    pub account: String,
    pub owner: String,
    pub name: String,
}

/// A repository row whose archive is gone from disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingDto {
    pub repository_id: String,
    pub full_name: String,
    pub local_path: String,
}

/// A repository row whose archive directory still carries a
/// pre-rename name (owner and/or name changed on GitHub after
/// the archive was created).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamedDto {
    pub repository_id: String,
    pub full_name: String,
    pub current_path: String,
    pub expected_path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageOverviewDto {
    pub root: String,
    pub root_configured: bool,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub mirrors_bytes: u64,
    pub database_bytes: u64,
    pub logs_bytes: u64,
    pub stored_repository_count: i64,
    pub repositories: Vec<StoredRepositoryDto>,
    pub orphans: Vec<OrphanDto>,
    pub orphans_bytes: u64,
    pub missing: Vec<MissingDto>,
    pub renamed: Vec<RenamedDto>,
}

/// One side moved by the rename command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveEntry {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveArchiveResult {
    pub moved: Vec<MoveEntry>,
    pub new_local_path: Option<String>,
}

/// Effective storage root: the `storage.root` setting when it is set
/// and non-empty, otherwise the product data directory (the same
/// default the background agent seeds on start). The returned
/// path is normalized (audit ST-m): the raw setting value could
/// carry `.`/`..` segments or a symlinked spelling, and every
/// other path in the app is canonical — comparing the two spellings
/// then disagreed about whether an archive was under the root.
pub(crate) async fn effective_storage_root(
    database: &Database,
) -> Result<(PathBuf, bool), CommandError> {
    let setting = get_setting(&database.pool, "storage.root").await?;
    match setting {
        Some(setting) if !setting.value.trim().is_empty() => {
            Ok((canonical_or_raw(&PathBuf::from(setting.value)), true))
        }
        _ => Ok((canonical_or_raw(&crate::dirs_data_dir()), false)),
    }
}

/// Canonicalize a path for comparison; the raw path when it does
/// not exist (a missing archive has nothing to canonicalize).
pub(crate) fn canonical_or_raw(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Parse an archive path into (account, owner, name, kind),
/// anchored at the storage root:
/// `<root>/repositories/github/<account>/<owner>/<name>.git` or
/// `<root>/working-clones/<account>/<owner>/<name>`, or the
/// collapsed form `<account>/<name>` when the account and the
/// owner are the same user (then owner == account). `None` for
/// paths outside the root or outside the layout.
///
/// Audit ST-m: the layout markers used to be searched anywhere in
/// the path, so a repository literally named "working-clones" (or
/// a `repositories/github` segment inside a repo name) parsed as
/// an archive under the wrong root. The path must sit under the
/// root and the layout must start at the root.
fn parse_archive_path(root: &Path, path: &Path) -> Option<(String, String, String, ArchiveKind)> {
    let root = canonical_or_raw(root);
    let path = canonical_or_raw(path);
    let rel = path.strip_prefix(&root).ok()?;
    let parts: Vec<&str> = rel
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();
    match parts.as_slice() {
        ["repositories", "github", account, name] if name.ends_with(".git") => Some((
            account.to_string(),
            account.to_string(),
            name.strip_suffix(".git").unwrap_or(name).to_string(),
            ArchiveKind::Mirror,
        )),
        ["repositories", "github", account, owner, name] if name.ends_with(".git") => Some((
            account.to_string(),
            owner.to_string(),
            name.strip_suffix(".git").unwrap_or(name).to_string(),
            ArchiveKind::Mirror,
        )),
        ["working-clones", account, name] => Some((
            account.to_string(),
            account.to_string(),
            name.to_string(),
            ArchiveKind::Working,
        )),
        ["working-clones", account, owner, name] => Some((
            account.to_string(),
            owner.to_string(),
            name.to_string(),
            ArchiveKind::Working,
        )),
        _ => None,
    }
}

/// Layout path for one side, from the row's current owner/name.
/// Collapses to `<account>/<name>` when the account and the owner
/// are the same user (same rule as `RepositoryLayout::new` in the
/// git engine, via `owner_segment`).
fn layout_path(
    root: &Path,
    account_username: &str,
    owner: &str,
    name: &str,
    kind: ArchiveKind,
) -> PathBuf {
    let base = match kind {
        ArchiveKind::Mirror => root.join("repositories").join("github"),
        ArchiveKind::Working => root.join("working-clones"),
    };
    let base = base.join(account_username);
    let base = match owner_segment(account_username, owner) {
        Some(owner) => base.join(owner),
        None => base,
    };
    match kind {
        ArchiveKind::Mirror => base.join(format!("{name}.git")),
        ArchiveKind::Working => base.join(name),
    }
}

/// The archive sides belonging to one repository, as (path, kind)
/// pairs for the sides that exist on disk. The row's `local_path`
/// is authoritative — after a rename it still points at the old
/// name — and the other side is derived by swapping the tree
/// segment. Without a `local_path`, the layout paths from the
/// row's current owner/name are used instead.
fn archive_sides_for(
    repo: &Repository,
    account_username: &str,
    root: &Path,
) -> Vec<(PathBuf, ArchiveKind)> {
    let mut sides: Vec<(PathBuf, ArchiveKind)> = Vec::new();
    if let Some(local) = &repo.local_path {
        let local = PathBuf::from(local);
        let kind = parse_archive_path(root, &local)
            .map(|(_, _, _, k)| k)
            .unwrap_or_else(|| {
                if local
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".git"))
                {
                    ArchiveKind::Mirror
                } else {
                    ArchiveKind::Working
                }
            });
        if local.exists() {
            sides.push((local.clone(), kind));
        }
        // The other side shares the old account/owner/name.
        if let Some((account, owner, name, _)) = parse_archive_path(root, &local) {
            let other = match kind {
                ArchiveKind::Mirror => {
                    layout_path(root, &account, &owner, &name, ArchiveKind::Working)
                }
                ArchiveKind::Working => {
                    layout_path(root, &account, &owner, &name, ArchiveKind::Mirror)
                }
            };
            if other != local && other.exists() {
                sides.push((
                    other,
                    match kind {
                        ArchiveKind::Mirror => ArchiveKind::Working,
                        ArchiveKind::Working => ArchiveKind::Mirror,
                    },
                ));
            }
        }
    } else {
        for kind in [ArchiveKind::Mirror, ArchiveKind::Working] {
            let path = layout_path(root, account_username, &repo.owner, &repo.name, kind);
            if path.exists() {
                sides.push((path, kind));
            }
        }
    }
    sides
}

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError {
        code: "InvalidInput".to_string(),
        message: message.into(),
    }
}

fn internal(message: impl Into<String>) -> CommandError {
    CommandError {
        code: "InternalError".to_string(),
        message: message.into(),
    }
}

async fn load_repo(database: &Database, repository_id: &str) -> CommandResult<Repository> {
    let id = Uuid::parse_str(repository_id).map_err(|_| invalid("repository id is not a UUID"))?;
    get_repository(&database.pool, id)
        .await?
        .ok_or_else(|| invalid("repository not found"))
}

// -- Commands ---------------------------------------------------------

#[tauri::command]
pub async fn get_storage_overview_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<StorageOverviewDto> {
    let database = require_db(&db)?;
    let (root, root_configured) = effective_storage_root(database).await?;
    let repos = list_repositories(&database.pool, RepositoryFilter::default()).await?;
    // (id, account_id, full_name, name, local_path, last_synced_at)
    let stored: Vec<(String, String, String, String, PathBuf, Option<String>)> = repos
        .iter()
        .filter_map(|repo| {
            repo.local_path.as_ref().map(|path| {
                (
                    repo.id.to_string(),
                    repo.account_id.to_string(),
                    repo.full_name.clone(),
                    repo.name.clone(),
                    PathBuf::from(path),
                    repo.last_synced_at.map(|at| at.to_rfc3339()),
                )
            })
        })
        .collect();
    // Canonicalized known paths, for orphan matching.
    let known: HashSet<PathBuf> = stored.iter().map(|row| canonical_or_raw(&row.4)).collect();

    let data_dir = crate::dirs_data_dir();
    let db_path = data_dir.join("data").join("reponyx.sqlite");
    let logs_dir = data_dir.join("logs");
    let paths: Vec<PathBuf> = stored.iter().map(|row| row.4.clone()).collect();
    let root_for_scan = root.clone();
    // Blocking filesystem walk — run off the async runtime.
    let scan = tokio::task::spawn_blocking(move || {
        let per_repo: Vec<u64> = paths.iter().map(|path| dir_size(path)).collect();
        let exists: Vec<bool> = paths.iter().map(|path| path.exists()).collect();
        let database_bytes = std::fs::metadata(&db_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        let logs_bytes = dir_size(&logs_dir);
        let disk = disk_usage(&root_for_scan).ok();
        let scanned = scan_archive_dirs(&root_for_scan);
        (per_repo, exists, database_bytes, logs_bytes, disk, scanned)
    })
    .await
    .map_err(|err| internal(format!("storage scan failed: {err}")))?;
    let (per_repo, exists, database_bytes, logs_bytes, disk, scanned) = scan;

    let mut repositories: Vec<StoredRepositoryDto> = stored
        .iter()
        .zip(per_repo.iter())
        .map(|(row, bytes)| StoredRepositoryDto {
            repository_id: row.0.clone(),
            account_id: row.1.clone(),
            full_name: row.2.clone(),
            local_path: row.4.to_string_lossy().into_owned(),
            bytes: *bytes,
            last_synced_at: row.5.clone(),
        })
        .collect();
    repositories.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.full_name.cmp(&b.full_name)));
    let mirrors_bytes = repositories.iter().map(|repo| repo.bytes).sum();

    let missing: Vec<MissingDto> = stored
        .iter()
        .zip(exists.iter())
        .filter(|(_, exists)| !**exists)
        .map(|(row, _)| MissingDto {
            repository_id: row.0.clone(),
            full_name: row.2.clone(),
            local_path: row.4.to_string_lossy().into_owned(),
        })
        .collect();

    // A stored archive whose directory basename no longer matches
    // the row's current name (renamed on GitHub after the clone).
    let renamed: Vec<RenamedDto> = stored
        .iter()
        .zip(exists.iter())
        .filter(|(_, exists)| **exists)
        .filter_map(|(row, _)| {
            let path = &row.4;
            let is_mirror = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".git"));
            let expected_name = if is_mirror {
                format!("{}.git", row.3)
            } else {
                row.3.clone()
            };
            let current_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if current_name == expected_name {
                return None;
            }
            Some(RenamedDto {
                repository_id: row.0.clone(),
                full_name: row.2.clone(),
                current_path: path.to_string_lossy().into_owned(),
                expected_path: path
                    .with_file_name(&expected_name)
                    .to_string_lossy()
                    .into_owned(),
                kind: if is_mirror { "mirror" } else { "working" }.to_string(),
            })
        })
        .collect();

    let orphans: Vec<OrphanDto> = scanned
        .iter()
        .filter(|dir| !known.contains(&canonical_or_raw(&dir.path)))
        .map(|dir| {
            let (account, owner, name) = parse_archive_path(&root, &dir.path)
                .map(|(a, o, n, _)| (a, o, n))
                .unwrap_or_default();
            OrphanDto {
                path: dir.path.to_string_lossy().into_owned(),
                kind: dir.kind.as_str().to_string(),
                bytes: dir.bytes,
                account,
                owner,
                name,
            }
        })
        .collect();
    let orphans_bytes = orphans.iter().map(|o| o.bytes).sum();

    Ok(StorageOverviewDto {
        root: root.to_string_lossy().into_owned(),
        root_configured,
        total_bytes: disk.map(|usage| usage.total_bytes),
        available_bytes: disk.map(|usage| usage.available_bytes),
        mirrors_bytes,
        database_bytes,
        logs_bytes,
        stored_repository_count: repositories.len() as i64,
        repositories,
        orphans,
        orphans_bytes,
        missing,
        renamed,
    })
}

/// Open the storage root in the OS file manager.
#[tauri::command]
pub async fn open_storage_root_command(db: State<'_, Option<Database>>) -> CommandResult<()> {
    let database = require_db(&db)?;
    let (root, _) = effective_storage_root(database).await?;
    open_with_os_app(&root.to_string_lossy())
}

/// Delete one orphan archive, returning the bytes freed. The path
/// is re-verified at delete time: it must scan as an archive under
/// the storage root right now, and match no repository row.
#[tauri::command]
pub async fn delete_orphan_command(
    db: State<'_, Option<Database>>,
    path: String,
) -> CommandResult<u64> {
    let database = require_db(&db)?;
    let (root, _) = effective_storage_root(database).await?;
    let repos = list_repositories(&database.pool, RepositoryFilter::default()).await?;
    let known: HashSet<PathBuf> = repos
        .iter()
        .filter_map(|repo| repo.local_path.as_ref())
        .map(|p| canonical_or_raw(Path::new(p)))
        .collect();
    let target = PathBuf::from(path);
    let root_for_delete = root.clone();
    tokio::task::spawn_blocking(move || -> CommandResult<u64> {
        let scanned = scan_archive_dirs(&root_for_delete);
        let canonical = canonical_or_raw(&target);
        let found = scanned
            .iter()
            .find(|dir| canonical_or_raw(&dir.path) == canonical)
            .ok_or_else(|| {
                invalid(format!(
                    "{} is not an archive under the storage root",
                    target.display()
                ))
            })?;
        if known.contains(&canonical) {
            return Err(invalid(
                "that archive belongs to a repository; delete it from the repository instead",
            ));
        }
        delete_archive_dir(&root_for_delete, &found.path)
            .map_err(|e| internal(format!("delete failed: {e}")))
    })
    .await
    .map_err(|err| internal(format!("storage delete failed: {err}")))?
}

/// Delete a stored repository's local archive(s), returning the
/// bytes freed. The row stays; its `local_path` is cleared and its
/// status returns to Pending, so the next sync re-clones it. Both
/// sides go for a Both repo.
#[tauri::command]
pub async fn delete_local_archive_command(
    db: State<'_, Option<Database>>,
    repository_id: String,
) -> CommandResult<u64> {
    let database = require_db(&db)?;
    let repo = load_repo(database, &repository_id).await?;
    let account = get_account(&database.pool, repo.account_id)
        .await?
        .ok_or_else(|| invalid("account for this repository not found"))?;
    let (root, _) = effective_storage_root(database).await?;
    let sides = archive_sides_for(&repo, &account.username, &root);
    if sides.is_empty() {
        return Err(invalid("that repository has no local archive to delete"));
    }
    let root_for_delete = root.clone();
    let freed = tokio::task::spawn_blocking(move || -> CommandResult<u64> {
        let mut freed = 0u64;
        for (path, _) in sides {
            freed += delete_archive_dir(&root_for_delete, &path)
                .map_err(|e| internal(format!("delete failed for {}: {e}", path.display())))?;
        }
        Ok(freed)
    })
    .await
    .map_err(|err| internal(format!("storage delete failed: {err}")))??;
    set_local_path(&database.pool, repo.id, None).await?;
    set_archive_status(&database.pool, repo.id, ArchiveStatus::Pending).await?;
    Ok(freed)
}

/// Move a renamed repository's archive(s) to match its current
/// name, returning what moved and the new `local_path`. Each side
/// is renamed on disk — never over an existing destination — and
/// a failure rolls back the sides already moved. `local_path` is
/// updated only after every move succeeded.
#[tauri::command]
pub async fn move_archive_for_rename_command(
    db: State<'_, Option<Database>>,
    repository_id: String,
) -> CommandResult<MoveArchiveResult> {
    let database = require_db(&db)?;
    let repo = load_repo(database, &repository_id).await?;
    let account = get_account(&database.pool, repo.account_id)
        .await?
        .ok_or_else(|| invalid("account for this repository not found"))?;
    let (root, _) = effective_storage_root(database).await?;
    let sides = archive_sides_for(&repo, &account.username, &root);
    if sides.is_empty() {
        return Err(invalid("that repository has no local archive to move"));
    }
    let expected = |kind: ArchiveKind| -> PathBuf {
        layout_path(&root, &account.username, &repo.owner, &repo.name, kind)
    };
    let moves: Vec<(PathBuf, PathBuf)> = sides
        .iter()
        .filter_map(|(path, kind)| {
            let dest = expected(*kind);
            if *path == dest {
                None
            } else {
                Some((path.clone(), dest))
            }
        })
        .collect();
    if moves.is_empty() {
        return Err(invalid("the archive already matches the repository name"));
    }
    for (_, dest) in &moves {
        if dest.exists() {
            return Err(invalid(format!(
                "{} already exists; move it out of the way first",
                dest.display()
            )));
        }
    }
    let (moved, new_local_path) = {
        let mirror_expected = expected(ArchiveKind::Mirror);
        let working_expected = expected(ArchiveKind::Working);
        let wants_mirror = matches!(repo.sync_mode, SyncMode::Mirror | SyncMode::Both);
        tokio::task::spawn_blocking(move || -> CommandResult<(Vec<MoveEntry>, Option<String>)> {
            let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
            for (from, to) in moves {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        internal(format!("cannot create {}: {e}", parent.display()))
                    })?;
                }
                if let Err(e) = std::fs::rename(&from, &to) {
                    for (moved_from, moved_to) in done.iter().rev() {
                        let _ = std::fs::rename(moved_to, moved_from);
                    }
                    return Err(internal(format!("move failed for {}: {e}", from.display())));
                }
                done.push((from, to));
            }
            // The new local_path: the mirror side when the mode wants
            // one and it exists there now, else the working side.
            let new_local = if wants_mirror && mirror_expected.exists() {
                Some(mirror_expected)
            } else if working_expected.exists() {
                Some(working_expected)
            } else {
                None
            };
            Ok((
                done.iter()
                    .map(|(from, to)| MoveEntry {
                        from: from.to_string_lossy().into_owned(),
                        to: to.to_string_lossy().into_owned(),
                    })
                    .collect(),
                new_local.map(|p| p.to_string_lossy().into_owned()),
            ))
        })
        .await
        .map_err(|err| internal(format!("storage move failed: {err}")))?
    }?;
    set_local_path(
        &database.pool,
        repo.id,
        new_local_path.as_deref().map(PathBuf::from).as_deref(),
    )
    .await?;
    Ok(MoveArchiveResult {
        moved,
        new_local_path,
    })
}
