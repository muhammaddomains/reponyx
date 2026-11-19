//! Audit-log registry: append-only.
//!
//! There is deliberately **no update or delete API** for audit logs:
//! entries are evidence, and the registry only ever appends and lists.
//! Listing supports filters (actor, resource type/id, time range)
//! with a capped limit.

use crate::common::{col, col_uuid, invalid, map_error, now_iso, opt_col};
use reponyx_core::{AuditLog, RepoNyxError};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, SqlitePool};
use uuid::Uuid;

/// Fields for appending one audit entry. `metadata` is a JSON object
/// (defaults to `{}`); anything else is rejected as `InvalidInput`.
#[derive(Debug, Clone)]
pub struct NewAuditLog {
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Filters for listing audit logs. All fields are optional; `limit`
/// is capped at 1000.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub actor: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

fn row_to_audit_log(row: &SqliteRow) -> Result<AuditLog, sqlx::Error> {
    Ok(AuditLog {
        id: col_uuid(row, "id")?,
        actor: col(row, "actor")?,
        action: col(row, "action")?,
        resource_type: col(row, "resource_type")?,
        resource_id: opt_col(row, "resource_id")?,
        metadata: col(row, "metadata")?,
        created_at: col(row, "created_at")?,
    })
}

pub async fn append_audit_log(
    pool: &SqlitePool,
    input: NewAuditLog,
) -> Result<AuditLog, RepoNyxError> {
    if input.actor.trim().is_empty() {
        return Err(invalid("actor must not be empty"));
    }
    if input.action.trim().is_empty() {
        return Err(invalid("action must not be empty"));
    }
    if input.resource_type.trim().is_empty() {
        return Err(invalid("resource_type must not be empty"));
    }
    let metadata = match &input.metadata {
        Some(value) if !value.is_object() => {
            return Err(invalid("audit metadata must be a JSON object"));
        }
        Some(value) => serde_json::to_string(value).map_err(|e| {
            RepoNyxError::new(
                reponyx_core::ErrorCode::InvalidInput,
                "metadata not serializable",
            )
            .with_details(e.to_string())
        })?,
        None => "{}".to_string(),
    };
    let id = Uuid::new_v4();
    let now = now_iso();
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (id, actor, action, resource_type, resource_id, metadata, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(input.actor.trim())
    .bind(input.action.trim())
    .bind(input.resource_type.trim())
    .bind(input.resource_id)
    .bind(metadata)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(map_error)?;
    let row = sqlx::query("SELECT * FROM audit_logs WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(pool)
        .await
        .map_err(map_error)?;
    row_to_audit_log(&row).map_err(map_error)
}

pub async fn get_audit_log(pool: &SqlitePool, id: Uuid) -> Result<Option<AuditLog>, RepoNyxError> {
    let row = sqlx::query("SELECT * FROM audit_logs WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(map_error)?;
    match row {
        Some(row) => Ok(Some(row_to_audit_log(&row).map_err(map_error)?)),
        None => Ok(None),
    }
}

/// Audit entries, newest first.
pub async fn list_audit_logs(
    pool: &SqlitePool,
    filter: AuditFilter,
) -> Result<Vec<AuditLog>, RepoNyxError> {
    let mut builder = QueryBuilder::new("SELECT * FROM audit_logs");
    let mut first = true;
    let mut push_condition = |builder: &mut QueryBuilder<sqlx::Sqlite>| {
        if first {
            builder.push(" WHERE ");
            first = false;
        } else {
            builder.push(" AND ");
        }
    };
    if let Some(actor) = &filter.actor {
        push_condition(&mut builder);
        builder.push("actor = ");
        builder.push_bind(actor);
    }
    if let Some(resource_type) = &filter.resource_type {
        push_condition(&mut builder);
        builder.push("resource_type = ");
        builder.push_bind(resource_type);
    }
    if let Some(resource_id) = &filter.resource_id {
        push_condition(&mut builder);
        builder.push("resource_id = ");
        builder.push_bind(resource_id);
    }
    if let Some(from) = filter.from {
        push_condition(&mut builder);
        builder.push("created_at >= ");
        builder.push_bind(from);
    }
    if let Some(to) = filter.to {
        push_condition(&mut builder);
        builder.push("created_at <= ");
        builder.push_bind(to);
    }
    builder.push(" ORDER BY created_at DESC LIMIT ");
    builder.push_bind(filter.limit.unwrap_or(100).clamp(1, 1000));
    let rows = builder.build().fetch_all(pool).await.map_err(map_error)?;
    rows.iter()
        .map(row_to_audit_log)
        .collect::<Result<_, _>>()
        .map_err(map_error)
}
