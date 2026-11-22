//! Tauri commands for the Phase 8 Settings page.
//!
//! Only four keys are writable, and each value is validated here
//! before it is stored — the settings table is internal state, not
//! a free-form store. Two of the keys were previously write-only
//! values nothing consumed: `sync.default_mode` is now read by
//! discovery (new repositories get that mode; existing rows keep
//! theirs) and `ui.theme` is applied by the frontend on startup.
//! `sync.concurrency` is read by the queue at agent start
//! (clamped 1..=10 there too); `storage.root` takes effect after
//! the agent restarts, and existing archives stay at the old
//! location — the page says so, rather than pretending a move
//! happens.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works, and the CI windows-build job compiles this crate.

use crate::auth::{CommandResult, invalid_input, require_db};
use reponyx_database::Database;
use reponyx_database::settings::{get_setting, set_setting};
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
/// are refused — callers can only write the four keys the Settings
/// page offers. The rules live in `reponyx-database` (`config`)
/// so the config import validates values the same way; this just
/// maps the error into a command error.
fn validate_setting(key: &str, value: &str) -> CommandResult<()> {
    reponyx_database::config::validate_setting_value(key, value)
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
    let saved = set_setting(&database.pool, key.trim(), &value).await?;
    Ok(saved.value)
}
