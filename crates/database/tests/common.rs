#![allow(dead_code)]
//! Shared helpers for database integration tests: temp databases and
//! fixture builders. Every test opens its own temp SQLite file so
//! WAL mode is real.

use reponyx_core::{
    Account, AccountType, AuthenticationMethod, Provider, Repository, SyncMode, Visibility,
};
use reponyx_database::Database;
use reponyx_database::accounts::{NewAccount, create_account};
use reponyx_database::repositories::{NewRepository, create_repository};
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;

pub async fn open_test_db() -> (TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = reponyx_database::open_database(&dir.path().join("reponyx.sqlite"))
        .await
        .unwrap();
    (dir, db)
}

pub fn pool(db: &Database) -> &SqlitePool {
    &db.pool
}

pub async fn make_account(pool: &SqlitePool, username: &str) -> Account {
    create_account(
        pool,
        NewAccount {
            provider: Provider::GitHub,
            provider_account_id: format!("prov-{username}"),
            username: username.to_string(),
            display_name: None,
            label: None,
            account_type: AccountType::Personal,
            authentication_method: AuthenticationMethod::FineGrainedPat,
        },
    )
    .await
    .unwrap()
}

pub async fn make_repository(
    pool: &SqlitePool,
    account_id: Uuid,
    provider_repository_id: &str,
    owner: &str,
    name: &str,
) -> Repository {
    create_repository(
        pool,
        NewRepository {
            account_id,
            provider_repository_id: provider_repository_id.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            visibility: Visibility::Private,
            default_branch: Some("main".to_string()),
            clone_url: format!("https://github.com/{owner}/{name}.git"),
            sync_enabled: true,
            sync_mode: SyncMode::Mirror,
            local_path: None,
            remote_updated_at: None,
        },
    )
    .await
    .unwrap()
}
