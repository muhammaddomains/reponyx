//! Tauri commands for the Phase 7 scheduler (scheduler part only).
//! These commands are thin delegates: the math lives in
//! `reponyx-scheduler`, the rows live in `reponyx-database`; this
//! module only adapts them to the Tauri boundary.
//!
//! The commands create job rows for due schedules (via
//! `materialize_due_command`) — they never run a sync. Running jobs
//! is the Phase 6 sync queue's job, which does not exist yet.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works.

use crate::auth::{CommandError, CommandResult, invalid_input, require_db};
use chrono::{DateTime, Utc};
use reponyx_core::{IntervalType, Schedule, ScheduleScope};
use reponyx_database::Database;
use reponyx_database::schedules::{
    NewSchedule, SchedulePatch, create_schedule, delete_schedule, get_schedule, list_schedules,
    update_schedule,
};
use reponyx_scheduler::{display_time, effective_next_runs, materialize_due, next_run, next_runs};
use serde::Serialize;
use std::str::FromStr;
use tauri::State;
use uuid::Uuid;

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

/// A schedule as stored, plus the next run computed just now (the
/// stored `next_run_at` is authoritative for due detection; this is
/// the display value). `next_run_error` is set instead of
/// `computed_next_run` when the cadence/timezone no longer parses —
/// a broken schedule stays visible rather than silently vanishing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDto {
    pub id: String,
    pub scope: String,
    pub account_id: Option<String>,
    pub repository_id: Option<String>,
    pub interval_type: String,
    pub interval_value: Option<String>,
    pub timezone: String,
    pub enabled: bool,
    pub next_run_at: Option<String>,
    pub computed_next_run: Option<String>,
    pub next_run_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ScheduleDto {
    fn from_schedule(schedule: &Schedule, now: DateTime<Utc>) -> Self {
        let (computed_next_run, next_run_error) = match next_run(schedule, now) {
            Ok(run) => (run.map(|at| at.to_rfc3339()), None),
            Err(e) => (None, Some(e.message.clone())),
        };
        Self {
            id: schedule.id.to_string(),
            scope: schedule.scope.as_str().to_string(),
            account_id: schedule.account_id.map(|id| id.to_string()),
            repository_id: schedule.repository_id.map(|id| id.to_string()),
            interval_type: schedule.interval_type.as_str().to_string(),
            interval_value: schedule.interval_value.clone(),
            timezone: schedule.timezone.clone(),
            enabled: schedule.enabled,
            next_run_at: schedule.next_run_at.map(|at| at.to_rfc3339()),
            computed_next_run,
            next_run_error,
            created_at: schedule.created_at.to_rfc3339(),
            updated_at: schedule.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextRunPreviewDto {
    /// UTC, RFC 3339.
    pub utc: String,
    /// Same instant, formatted in `timezone`.
    pub local: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleMaterializeErrorDto {
    pub schedule_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterializeReportDto {
    pub checked: usize,
    pub primed: usize,
    pub created_job_ids: Vec<String>,
    pub skipped_duplicate: usize,
    pub skipped_sync_disabled: usize,
    pub skipped_missing_repository: usize,
    pub skipped_out_of_window: usize,
    pub errors: Vec<ScheduleMaterializeErrorDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveScheduleDto {
    pub schedule: ScheduleDto,
    /// Next 3 runs, UTC RFC 3339.
    pub next_runs: Vec<String>,
}

// -- helpers ----------------------------------------------------------

fn parse_uuid(id: &str, what: &str) -> CommandResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| invalid_input(format!("invalid {what} id")))
}

fn parse_scope(value: &str) -> CommandResult<ScheduleScope> {
    ScheduleScope::from_str(value).map_err(CommandError::from)
}

fn parse_interval_type(value: &str) -> CommandResult<IntervalType> {
    IntervalType::from_str(value).map_err(CommandError::from)
}

// -- commands ---------------------------------------------------------

#[tauri::command]
pub async fn list_schedules_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<Vec<ScheduleDto>> {
    let database = require_db(&db)?;
    let now = Utc::now();
    let schedules = list_schedules(&database.pool).await?;
    Ok(schedules
        .iter()
        .map(|schedule| ScheduleDto::from_schedule(schedule, now))
        .collect())
}

#[tauri::command]
pub async fn create_schedule_command(
    db: State<'_, Option<Database>>,
    scope: String,
    account_id: Option<String>,
    repository_id: Option<String>,
    interval_type: String,
    interval_value: Option<String>,
    timezone: String,
    enabled: bool,
) -> CommandResult<ScheduleDto> {
    let database = require_db(&db)?;
    let scope = parse_scope(&scope)?;
    let interval_type = parse_interval_type(&interval_type)?;
    let account_id = account_id
        .map(|id| parse_uuid(&id, "account"))
        .transpose()?;
    let repository_id = repository_id
        .map(|id| parse_uuid(&id, "repository"))
        .transpose()?;
    let now = Utc::now();
    // Validate the cadence and timezone (and compute the first run)
    // before writing anything.
    let draft = Schedule {
        id: Uuid::new_v4(),
        scope,
        account_id,
        repository_id,
        interval_type,
        interval_value: interval_value.clone(),
        timezone: timezone.clone(),
        enabled,
        next_run_at: None,
        created_at: now,
        updated_at: now,
    };
    let next = next_run(&draft, now)?;
    let created = create_schedule(
        &database.pool,
        NewSchedule {
            scope,
            account_id,
            repository_id,
            interval_type,
            interval_value,
            timezone,
            enabled,
            next_run_at: next,
        },
    )
    .await?;
    Ok(ScheduleDto::from_schedule(&created, now))
}

#[tauri::command]
pub async fn update_schedule_command(
    db: State<'_, Option<Database>>,
    id: String,
    interval_type: Option<String>,
    interval_value: Option<String>,
    timezone: Option<String>,
    enabled: Option<bool>,
) -> CommandResult<ScheduleDto> {
    let database = require_db(&db)?;
    let id = parse_uuid(&id, "schedule")?;
    let existing = get_schedule(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input(format!("schedule {id} not found")))?;
    let mut updated = existing.clone();
    let mut changed = false;
    if let Some(value) = interval_type {
        updated.interval_type = parse_interval_type(&value)?;
        changed = true;
    }
    if let Some(value) = interval_value {
        // Empty string clears the value; anything else replaces it.
        updated.interval_value = if value.is_empty() { None } else { Some(value) };
        changed = true;
    }
    if let Some(value) = timezone {
        updated.timezone = value;
        changed = true;
    }
    if let Some(value) = enabled {
        updated.enabled = value;
        changed = true;
    }
    let now = Utc::now();
    // Recompute the next run when the cadence changed and the
    // schedule is enabled. The old `next_run_at` stays the interval
    // anchor, so the cadence keeps its alignment; a disabled
    // schedule keeps its stored value as the anchor for re-enable.
    let next_run_at = if changed && updated.enabled {
        Some(next_run(&updated, now)?)
    } else {
        None
    };
    let saved = update_schedule(
        &database.pool,
        id,
        SchedulePatch {
            interval_type: if changed {
                Some(updated.interval_type)
            } else {
                None
            },
            interval_value: if changed {
                Some(updated.interval_value.clone())
            } else {
                None
            },
            timezone: if changed {
                Some(updated.timezone.clone())
            } else {
                None
            },
            enabled: if changed { Some(updated.enabled) } else { None },
            next_run_at,
        },
    )
    .await?;
    Ok(ScheduleDto::from_schedule(&saved, now))
}

#[tauri::command]
pub async fn delete_schedule_command(
    db: State<'_, Option<Database>>,
    id: String,
) -> CommandResult<bool> {
    let database = require_db(&db)?;
    let id = parse_uuid(&id, "schedule")?;
    Ok(delete_schedule(&database.pool, id).await?)
}

#[tauri::command]
pub async fn preview_next_runs_command(
    db: State<'_, Option<Database>>,
    schedule_id: String,
    count: u32,
    display_timezone: Option<String>,
) -> CommandResult<Vec<NextRunPreviewDto>> {
    let database = require_db(&db)?;
    if count == 0 || count > 50 {
        return Err(invalid_input("count must be 1..=50"));
    }
    let id = parse_uuid(&schedule_id, "schedule")?;
    let schedule = get_schedule(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input(format!("schedule {id} not found")))?;
    let timezone = display_timezone.unwrap_or_else(|| schedule.timezone.clone());
    let runs = next_runs(&schedule, Utc::now(), count as usize)?;
    let mut previews = Vec::with_capacity(runs.len());
    for run in runs {
        previews.push(NextRunPreviewDto {
            utc: run.to_rfc3339(),
            local: display_time(run, &timezone)?,
            timezone: timezone.clone(),
        });
    }
    Ok(previews)
}

#[tauri::command]
pub async fn materialize_due_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<MaterializeReportDto> {
    let database = require_db(&db)?;
    let report = materialize_due(&database.pool, Utc::now()).await?;
    Ok(MaterializeReportDto {
        checked: report.checked,
        primed: report.primed,
        created_job_ids: report
            .created
            .iter()
            .map(|job| job.id.to_string())
            .collect(),
        skipped_duplicate: report.skipped_duplicate,
        skipped_sync_disabled: report.skipped_sync_disabled,
        skipped_missing_repository: report.skipped_missing_repository,
        skipped_out_of_window: report.skipped_out_of_window,
        errors: report
            .errors
            .iter()
            .map(|e| ScheduleMaterializeErrorDto {
                schedule_id: e.schedule_id.to_string(),
                message: e.message.clone(),
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn effective_schedule_command(
    db: State<'_, Option<Database>>,
    repository_id: String,
) -> CommandResult<Option<EffectiveScheduleDto>> {
    let database = require_db(&db)?;
    let repository_id = parse_uuid(&repository_id, "repository")?;
    let now = Utc::now();
    let Some(effective) = effective_next_runs(&database.pool, repository_id, now, 3).await? else {
        return Ok(None);
    };
    Ok(Some(EffectiveScheduleDto {
        schedule: ScheduleDto::from_schedule(&effective.schedule, now),
        next_runs: effective.runs.iter().map(|run| run.to_rfc3339()).collect(),
    }))
}
