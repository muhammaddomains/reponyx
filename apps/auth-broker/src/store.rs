//! Durable broker state (Phase 10, F1): a SQLite backing store for
//! [`BrokerState`].
//!
//! The broker keeps its working state in memory maps (fast reads,
//! the same behaviour as the in-memory-only Phase 5 broker) and
//! writes every mutation through to this store first; on startup
//! the maps are loaded from the store, so a restart no longer loses
//! installations, pairing codes, setup `state` values, device
//! tokens, or the audit log.
//!
//! Only SHA-256 hashes of pairing codes, `state` values, and device
//! tokens are stored — never the raw values. The minted
//! installation-token cache is deliberately NOT persisted: tokens
//! are short-lived and are re-minted on demand after a restart.
//! The rate-limiter buckets are likewise in-memory only (a restart
//! resets them; see the threat model).

use crate::audit::AuditEvent;
use crate::state::{
    DeviceTokenRecord, InstallationRecord, PairingCodeRecord, PairingStatus, SetupStateRecord,
};
use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// Audit events older than this are pruned at startup. The log is
/// append-only by API, but a broker that runs for years would grow
/// the table without bound; 90 days covers incident review.
pub const AUDIT_RETENTION_DAYS: i64 = 90;

/// SQLite backing store for broker state.
#[derive(Debug, Clone)]
pub struct BrokerStore {
    pool: SqlitePool,
}

fn to_text(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn parse_time(value: String) -> Result<DateTime<Utc>, sqlx::Error> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

impl BrokerStore {
    /// Open (creating it if needed) the store at `path`, creating
    /// parent directories and running migrations.
    pub async fn open(path: &Path) -> Result<Self, sqlx::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
        }
        let url = format!("sqlite://{}?mode=rwc", path.to_string_lossy());
        let options = SqliteConnectOptions::from_str(&url)?
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5))
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        sqlx::migrate!("../auth-broker/migrations")
            .run(&pool)
            .await?;
        Ok(Self { pool })
    }

    // -- installations -------------------------------------------------

    pub async fn upsert_installation(
        &self,
        record: &InstallationRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO broker_installations (installation_id, setup_action, account_login, installed_at, paired_device)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(installation_id) DO UPDATE SET
               setup_action = excluded.setup_action,
               account_login = excluded.account_login,
               installed_at = excluded.installed_at,
               paired_device = excluded.paired_device",
        )
        .bind(&record.installation_id)
        .bind(&record.setup_action)
        .bind(&record.account_login)
        .bind(to_text(record.installed_at))
        .bind(&record.paired_device)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_installations(&self) -> Result<Vec<InstallationRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT installation_id, setup_action, account_login, installed_at, paired_device
             FROM broker_installations ORDER BY installed_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(InstallationRecord {
                installation_id: row.get("installation_id"),
                setup_action: row.get("setup_action"),
                account_login: row.get("account_login"),
                installed_at: parse_time(row.get("installed_at"))?,
                paired_device: row.get("paired_device"),
            });
        }
        Ok(out)
    }

    // -- pairing codes -------------------------------------------------

    pub async fn insert_pairing_code(&self, record: &PairingCodeRecord) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO broker_pairing_codes (code_hash, installation_id, device_name, device_identifier, created_at, expires_at, attempts, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.code_hash)
        .bind(&record.installation_id)
        .bind(&record.device_name)
        .bind(&record.device_identifier)
        .bind(to_text(record.created_at))
        .bind(to_text(record.expires_at))
        .bind(record.attempts as i64)
        .bind(record.status.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_pairing_code(&self, record: &PairingCodeRecord) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE broker_pairing_codes SET attempts = ?, status = ? WHERE code_hash = ?")
            .bind(record.attempts as i64)
            .bind(record.status.as_str())
            .bind(&record.code_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_pairing_codes(&self) -> Result<Vec<PairingCodeRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT code_hash, installation_id, device_name, device_identifier, created_at, expires_at, attempts, status
             FROM broker_pairing_codes ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let status: String = row.get("status");
            out.push(PairingCodeRecord {
                code_hash: row.get("code_hash"),
                installation_id: row.get("installation_id"),
                device_name: row.get("device_name"),
                device_identifier: row.get("device_identifier"),
                created_at: parse_time(row.get("created_at"))?,
                expires_at: parse_time(row.get("expires_at"))?,
                attempts: row.get::<i64, _>("attempts") as u32,
                status: match status.as_str() {
                    "confirmed" => PairingStatus::Confirmed,
                    "revoked" => PairingStatus::Revoked,
                    _ => PairingStatus::Pending,
                },
            });
        }
        Ok(out)
    }

    /// Delete pairing codes whose TTL has passed. Called at startup
    /// (expired codes are inert, but there is no reason to keep
    /// them) and before issuing a new code.
    pub async fn prune_expired_pairing_codes(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM broker_pairing_codes WHERE expires_at < ?")
            .bind(to_text(Utc::now()))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // -- setup states --------------------------------------------------

    pub async fn insert_setup_state(&self, record: &SetupStateRecord) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO broker_setup_states (state_hash, created_at, expires_at, used)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&record.state_hash)
        .bind(to_text(record.created_at))
        .bind(to_text(record.expires_at))
        .bind(record.used as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_setup_state(&self, record: &SetupStateRecord) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE broker_setup_states SET used = ? WHERE state_hash = ?")
            .bind(record.used as i64)
            .bind(&record.state_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_setup_states(&self) -> Result<Vec<SetupStateRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT state_hash, created_at, expires_at, used FROM broker_setup_states ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(SetupStateRecord {
                state_hash: row.get("state_hash"),
                created_at: parse_time(row.get("created_at"))?,
                expires_at: parse_time(row.get("expires_at"))?,
                used: row.get::<i64, _>("used") != 0,
            });
        }
        Ok(out)
    }

    /// Delete setup states whose TTL has passed or that were already
    /// consumed (single-use: a consumed state is never valid again).
    pub async fn prune_setup_states(&self) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM broker_setup_states WHERE expires_at < ? OR used != 0")
                .bind(to_text(Utc::now()))
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    // -- device tokens -------------------------------------------------

    pub async fn insert_device_token(&self, record: &DeviceTokenRecord) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO broker_device_tokens (token_hash, device_identifier, device_name, installation_id, created_at, revoked)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.token_hash)
        .bind(&record.device_identifier)
        .bind(&record.device_name)
        .bind(&record.installation_id)
        .bind(to_text(record.created_at))
        .bind(record.revoked as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_device_token(&self, record: &DeviceTokenRecord) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE broker_device_tokens SET revoked = ? WHERE token_hash = ?")
            .bind(record.revoked as i64)
            .bind(&record.token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_device_tokens(&self) -> Result<Vec<DeviceTokenRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT token_hash, device_identifier, device_name, installation_id, created_at, revoked
             FROM broker_device_tokens ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(DeviceTokenRecord {
                token_hash: row.get("token_hash"),
                device_identifier: row.get("device_identifier"),
                device_name: row.get("device_name"),
                installation_id: row.get("installation_id"),
                created_at: parse_time(row.get("created_at"))?,
                revoked: row.get::<i64, _>("revoked") != 0,
            });
        }
        Ok(out)
    }

    // -- audit events --------------------------------------------------

    pub async fn insert_audit_event(&self, event: &AuditEvent) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO broker_audit_events (id, timestamp, actor, action, outcome, detail)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(to_text(event.timestamp))
        .bind(&event.actor)
        .bind(&event.action)
        .bind(&event.outcome)
        .bind(&event.detail)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_audit_events(&self) -> Result<Vec<AuditEvent>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, timestamp, actor, action, outcome, detail
             FROM broker_audit_events ORDER BY timestamp",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(AuditEvent {
                id: row.get("id"),
                timestamp: parse_time(row.get("timestamp"))?,
                actor: row.get("actor"),
                action: row.get("action"),
                outcome: row.get("outcome"),
                detail: row.get("detail"),
            });
        }
        Ok(out)
    }

    /// Delete audit events older than the retention window.
    pub async fn prune_audit_events(&self) -> Result<u64, sqlx::Error> {
        let cutoff = Utc::now() - chrono::Duration::days(AUDIT_RETENTION_DAYS);
        let result = sqlx::query("DELETE FROM broker_audit_events WHERE timestamp < ?")
            .bind(to_text(cutoff))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
