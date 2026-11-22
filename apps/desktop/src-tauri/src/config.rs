//! Tauri commands for config import/export (Phase 9).
//!
//! The config file is a versioned JSON document (`reponyx-config`,
//! version 1) with the four non-secret settings, each repository's
//! sync selection, and the schedules — see the `config` module in
//! `reponyx-database` for the format and the apply rules. No
//! credentials, tokens, local paths, or archive state ever travels
//! in it.
//!
//! Both commands open a native file dialog (`tauri-plugin-dialog`):
//! a Save dialog for export, an Open dialog for import. Cancelling
//! the dialog returns `None` — nothing is written or applied.
//! Import validates the whole file before writing anything, so a
//! malformed file refuses the import instead of applying half of
//! it; entries whose repository or account does not exist on this
//! machine are skipped and listed in the report.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. The format, validation, and apply
//! rules live in `reponyx-database` and are covered there.

use crate::auth::{CommandError, CommandResult, require_db};
use reponyx_database::Database;
use reponyx_database::config::{
    ImportReport, apply_config_import, build_config_export, parse_config_file,
};
use reponyx_scheduler::next_run;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigExportDto {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRefDto {
    pub kind: String,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReportDto {
    pub settings_applied: usize,
    pub repositories_applied: usize,
    pub schedules_created: usize,
    pub schedules_updated: usize,
    pub skipped: Vec<SkippedRefDto>,
}

impl From<ImportReport> for ImportReportDto {
    fn from(report: ImportReport) -> Self {
        Self {
            settings_applied: report.settings_applied,
            repositories_applied: report.repositories_applied,
            schedules_created: report.schedules_created,
            schedules_updated: report.schedules_updated,
            skipped: report
                .skipped
                .into_iter()
                .map(|s| SkippedRefDto {
                    kind: s.kind,
                    label: s.label,
                    reason: s.reason,
                })
                .collect(),
        }
    }
}

// -- Helpers ----------------------------------------------------------

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

/// `YYYYMMDD-HHMMSS` (UTC) for the suggested export file name.
fn timestamp_now() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// The file path a native dialog returned. Desktop dialogs return
/// a plain path; a URL (mobile content URIs) is refused rather
/// than guessed at.
fn dialog_path(picked: tauri_plugin_dialog::FilePath) -> CommandResult<std::path::PathBuf> {
    match picked {
        tauri_plugin_dialog::FilePath::Path(path) => Ok(path),
        tauri_plugin_dialog::FilePath::Url(_) => {
            Err(invalid("the file dialog returned a URL, not a file path"))
        }
    }
}

// -- Commands ---------------------------------------------------------

/// Build the config file and let the user choose where to save it.
/// `None` when the dialog is cancelled — nothing is written.
#[tauri::command]
pub async fn export_config_command(
    app: AppHandle,
    db: State<'_, Option<Database>>,
) -> CommandResult<Option<ConfigExportDto>> {
    let database = require_db(&db)?;
    let file = build_config_export(&database.pool, env!("CARGO_PKG_VERSION")).await?;
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| internal(format!("cannot serialize config: {e}")))?;
    let name = format!("reponyx-config-{}.json", timestamp_now());
    // The blocking dialog API is the documented choice inside
    // async commands; the dialog runs its own event loop.
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("RepoNyx config", &["json"])
        .set_file_name(name.as_str())
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = dialog_path(picked)?;
    std::fs::write(&path, &json)
        .map_err(|e| internal(format!("cannot write {}: {e}", path.display())))?;
    Ok(Some(ConfigExportDto {
        path: path.to_string_lossy().into_owned(),
        bytes: json.len() as u64,
    }))
}

/// Let the user pick a config file, then validate and apply it.
/// `None` when the dialog is cancelled — nothing is applied.
#[tauri::command]
pub async fn import_config_command(
    app: AppHandle,
    db: State<'_, Option<Database>>,
) -> CommandResult<Option<ImportReportDto>> {
    let database = require_db(&db)?;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("RepoNyx config", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = dialog_path(picked)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| internal(format!("cannot read {}: {e}", path.display())))?;
    let file = parse_config_file(&text)?;
    let report = apply_config_import(&database.pool, &file, next_run).await?;
    Ok(Some(ImportReportDto::from(report)))
}
