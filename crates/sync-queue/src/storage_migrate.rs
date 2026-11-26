//! Move repository archives when `storage.root` changes (audit ST-M1).
//!
//! Changing the storage root must not orphan the archives that live
//! under the old root: for every repository, each existing side
//! (mirror / working clone) is renamed under the new root and
//! `local_path` is updated when it pointed at a moved side. Each
//! move runs under the cross-process repository lock for the old
//! path, so a running sync cannot touch the directory mid-move.
//! Per-repo failures are recorded in the report and the old
//! `local_path` is kept — a repo that fails to move is not lost,
//! and the next fetch re-clones it (ST-M2).

use std::path::Path;
use std::time::Duration;

use reponyx_database::accounts::get_account;
use reponyx_database::repositories::{RepositoryFilter, list_repositories, set_local_path};
use reponyx_git_engine::{RepoLockManager, RepositoryLayout};
use sqlx::SqlitePool;

/// Are these two paths the same location? Compares the raw paths
/// first, then their canonical forms (a symlinked or `.`/`..`
/// spelling of the same archive must compare equal), so a
/// differently-spelled `local_path` is still recognized after a
/// move. Audit ST-m: the relocation code compared `PathBuf`
/// equality and `local_path` strings, so a differently-spelled
/// `local_path` was never updated after its archive moved.
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The outcome of one `storage.root` migration.
#[derive(Debug, Default)]
pub struct StorageMigrationReport {
    /// Archive sides (mirror / working) successfully moved.
    pub moved: usize,
    /// Repositories with no archive under the old root.
    pub skipped: usize,
    /// (repository full name, reason) for every repo or side that
    /// could not be moved. Those archives stay where they are.
    pub failed: Vec<(String, String)>,
}

impl StorageMigrationReport {
    /// One line for the Settings page / log, e.g.
    /// "Moved 3 archive(s); 1 repo had no archive; 1 failed".
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("Moved {} archive(s)", self.moved)];
        if self.skipped > 0 {
            parts.push(format!("{} repo(s) had no archive", self.skipped));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} failed", self.failed.len()));
        }
        parts.join("; ")
    }
}

/// Move every archive under `old_root` to the matching path under
/// `new_root`, updating `local_path` for moved sides. Never deletes
/// anything: a side that cannot be moved stays where it is.
pub async fn migrate_storage_root(
    pool: &SqlitePool,
    old_root: &Path,
    new_root: &Path,
) -> StorageMigrationReport {
    let mut report = StorageMigrationReport::default();
    let repos = match list_repositories(pool, RepositoryFilter::default()).await {
        Ok(repos) => repos,
        Err(e) => {
            report.failed.push(("<all>".to_string(), e.message));
            return report;
        }
    };
    let locks = RepoLockManager::new();
    for repo in repos {
        let account = match get_account(pool, repo.account_id).await {
            Ok(Some(account)) => account,
            Ok(None) => {
                report.failed.push((
                    repo.full_name.clone(),
                    "account no longer exists".to_string(),
                ));
                continue;
            }
            Err(e) => {
                report.failed.push((repo.full_name.clone(), e.message));
                continue;
            }
        };
        let old_layout =
            match RepositoryLayout::new(old_root, &account.username, &repo.owner, &repo.name) {
                Ok(layout) => layout,
                Err(e) => {
                    report.failed.push((repo.full_name.clone(), e.message));
                    continue;
                }
            };
        let new_layout =
            match RepositoryLayout::new(new_root, &account.username, &repo.owner, &repo.name) {
                Ok(layout) => layout,
                Err(e) => {
                    report.failed.push((repo.full_name.clone(), e.message));
                    continue;
                }
            };
        let mut repo_moved = false;
        for (old_side, new_side) in [
            (&old_layout.mirror_path, &new_layout.mirror_path),
            (&old_layout.working_path, &new_layout.working_path),
        ] {
            if !old_side.is_dir() {
                continue;
            }
            // Hold the lock for the old path for the whole move,
            // so a running sync cannot fetch/clone into it.
            let _guard = match locks.acquire(old_side, Duration::from_secs(30)).await {
                Ok(guard) => guard,
                Err(e) => {
                    report.failed.push((repo.full_name.clone(), e.message));
                    continue;
                }
            };
            if new_side.exists() {
                report.failed.push((
                    repo.full_name.clone(),
                    format!("destination already exists: {}", new_side.display()),
                ));
                continue;
            }
            if let Some(parent) = new_side.parent()
                && let Err(e) = tokio::fs::create_dir_all(parent).await
            {
                report.failed.push((repo.full_name.clone(), e.to_string()));
                continue;
            }
            match tokio::fs::rename(old_side, new_side).await {
                Ok(()) => {
                    repo_moved = true;
                    report.moved += 1;
                    if repo
                        .local_path
                        .as_deref()
                        .is_some_and(|p| same_path(Path::new(p), old_side))
                        && let Err(e) = set_local_path(pool, repo.id, Some(new_side)).await
                    {
                        report.failed.push((repo.full_name.clone(), e.message));
                    }
                }
                Err(e) => {
                    report.failed.push((
                        repo.full_name.clone(),
                        format!("could not move {}: {e}", old_side.display()),
                    ));
                }
            }
        }
        if !repo_moved && !report.failed.iter().any(|(n, _)| n == &repo.full_name) {
            report.skipped += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use reponyx_core::{AccountType, AuthenticationMethod, Provider, SyncMode, Visibility};
    use reponyx_database::accounts::{NewAccount, create_account};
    use reponyx_database::repositories::{NewRepository, create_repository, get_repository};
    use reponyx_git_engine::RepositoryLayout;
    use std::path::PathBuf;
    use uuid::Uuid;

    async fn open_test_db() -> (tempfile::TempDir, reponyx_database::Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = reponyx_database::open_database(&dir.path().join("reponyx.sqlite"))
            .await
            .unwrap();
        (dir, db)
    }

    async fn seed_repo(pool: &SqlitePool, old_root: &Path) -> (Uuid, PathBuf, PathBuf) {
        let account = create_account(
            pool,
            NewAccount {
                provider: Provider::GitHub,
                provider_account_id: "prov-1".to_string(),
                username: "acme".to_string(),
                display_name: None,
                label: None,
                account_type: AccountType::Personal,
                authentication_method: AuthenticationMethod::FineGrainedPat,
            },
        )
        .await
        .unwrap();
        let repo = create_repository(
            pool,
            NewRepository {
                account_id: account.id,
                provider_repository_id: "repo-1".to_string(),
                owner: "acme".to_string(),
                name: "widgets".to_string(),
                visibility: Visibility::Public,
                default_branch: Some("main".to_string()),
                clone_url: "https://github.com/acme/widgets.git".to_string(),
                sync_enabled: true,
                sync_mode: SyncMode::Mirror,
                local_path: None,
                remote_updated_at: None,
            },
        )
        .await
        .unwrap();
        let layout = RepositoryLayout::new(old_root, "acme", "acme", "widgets").unwrap();
        tokio::fs::create_dir_all(&layout.mirror_path)
            .await
            .unwrap();
        tokio::fs::create_dir_all(&layout.working_path)
            .await
            .unwrap();
        set_local_path(pool, repo.id, Some(&layout.mirror_path))
            .await
            .unwrap();
        (repo.id, layout.mirror_path, layout.working_path)
    }

    #[tokio::test]
    async fn moves_both_sides_and_updates_local_path() {
        let (_dir, db) = open_test_db().await;
        let old_root = _dir.path().join("old");
        let new_root = _dir.path().join("new");
        let (repo_id, old_mirror, old_working) = seed_repo(&db.pool, &old_root).await;
        let report = migrate_storage_root(&db.pool, &old_root, &new_root).await;
        assert_eq!(report.moved, 2, "report: {report:?}");
        assert!(report.failed.is_empty(), "report: {report:?}");
        assert!(!old_mirror.exists());
        assert!(!old_working.exists());
        let new_layout = RepositoryLayout::new(&new_root, "acme", "acme", "widgets").unwrap();
        assert!(new_layout.mirror_path.is_dir());
        assert!(new_layout.working_path.is_dir());
        let repo = get_repository(&db.pool, repo_id).await.unwrap().unwrap();
        assert_eq!(
            repo.local_path.as_deref(),
            Some(new_layout.mirror_path.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn repo_without_an_archive_is_skipped() {
        let (_dir, db) = open_test_db().await;
        let old_root = _dir.path().join("old");
        let new_root = _dir.path().join("new");
        seed_repo_no_archive(&db.pool).await;
        let report = migrate_storage_root(&db.pool, &old_root, &new_root).await;
        assert_eq!(report.moved, 0);
        assert_eq!(report.skipped, 1);
        assert!(report.failed.is_empty());
    }

    async fn seed_repo_no_archive(pool: &SqlitePool) {
        let account = create_account(
            pool,
            NewAccount {
                provider: Provider::GitHub,
                provider_account_id: "prov-2".to_string(),
                username: "acme".to_string(),
                display_name: None,
                label: None,
                account_type: AccountType::Personal,
                authentication_method: AuthenticationMethod::FineGrainedPat,
            },
        )
        .await
        .unwrap();
        create_repository(
            pool,
            NewRepository {
                account_id: account.id,
                provider_repository_id: "repo-2".to_string(),
                owner: "acme".to_string(),
                name: "empty".to_string(),
                visibility: Visibility::Public,
                default_branch: Some("main".to_string()),
                clone_url: "https://github.com/acme/empty.git".to_string(),
                sync_enabled: true,
                sync_mode: SyncMode::Mirror,
                local_path: None,
                remote_updated_at: None,
            },
        )
        .await
        .unwrap();
    }

    #[test]
    fn summary_reads_naturally() {
        let report = StorageMigrationReport {
            moved: 3,
            skipped: 1,
            failed: vec![("a/b".to_string(), "locked".to_string())],
        };
        assert_eq!(
            report.summary(),
            "Moved 3 archive(s); 1 repo(s) had no archive; 1 failed"
        );
    }
}
