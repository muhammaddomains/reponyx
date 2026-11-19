//! GitHub App installation registry: CRUD, status transitions.
//!
//! Identity: `(account_id, installation_id)` is unique (V1 schema).

use crate::common::{col, col_uuid, invalid, map_error, now_iso, require_non_empty};
use reponyx_core::{Installation, InstallationStatus, InstallationType, RepoNyxError};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteRow;
use std::str::FromStr;
use uuid::Uuid;

/// Fields for creating an installation. `permissions_json` is a JSON
/// object describing the granted permissions; `{}` when unknown.
#[derive(Debug, Clone)]
pub struct NewInstallation {
    pub account_id: Uuid,
    pub installation_id: String,
    pub installation_type: InstallationType,
    pub permissions_json: String,
}

fn row_to_installation(row: &SqliteRow) -> Result<Installation, sqlx::Error> {
    Ok(Installation {
        id: col_uuid(row, "id")?,
        account_id: col_uuid(row, "account_id")?,
        installation_id: col(row, "installation_id")?,
        installation_type: InstallationType::from_str(&col::<String>(row, "installation_type")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        installation_status: InstallationStatus::from_str(&col::<String>(
            row,
            "installation_status",
        )?)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        permissions_json: col(row, "permissions_json")?,
        created_at: col(row, "created_at")?,
        updated_at: col(row, "updated_at")?,
    })
}

pub async fn create_installation(
    pool: &SqlitePool,
    input: NewInstallation,
) -> Result<Installation, RepoNyxError> {
    require_non_empty("installation_id", &input.installation_id)?;
    if serde_json::from_str::<serde_json::Value>(&input.permissions_json).is_err() {
        return Err(invalid("permissions_json must be valid JSON"));
    }
    let id = Uuid::new_v4();
    let now = now_iso();
    sqlx::query(
        r#"
        INSERT INTO installations
            (id, account_id, installation_id, installation_type,
             installation_status, permissions_json, created_at, updated_at)
        VALUES (?, ?, ?, ?, 'active', ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.account_id.to_string())
    .bind(input.installation_id.trim())
    .bind(input.installation_type.as_str())
    .bind(input.permissions_json)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_installation(pool, id)
        .await?
        .ok_or_else(|| invalid("installation just created but not found"))
}

pub async fn get_installation(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Option<Installation>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM installations WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_installation(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn get_installation_by_account_and_id(
    pool: &SqlitePool,
    account_id: Uuid,
    installation_id: &str,
) -> Result<Option<Installation>, RepoNyxError> {
    let row =
        sqlx::query("SELECT * FROM installations WHERE account_id = ? AND installation_id = ?")
            .bind(account_id.to_string())
            .bind(installation_id)
            .fetch_optional(pool)
            .await
            .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_installation(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn list_installations_for_account(
    pool: &SqlitePool,
    account_id: Uuid,
) -> Result<Vec<Installation>, RepoNyxError> {
    let rows =
        sqlx::query("SELECT * FROM installations WHERE account_id = ? ORDER BY created_at ASC")
            .bind(account_id.to_string())
            .fetch_all(pool)
            .await
            .map_err(map_error)?;
    rows.iter()
        .map(row_to_installation)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

/// Change an installation's status, enforcing the allowed-transition
/// table (`deleted` is terminal).
pub async fn set_installation_status(
    pool: &SqlitePool,
    id: Uuid,
    new_status: InstallationStatus,
) -> Result<Installation, RepoNyxError> {
    let current = get_installation(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("installation {id} not found")))?;
    if current.installation_status != new_status
        && !current
            .installation_status
            .allowed_transitions()
            .contains(&new_status)
    {
        return Err(invalid(format!(
            "installation status cannot move from {} to {}",
            current.installation_status.as_str(),
            new_status.as_str()
        )));
    }
    let now = now_iso();
    sqlx::query("UPDATE installations SET installation_status = ?, updated_at = ? WHERE id = ?")
        .bind(new_status.as_str())
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_installation(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("installation {id} not found")))
}

pub async fn delete_installation(pool: &SqlitePool, id: Uuid) -> Result<bool, RepoNyxError> {
    let result = sqlx::query("DELETE FROM installations WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    Ok(result.rows_affected() > 0)
}
