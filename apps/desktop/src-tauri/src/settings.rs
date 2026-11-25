//! Tauri commands for the Phase 8 Settings page.
//!
//! Only five keys are writable, and each value is validated here
//! before it is stored — the settings table is internal state, not
//! a free-form store. Two of the keys were previously write-only
//! values nothing consumed: `sync.default_mode` is now read by
//! discovery (new repositories get that mode; existing rows keep
//! theirs) and `ui.theme` is applied by the frontend on startup.
//! `sync.concurrency` is read by the queue at agent start
//! (clamped 1..=10 there too); `storage.root` takes effect after
//! the agent restarts, and changing it moves the existing archives
//! to the new root (audit ST-M1) — the migration summary comes back
//! in the command result. `notifications.enabled` is read by the frontend's sync
//! notification hook (on unless explicitly set to `false`).
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works, and the CI windows-build job compiles this crate.

use crate::auth::{CommandResult, invalid_input, require_db};
use crate::storage::canonical_or_raw;
use reponyx_database::Database;
use reponyx_database::settings::{delete_setting, get_setting, set_setting};
use serde::Serialize;
use tauri::State;

// -- DTO (camelCase, matching the TypeScript type in
// `packages/shared/src/types.ts`) -------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub default_sync_mode: Option<String>,
    pub concurrency: Option<i64>,
    pub storage_root: Option<String>,
    pub theme: Option<String>,
    pub notifications_enabled: Option<bool>,
    pub device_identifier: Option<String>,
    pub data_dir: String,
    pub app_version: String,
}

async fn setting_value(database: &Database, key: &str) -> CommandResult<Option<String>> {
    Ok(get_setting(&database.pool, key)
        .await?
        .map(|setting| setting.value))
}

/// Validate one writable setting before it is stored. Unknown keys
/// are refused — callers can only write the five keys the Settings
/// page offers. The rules live in `reponyx-database` (`config`)
/// so the config import validates values the same way; this just
/// maps the error into a command error.
fn validate_setting(key: &str, value: &str) -> CommandResult<()> {
    reponyx_database::config::validate_setting_value(key, value)
        .map_err(|e| invalid_input(e.message.clone()))?;
    Ok(())
}

/// The disk checks for `storage.root`: the value must name a real,
/// writable directory that is not a drive root and does not hold (or
/// sit inside) the data directory. Runs before the value is stored,
/// so a bad path never lands in the settings table. The config
/// import deliberately does not run these — a config file is
/// cross-machine, and its root may not exist on the importing
/// machine; it keeps the portable absolute-path check.
fn validate_storage_root_on_disk(value: &str) -> CommandResult<()> {
    reponyx_database::validate_storage_root(value, &crate::dirs_data_dir())
        .map_err(|e| invalid_input(e.message.clone()))?;
    Ok(())
}

// -- Commands ---------------------------------------------------------

#[tauri::command]
pub async fn get_settings_command(db: State<'_, Option<Database>>) -> CommandResult<SettingsDto> {
    let database = require_db(&db)?;
    let concurrency = match setting_value(database, "sync.concurrency").await? {
        Some(raw) => raw.trim().parse::<i64>().ok(),
        None => None,
    };
    Ok(SettingsDto {
        default_sync_mode: setting_value(database, "sync.default_mode").await?,
        concurrency,
        storage_root: setting_value(database, "storage.root").await?,
        theme: setting_value(database, "ui.theme").await?,
        notifications_enabled: match setting_value(database, "notifications.enabled").await? {
            Some(raw) => raw.trim().parse::<bool>().ok(),
            None => None,
        },
        device_identifier: setting_value(database, "device.identifier").await?,
        data_dir: crate::dirs_data_dir().to_string_lossy().into_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn set_setting_command(
    db: State<'_, Option<Database>>,
    key: String,
    value: String,
) -> CommandResult<String> {
    let database = require_db(&db)?;
    let value = value.trim().to_string();
    validate_setting(key.trim(), &value)?;
    let key = key.trim();
    // Audit Phase 4: a storage root is checked against the disk
    // before it is stored — a relative path, a missing folder, a
    // drive root, the data directory, or a read-only folder must
    // never land in the settings table.
    if key == "storage.root" {
        validate_storage_root_on_disk(&value)?;
    }
    // Audit ST-M1: changing `storage.root` moves the existing
    // archives to the new root. The setting is saved first, so a
    // failed move never loses the new root; the migration then
    // reports what moved and what stayed behind.
    let old_root = if key == "storage.root" {
        setting_value(database, "storage.root").await?
    } else {
        None
    };
    let saved = set_setting(&database.pool, key, &value).await?;
    if key == "storage.root" {
        let new_root = std::path::Path::new(&saved.value);
        let old_root = old_root
            .map(std::path::PathBuf::from)
            .unwrap_or_else(crate::dirs_data_dir);
        // Compare canonical paths: re-saving the same folder under a
        // different spelling (a symlink, a `..`, a trailing slash)
        // must not trigger a pointless migration.
        if canonical_or_raw(&old_root) != canonical_or_raw(new_root) {
            let report =
                reponyx_sync_queue::migrate_storage_root(&database.pool, &old_root, new_root).await;
            tracing::info!(summary = %report.summary(), "storage root migrated");
            return Ok(format!("Storage root saved. {}", report.summary()));
        }
    }
    Ok(saved.value)
}

/// Clear the `storage.root` setting, restoring the default (the data
/// directory). The archives migrate back with it, exactly as when
/// the root is changed — clearing without moving would strand them
/// under the old root while new syncs went to the default. Only
/// `storage.root` can be cleared; the other settings have no
/// meaningful "unset" (their absence already means the default).
#[tauri::command]
pub async fn clear_setting_command(
    db: State<'_, Option<Database>>,
    key: String,
) -> CommandResult<String> {
    let database = require_db(&db)?;
    let key = key.trim();
    if key != "storage.root" {
        return Err(invalid_input("only storage.root can be cleared"));
    }
    let old_root = setting_value(database, "storage.root").await?;
    let cleared = delete_setting(&database.pool, key).await?;
    if !cleared {
        return Ok("Storage root is already the default.".to_string());
    }
    let old_root = old_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::dirs_data_dir);
    let new_root = crate::dirs_data_dir();
    if canonical_or_raw(&old_root) != canonical_or_raw(&new_root) {
        let report =
            reponyx_sync_queue::migrate_storage_root(&database.pool, &old_root, &new_root).await;
        tracing::info!(summary = %report.summary(), "storage root reset to default");
        return Ok(format!(
            "Storage root reset to the default. {}",
            report.summary()
        ));
    }
    Ok("Storage root reset to the default.".to_string())
}
