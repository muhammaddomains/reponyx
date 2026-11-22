//! Due detection and job creation for due schedules.
//!
//! A schedule is due when it is enabled and its stored `next_run_at`
//! has passed. Materializing a due schedule creates one sync-job row
//! per governed repository — and never a duplicate: a repository
//! with a pending or running job is skipped. Advancing the schedule
//! (setting `next_run_at` to the next computed run) is what stops a
//! due schedule being reported due forever; there is no `last_run_at`
//! column in the V1 schema, and none is needed — the advanced
//! `next_run_at` is the progress marker.
//!
//! A due schedule whose stored run falls outside its current window
//! (or on a filtered-out weekday — possible when the window/weekdays
//! were edited after `next_run_at` was computed) creates no job for
//! that cycle: it is advanced to the next valid run and counted as
//! skipped. A schedule whose cadence no longer parses is recorded in
//! the report as an error and left un-advanced, so it stays visible
//! instead of silently never firing.
//!
//! This module creates job rows only. It never runs them.

use crate::cadence::TimeOfDay;
use crate::cadence::parse_cadence;
use crate::next_run::{next_run, next_runs};
use crate::timezone::parse_timezone;
use chrono::{DateTime, Datelike, Timelike, Utc};
use reponyx_core::{
    ErrorCode, JobStatus, JobType, RepoNyxError, Repository, Schedule, ScheduleScope, SyncJob,
    SyncMode,
};
use reponyx_database::jobs::{JobFilter, NewJob, create_job, list_jobs};
use reponyx_database::repositories::{RepositoryFilter, get_repository, list_repositories};
use reponyx_database::schedules::{
    SchedulePatch, due_schedules, effective_schedule_for, list_schedules, update_schedule,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

/// One schedule that could not be materialized, with the reason.
/// Per-schedule failures never abort the whole run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleMaterializeError {
    pub schedule_id: Uuid,
    pub message: String,
}

/// Outcome of one [`materialize_due`] run.
#[derive(Debug, Default)]
pub struct MaterializeReport {
    /// Due schedules examined.
    pub checked: usize,
    /// Schedules whose `next_run_at` was `NULL` and got an initial
    /// value computed (no job created for them).
    pub primed: usize,
    /// Jobs created by this run.
    pub created: Vec<SyncJob>,
    /// Repositories skipped because a pending/running job already
    /// exists for them.
    pub skipped_duplicate: usize,
    /// Repositories skipped because sync is disabled for them.
    pub skipped_sync_disabled: usize,
    /// Repository-scope schedules whose repository no longer exists.
    pub skipped_missing_repository: usize,
    /// Due schedules whose stored run fell outside their current
    /// window/weekdays — advanced, no job created.
    pub skipped_out_of_window: usize,
    /// Schedules that errored (unparsable cadence/timezone, or a
    /// database error while creating/advancing). They are left
    /// un-advanced so they stay visible.
    pub errors: Vec<ScheduleMaterializeError>,
}

/// The effective schedule governing a repository, with its next runs.
#[derive(Debug, Clone)]
pub struct EffectiveNextRuns {
    pub schedule: Schedule,
    pub runs: Vec<DateTime<Utc>>,
}

/// Job type for a freshly materialized job: a repository that has
/// never been cloned gets a clone job for its sync mode; one that
/// already has a local path gets a fetch.
fn job_type_for(repository: &Repository) -> JobType {
    match repository.local_path {
        Some(_) => JobType::Fetch,
        None => match repository.sync_mode {
            SyncMode::Mirror | SyncMode::Both => JobType::MirrorClone,
            SyncMode::Working => JobType::WorkingClone,
        },
    }
}

async fn has_active_job(pool: &SqlitePool, repository_id: Uuid) -> Result<bool, RepoNyxError> {
    for status in [JobStatus::Pending, JobStatus::Running] {
        let jobs = list_jobs(
            pool,
            JobFilter {
                repository_id: Some(repository_id),
                status: Some(status),
                ..Default::default()
            },
        )
        .await?;
        if !jobs.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn advance_schedule(
    pool: &SqlitePool,
    schedule: &Schedule,
    now: DateTime<Utc>,
) -> Result<(), RepoNyxError> {
    let next = next_run(schedule, now)?;
    update_schedule(
        pool,
        schedule.id,
        SchedulePatch {
            next_run_at: Some(next),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

/// Whether the schedule's stored run time is inside its current
/// window and on an allowed weekday. `true` when there is no window
/// or weekday filter to violate.
fn stored_run_in_window(schedule: &Schedule) -> Result<bool, RepoNyxError> {
    let Some(run_at) = schedule.next_run_at else {
        return Ok(false);
    };
    let cadence = parse_cadence(schedule.interval_type, schedule.interval_value.as_deref())?;
    let tz = parse_timezone(&schedule.timezone)?;
    let local = run_at.with_timezone(&tz);
    if let Some(days) = &cadence.weekdays
        && !days.contains(&(local.weekday().number_from_monday() as u8))
    {
        return Ok(false);
    }
    if let Some(window) = &cadence.window {
        let time = TimeOfDay {
            hour: local.hour() as u8,
            minute: local.minute() as u8,
        };
        if !window.contains(time) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Materialize one due schedule: create a job per governed
/// repository (deduped against active jobs), then advance it.
async fn materialize_one(
    pool: &SqlitePool,
    schedule: &Schedule,
    now: DateTime<Utc>,
    report: &mut MaterializeReport,
) -> Result<(), RepoNyxError> {
    // Unparsable cadence/timezone or a stored run outside the current
    // window: no job this cycle. An out-of-window run is advanced to
    // the next valid run; an unparsable schedule is left alone and
    // reported as an error by the caller.
    if !stored_run_in_window(schedule)? {
        advance_schedule(pool, schedule, now).await?;
        report.skipped_out_of_window += 1;
        return Ok(());
    }

    let repositories: Vec<Repository> = match schedule.scope {
        ScheduleScope::Repository => {
            let id = schedule.repository_id.ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "repository schedule has no repository_id",
                )
            })?;
            match get_repository(pool, id).await? {
                Some(repository) => vec![repository],
                None => {
                    report.skipped_missing_repository += 1;
                    advance_schedule(pool, schedule, now).await?;
                    return Ok(());
                }
            }
        }
        ScheduleScope::Account => {
            let account_id = schedule.account_id.ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "account schedule has no account_id",
                )
            })?;
            list_repositories(
                pool,
                RepositoryFilter {
                    account_id: Some(account_id),
                    ..Default::default()
                },
            )
            .await?
        }
        ScheduleScope::Global => list_repositories(pool, RepositoryFilter::default()).await?,
    };

    for repository in repositories {
        if !repository.sync_enabled {
            report.skipped_sync_disabled += 1;
            continue;
        }
        if has_active_job(pool, repository.id).await? {
            report.skipped_duplicate += 1;
            continue;
        }
        let job = create_job(
            pool,
            NewJob {
                repository_id: repository.id,
                job_type: job_type_for(&repository),
                priority: 0,
            },
        )
        .await?;
        report.created.push(job);
    }

    advance_schedule(pool, schedule, now).await?;
    Ok(())
}

/// Create jobs for every due schedule as of `now`, advancing each
/// schedule to its next run. Per-schedule failures are recorded in
/// the report and do not abort the run.
///
/// Before the due pass, enabled schedules whose `next_run_at` is
/// `NULL` (never computed — e.g. created before this crate existed)
/// get an initial value computed from `now` (`primed`); no job is
/// created for them.
pub async fn materialize_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<MaterializeReport, RepoNyxError> {
    let mut report = MaterializeReport::default();

    for schedule in list_schedules(pool).await? {
        if schedule.enabled && schedule.next_run_at.is_none() {
            match next_run(&schedule, now) {
                Ok(Some(run)) => {
                    update_schedule(
                        pool,
                        schedule.id,
                        SchedulePatch {
                            next_run_at: Some(Some(run)),
                            ..Default::default()
                        },
                    )
                    .await?;
                    report.primed += 1;
                }
                Ok(None) => {}
                Err(e) => report.errors.push(ScheduleMaterializeError {
                    schedule_id: schedule.id,
                    message: e.message.clone(),
                }),
            }
        }
    }

    let due = due_schedules(pool, now).await?;
    report.checked = due.len();
    for schedule in &due {
        if let Err(e) = materialize_one(pool, schedule, now, &mut report).await {
            report.errors.push(ScheduleMaterializeError {
                schedule_id: schedule.id,
                message: e.message.clone(),
            });
        }
    }
    Ok(report)
}

/// The effective schedule governing a repository (repository >
/// account > global, resolved by Phase 2) with its next `count` runs
/// after `from`. `None` when no schedule governs the repository. An
/// unknown repository id is `InvalidInput` (from Phase 2).
pub async fn effective_next_runs(
    pool: &SqlitePool,
    repository_id: Uuid,
    from: DateTime<Utc>,
    count: usize,
) -> Result<Option<EffectiveNextRuns>, RepoNyxError> {
    let Some(schedule) = effective_schedule_for(pool, repository_id).await? else {
        return Ok(None);
    };
    let runs = next_runs(&schedule, from, count)?;
    Ok(Some(EffectiveNextRuns { schedule, runs }))
}
