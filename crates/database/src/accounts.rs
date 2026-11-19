//! Account registry: CRUD, validation, status transitions.
//!
//! Identity: `(provider, provider_account_id)` is unique (V1 schema).
//! Status transitions follow the table in
//! `AccountStatus::allowed_transitions` (`reponyx-core`): `revoked`
//! and `token_expired` are terminal-ish — they return to `connected`
//! only through an explicit `reconnect_account`, never through
//! `set_account_status`.

use crate::common::{col, col_uuid, invalid, map_error, now_iso, opt_col, require_non_empty};
use reponyx_core::{
    Account, AccountStatus, AccountType, AuthenticationMethod, Provider, RepoNyxError,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use uuid::Uuid;

/// Fields for creating an account. `provider` and
/// `authentication_method` are enums, so unknown values cannot be
/// constructed; `provider_account_id` and `username` are validated
/// non-empty here.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub provider: Provider,
    pub provider_account_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub label: Option<String>,
    pub account_type: AccountType,
    pub authentication_method: AuthenticationMethod,
}

/// Optional fields for `update_account`. `Option<Option<String>>`:
/// outer `None` leaves the column alone, inner `None` clears it.
#[derive(Debug, Clone, Default)]
pub struct AccountPatch {
    pub display_name: Option<Option<String>>,
    pub label: Option<Option<String>>,
    pub account_type: Option<AccountType>,
}

fn row_to_account(row: &SqliteRow) -> Result<Account, sqlx::Error> {
    Ok(Account {
        id: col_uuid(row, "id")?,
        provider: Provider::from_str(&col::<String>(row, "provider")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        provider_account_id: col(row, "provider_account_id")?,
        username: col(row, "username")?,
        display_name: opt_col(row, "display_name")?,
        label: opt_col(row, "label")?,
        account_type: AccountType::from_str(&col::<String>(row, "account_type")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        authentication_method: AuthenticationMethod::from_str(&col::<String>(
            row,
            "authentication_method",
        )?)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        status: AccountStatus::from_str(&col::<String>(row, "connection_status")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        last_validated_at: opt_col(row, "last_validated_at")?,
        created_at: col(row, "created_at")?,
        updated_at: col(row, "updated_at")?,
    })
}

pub async fn create_account(pool: &SqlitePool, input: NewAccount) -> Result<Account, RepoNyxError> {
    require_non_empty("provider_account_id", &input.provider_account_id)?;
    require_non_empty("username", &input.username)?;
    let id = Uuid::new_v4();
    let now = now_iso();
    sqlx::query(
        r#"
        INSERT INTO accounts
            (id, provider, provider_account_id, username, display_name, label,
             account_type, authentication_method, connection_status,
             created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'connected', ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.provider.as_str())
    .bind(input.provider_account_id.trim())
    .bind(input.username.trim())
    .bind(input.display_name)
    .bind(input.label)
    .bind(input.account_type.as_str())
    .bind(input.authentication_method.as_str())
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_account(pool, id)
        .await?
        .ok_or_else(|| invalid("account just created but not found"))
}

pub async fn get_account(pool: &SqlitePool, id: Uuid) -> Result<Option<Account>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM accounts WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_account(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn get_account_by_provider(
    pool: &SqlitePool,
    provider: Provider,
    provider_account_id: &str,
) -> Result<Option<Account>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM accounts WHERE provider = ? AND provider_account_id = ?")
        .bind(provider.as_str())
        .bind(provider_account_id)
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_account(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn list_accounts(pool: &SqlitePool) -> Result<Vec<Account>, RepoNyxError> {
    let rows = sqlx::query("SELECT * FROM accounts ORDER BY username ASC")
        .fetch_all(pool)
        .await
        .map_err(map_error)?;
    rows.iter()
        .map(row_to_account)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

pub async fn count_accounts(pool: &SqlitePool) -> Result<i64, RepoNyxError> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM accounts")
        .fetch_one(pool)
        .await
        .map_err(map_error)?;
    Ok(row.get("n"))
}

pub async fn update_account(
    pool: &SqlitePool,
    id: Uuid,
    patch: AccountPatch,
) -> Result<Account, RepoNyxError> {
    let mut builder = sqlx::QueryBuilder::new("UPDATE accounts SET updated_at = ");
    builder.push_bind(now_iso());
    if let Some(display_name) = patch.display_name {
        builder.push(", display_name = ");
        builder.push_bind(display_name);
    }
    if let Some(label) = patch.label {
        builder.push(", label = ");
        builder.push_bind(label);
    }
    if let Some(account_type) = patch.account_type {
        builder.push(", account_type = ");
        builder.push_bind(account_type.as_str());
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id.to_string());
    builder.build().execute(pool).await.map_err(map_error)?;
    get_account(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("account {id} not found")))
}

/// Change an account's status, enforcing the allowed-transition table.
/// Moving from `revoked`/`token_expired` back to `connected` is
/// rejected here — use `reconnect_account` for that.
pub async fn set_account_status(
    pool: &SqlitePool,
    id: Uuid,
    new_status: AccountStatus,
) -> Result<Account, RepoNyxError> {
    let current = get_account(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("account {id} not found")))?;
    if current.status != new_status && !current.status.allowed_transitions().contains(&new_status) {
        return Err(invalid(format!(
            "account status cannot move from {} to {}",
            current.status.as_str(),
            new_status.as_str()
        )));
    }
    let now = now_iso();
    sqlx::query("UPDATE accounts SET connection_status = ?, updated_at = ? WHERE id = ?")
        .bind(new_status.as_str())
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_account(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("account {id} not found")))
}

/// Explicit reconnect: restores a `token_expired`, `revoked` or
/// `error` account to `connected` and refreshes `last_validated_at`.
/// This is the only path from those states back to `connected`.
pub async fn reconnect_account(pool: &SqlitePool, id: Uuid) -> Result<Account, RepoNyxError> {
    let current = get_account(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("account {id} not found")))?;
    if !matches!(
        current.status,
        AccountStatus::TokenExpired | AccountStatus::Revoked | AccountStatus::Error
    ) {
        return Err(invalid(format!(
            "account in status {} does not need reconnect",
            current.status.as_str()
        )));
    }
    let now = now_iso();
    sqlx::query(
        "UPDATE accounts SET connection_status = 'connected', last_validated_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_account(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("account {id} not found")))
}

/// Hard-delete an account. Child rows (installations, repositories,
/// schedules, jobs, history) are removed by the V1 `ON DELETE CASCADE`
/// foreign keys. Returns `true` if a row was deleted.
pub async fn delete_account(pool: &SqlitePool, id: Uuid) -> Result<bool, RepoNyxError> {
    let result = sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    Ok(result.rows_affected() > 0)
}
