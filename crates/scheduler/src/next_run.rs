//! Next-run calculation: the first run of a schedule strictly after a
//! given instant, in UTC.
//!
//! DST rules (documented here and in `lib.rs`):
//! - A scheduled local time that does not exist (spring-forward gap)
//!   fires at the **first valid local time after the gap**.
//! - A scheduled local time that occurs twice (fall-back ambiguity)
//!   fires once, at the **first occurrence** (the earlier UTC instant).
//! - Interval schedules (`interval_hours`, `custom`) are absolute-time
//!   cadences: every N hours/minutes from an anchor. DST does not shift
//!   them; only the weekday/window filters look at local time.
//!
//! A disabled schedule has no next run (`None`). An unparsable
//! cadence or timezone is `InvalidInput` — never a fake time.

use crate::cadence::{Cadence, CadenceKind, TimeOfDay, parse_cadence};
use crate::timezone::parse_timezone;
use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc,
};
use chrono_tz::Tz;
use reponyx_core::{ErrorCode, RepoNyxError, Schedule};

fn invalid(message: impl Into<String>) -> RepoNyxError {
    RepoNyxError::new(ErrorCode::InvalidInput, message)
}

/// Resolve a local naive datetime in a zone to UTC.
///
/// - `Single`: the one instant.
/// - `Ambiguous` (fall back): the first occurrence.
/// - `None` (spring-forward gap): the first valid local time after
///   the gap, found by scanning forward in 1-minute steps (a DST gap
///   is at most a few hours; the scan is capped at 4 hours).
fn resolve_local(tz: Tz, naive: NaiveDateTime) -> Result<DateTime<Utc>, RepoNyxError> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(earliest, _latest) => Ok(earliest.with_timezone(&Utc)),
        LocalResult::None => {
            let mut probe = naive + Duration::minutes(1);
            for _ in 0..(4 * 60) {
                match tz.from_local_datetime(&probe) {
                    LocalResult::Single(dt) => return Ok(dt.with_timezone(&Utc)),
                    LocalResult::Ambiguous(dt, _) => return Ok(dt.with_timezone(&Utc)),
                    LocalResult::None => probe += Duration::minutes(1),
                }
            }
            Err(RepoNyxError::new(
                ErrorCode::InternalError,
                format!("local time {naive} has no valid mapping in its timezone"),
            ))
        }
    }
}

fn weekday_allowed(cadence: &Cadence, weekday_iso: u8) -> bool {
    match &cadence.weekdays {
        Some(days) => days.contains(&weekday_iso),
        None => true,
    }
}

fn window_allowed(cadence: &Cadence, time: TimeOfDay) -> bool {
    match &cadence.window {
        Some(window) => window.contains(time),
        None => true,
    }
}

fn local_time_of_day(tz: Tz, instant: DateTime<Utc>) -> TimeOfDay {
    let local = instant.with_timezone(&tz);
    TimeOfDay {
        hour: local.hour() as u8,
        minute: local.minute() as u8,
    }
}

/// Next wall-clock run (daily / weekly): scan local dates from `from`
/// forward, resolving each candidate date + time through the DST
/// rules. The window check uses the *resolved* local time — a
/// gap-shifted run that lands outside the window does not fire that
/// day.
fn next_wall_clock_run(
    tz: Tz,
    cadence: &Cadence,
    time: TimeOfDay,
    from: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, RepoNyxError> {
    let mut date = from.with_timezone(&tz).date_naive();
    for _ in 0..366 {
        if weekday_allowed(cadence, date.weekday().number_from_monday() as u8) {
            let naive = date.and_time(
                NaiveTime::from_hms_opt(time.hour as u32, time.minute as u32, 0)
                    .expect("time of day validated at parse"),
            );
            let candidate = resolve_local(tz, naive)?;
            if candidate > from && window_allowed(cadence, local_time_of_day(tz, candidate)) {
                return Ok(Some(candidate));
            }
        }
        date += Duration::days(1);
    }
    Ok(None)
}

/// Next interval run (`interval_hours` / `custom`): candidates are
/// `anchor + k * step` in absolute time; the weekday/window filters
/// look at each candidate's local time.
fn next_interval_run(
    tz: Tz,
    cadence: &Cadence,
    step: Duration,
    anchor: DateTime<Utc>,
    from: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, RepoNyxError> {
    let mut candidate = if anchor > from {
        anchor
    } else {
        let elapsed = from - anchor;
        let steps = elapsed.num_seconds() / step.num_seconds() + 1;
        anchor + step * steps as i32
    };
    let horizon = from + Duration::days(366);
    for _ in 0..10_000 {
        if candidate > horizon {
            return Ok(None);
        }
        let local = candidate.with_timezone(&tz);
        if weekday_allowed(cadence, local.weekday().number_from_monday() as u8)
            && window_allowed(cadence, local_time_of_day(tz, candidate))
        {
            return Ok(Some(candidate));
        }
        candidate += step;
    }
    Ok(None)
}

/// The next run of a schedule strictly after `from`, in UTC.
/// `None` when the schedule is disabled or has no future run. An
/// unparsable cadence or timezone is `InvalidInput`.
pub fn next_run(
    schedule: &Schedule,
    from: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, RepoNyxError> {
    if !schedule.enabled {
        return Ok(None);
    }
    let tz = parse_timezone(&schedule.timezone)?;
    let cadence = parse_cadence(schedule.interval_type, schedule.interval_value.as_deref())?;
    match cadence.kind {
        CadenceKind::EveryHours(hours) => next_interval_run(
            tz,
            &cadence,
            Duration::hours(i64::from(hours)),
            schedule.next_run_at.unwrap_or(schedule.created_at),
            from,
        ),
        CadenceKind::EveryMinutes(minutes) => next_interval_run(
            tz,
            &cadence,
            Duration::minutes(i64::from(minutes)),
            schedule.next_run_at.unwrap_or(schedule.created_at),
            from,
        ),
        CadenceKind::Daily(time) | CadenceKind::Weekly(time) => {
            next_wall_clock_run(tz, &cadence, time, from)
        }
    }
}

/// The next `count` runs of a schedule after `from`, in UTC, in
/// order. Fewer than `count` are returned when the schedule has no
/// more runs (disabled, or no matching day). `count` must be 1..=100.
pub fn next_runs(
    schedule: &Schedule,
    from: DateTime<Utc>,
    count: usize,
) -> Result<Vec<DateTime<Utc>>, RepoNyxError> {
    if count == 0 {
        return Err(invalid("count must be at least 1"));
    }
    if count > 100 {
        return Err(invalid("count must be at most 100"));
    }
    let mut runs = Vec::with_capacity(count);
    let mut cursor = from;
    while runs.len() < count {
        match next_run(schedule, cursor)? {
            Some(run) => {
                runs.push(run);
                cursor = run;
            }
            None => break,
        }
    }
    Ok(runs)
}
