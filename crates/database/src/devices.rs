//! Device registry: CRUD, pairing-status transitions, lookup by
//! device identifier.
//!
//! Identity: `device_identifier` is unique (V1 schema). Pairing
//! transitions follow `DevicePairingStatus::allowed_transitions`:
//! `revoked` is terminal — a revoked device registers again, it is
//! never re-paired in place.

use crate::common::{
    col, col_uuid, invalid, map_error, now_iso, now_utc, opt_col, require_non_empty,
};
use reponyx_core::{Device, DevicePairingStatus, RepoNyxError};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteRow;
use std::str::FromStr;
use uuid::Uuid;

/// Fields for registering a device. New devices start `pending`.
#[derive(Debug, Clone)]
pub struct NewDevice {
    pub device_name: String,
    pub device_identifier: String,
}

fn row_to_device(row: &SqliteRow) -> Result<Device, sqlx::Error> {
    Ok(Device {
        id: col_uuid(row, "id")?,
        device_name: col(row, "device_name")?,
        device_identifier: col(row, "device_identifier")?,
        pairing_status: DevicePairingStatus::from_str(&col::<String>(row, "pairing_status")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        created_at: col(row, "created_at")?,
        last_seen_at: opt_col(row, "last_seen_at")?,
    })
}

pub async fn create_device(pool: &SqlitePool, input: NewDevice) -> Result<Device, RepoNyxError> {
    require_non_empty("device_name", &input.device_name)?;
    require_non_empty("device_identifier", &input.device_identifier)?;
    let id = Uuid::new_v4();
    let now = now_iso();
    sqlx::query(
        r#"
        INSERT INTO devices
            (id, device_name, device_identifier, pairing_status, created_at)
        VALUES (?, ?, ?, 'pending', ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.device_name.trim())
    .bind(input.device_identifier.trim())
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    get_device(pool, id)
        .await?
        .ok_or_else(|| invalid("device just created but not found"))
}

pub async fn get_device(pool: &SqlitePool, id: Uuid) -> Result<Option<Device>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM devices WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_device(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn get_device_by_identifier(
    pool: &SqlitePool,
    device_identifier: &str,
) -> Result<Option<Device>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM devices WHERE device_identifier = ?")
        .bind(device_identifier)
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_device(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

pub async fn list_devices(pool: &SqlitePool) -> Result<Vec<Device>, RepoNyxError> {
    let rows = sqlx::query("SELECT * FROM devices ORDER BY device_name ASC")
        .fetch_all(pool)
        .await
        .map_err(map_error)?;
    rows.iter()
        .map(row_to_device)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}

/// Change a device's pairing status, enforcing the
/// allowed-transition table.
pub async fn set_pairing_status(
    pool: &SqlitePool,
    id: Uuid,
    new_status: DevicePairingStatus,
) -> Result<Device, RepoNyxError> {
    let current = get_device(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("device {id} not found")))?;
    if current.pairing_status != new_status
        && !current
            .pairing_status
            .allowed_transitions()
            .contains(&new_status)
    {
        return Err(invalid(format!(
            "device pairing status cannot move from {} to {}",
            current.pairing_status.as_str(),
            new_status.as_str()
        )));
    }
    sqlx::query("UPDATE devices SET pairing_status = ? WHERE id = ?")
        .bind(new_status.as_str())
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_device(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("device {id} not found")))
}

/// Record that a device was seen (updates `last_seen_at`).
pub async fn touch_device(pool: &SqlitePool, id: Uuid) -> Result<Device, RepoNyxError> {
    sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
        .bind(now_utc())
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    get_device(pool, id)
        .await?
        .ok_or_else(|| invalid(format!("device {id} not found")))
}

pub async fn delete_device(pool: &SqlitePool, id: Uuid) -> Result<bool, RepoNyxError> {
    let result = sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(map_error)?;
    Ok(result.rows_affected() > 0)
}
