//! Next-run math tests (pure functions, no database).

#[path = "common.rs"]
mod common;

use common::{schedule_struct, utc};
use reponyx_core::{ErrorCode, IntervalType};
use reponyx_scheduler::{next_run, next_runs};

#[test]
fn interval_hours_from_next_run_anchor() {
    // Anchor (stored next_run_at) 04:00, every 6 h, from 10:00.
    let schedule = schedule_struct(
        IntervalType::IntervalHours,
        Some("6"),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 4, 0)),
        utc(2026, 11, 19, 4, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 19, 16, 0))
    );
    // Strictly after: exactly on a run time moves to the next one.
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 16, 0)).unwrap(),
        Some(utc(2026, 11, 19, 22, 0))
    );
    // Anchor in the future is itself the next run.
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 3, 0)).unwrap(),
        Some(utc(2026, 11, 19, 4, 0))
    );
}

#[test]
fn interval_hours_without_next_run_anchors_on_created_at() {
    let schedule = schedule_struct(
        IntervalType::IntervalHours,
        Some("2"),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 11, 0)).unwrap(),
        Some(utc(2026, 11, 19, 12, 0))
    );
}

#[test]
fn custom_interval_is_minutes() {
    let schedule = schedule_struct(
        IntervalType::Custom,
        Some("90"),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 10, 0)),
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 30)).unwrap(),
        Some(utc(2026, 11, 19, 11, 30))
    );
}

#[test]
fn daily_at_fixed_time() {
    let schedule = schedule_struct(
        IntervalType::Daily,
        Some("02:30"),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 20, 2, 30))
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 1, 0)).unwrap(),
        Some(utc(2026, 11, 19, 2, 30))
    );
    // Exactly at the run time: next day.
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 2, 30)).unwrap(),
        Some(utc(2026, 11, 20, 2, 30))
    );
}

#[test]
fn daily_with_weekdays_skips_weekends() {
    // 2026-11-20 is a Friday; weekdays Mon-Fri.
    let schedule = schedule_struct(
        IntervalType::Daily,
        Some(r#"{"time": "02:30", "weekdays": [1, 2, 3, 4, 5]}"#),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 20, 10, 0)).unwrap(),
        Some(utc(2026, 11, 23, 2, 30)) // Monday
    );
    // A weekday still fires the same week.
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 20, 2, 30)) // Friday
    );
}

#[test]
fn weekly_plain_form() {
    // Mondays and Wednesdays at 09:00; 2026-11-19 is a Thursday.
    let schedule = schedule_struct(
        IntervalType::Weekly,
        Some("1,3 09:00"),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 23, 9, 0)) // Monday
    );
}

#[test]
fn window_filters_interval_candidates() {
    // Every 2 h from 00:00, window 01:00-05:00.
    let schedule = schedule_struct(
        IntervalType::IntervalHours,
        Some(r#"{"hours": 2, "window": {"start": "01:00", "end": "05:00"}}"#),
        "UTC",
        true,
        Some(utc(2026, 11, 19, 0, 0)),
        utc(2026, 11, 19, 0, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 0, 30)).unwrap(),
        Some(utc(2026, 11, 19, 2, 0))
    );
    // After 04:00 the next in-window candidate is tomorrow 02:00.
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 4, 30)).unwrap(),
        Some(utc(2026, 11, 20, 2, 0))
    );
}

#[test]
fn window_boundaries_are_start_inclusive_end_exclusive() {
    // Daily 02:30, window starting exactly at the run time: fires.
    let fires = schedule_struct(
        IntervalType::Daily,
        Some(r#"{"time": "02:30", "window": {"start": "02:30", "end": "05:00"}}"#),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&fires, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 20, 2, 30))
    );
    // Window ending exactly at the run time: never fires.
    let never = schedule_struct(
        IntervalType::Daily,
        Some(r#"{"time": "02:30", "window": {"start": "01:00", "end": "02:30"}}"#),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(next_run(&never, utc(2026, 11, 19, 10, 0)).unwrap(), None);
}

#[test]
fn midnight_crossing_window() {
    let schedule = schedule_struct(
        IntervalType::Daily,
        Some(r#"{"time": "23:30", "window": {"start": "22:00", "end": "02:00"}}"#),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 19, 23, 30))
    );
    let early = schedule_struct(
        IntervalType::Daily,
        Some(r#"{"time": "01:30", "window": {"start": "22:00", "end": "02:00"}}"#),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&early, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 20, 1, 30))
    );
}

#[test]
fn daily_time_outside_window_never_fires() {
    // The scheduled time is outside its own window every day: the
    // schedule is misconfigured and has no next run (never a fake
    // time). The preview surfaces this as "no runs".
    let schedule = schedule_struct(
        IntervalType::Daily,
        Some(r#"{"time": "02:30", "window": {"start": "03:00", "end": "05:00"}}"#),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(), None);
}

#[test]
fn disabled_schedule_has_no_next_run() {
    let schedule = schedule_struct(
        IntervalType::IntervalHours,
        Some("6"),
        "UTC",
        false,
        Some(utc(2026, 11, 19, 16, 0)),
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(), None);
    assert_eq!(
        next_runs(&schedule, utc(2026, 11, 19, 10, 0), 3).unwrap(),
        Vec::<chrono::DateTime<chrono::Utc>>::new()
    );
}

#[test]
fn next_runs_returns_n_in_order() {
    let schedule = schedule_struct(
        IntervalType::Daily,
        Some("02:30"),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_runs(&schedule, utc(2026, 11, 19, 10, 0), 3).unwrap(),
        vec![
            utc(2026, 11, 20, 2, 30),
            utc(2026, 11, 21, 2, 30),
            utc(2026, 11, 22, 2, 30),
        ]
    );
}

#[test]
fn invalid_cadence_and_timezone_are_invalid_input() {
    let bad_cadence = schedule_struct(
        IntervalType::Daily,
        Some("not-a-time"),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    let err = next_run(&bad_cadence, utc(2026, 11, 19, 10, 0)).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);

    let bad_zone = schedule_struct(
        IntervalType::Daily,
        Some("02:30"),
        "Mars/Olympus",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    let err = next_run(&bad_zone, utc(2026, 11, 19, 10, 0)).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);

    let schedule = schedule_struct(
        IntervalType::Daily,
        Some("02:30"),
        "UTC",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert!(next_runs(&schedule, utc(2026, 11, 19, 10, 0), 0).is_err());
    assert!(next_runs(&schedule, utc(2026, 11, 19, 10, 0), 101).is_err());
}

#[test]
fn daily_in_a_named_timezone_converts_to_utc() {
    // 02:30 in America/Phoenix (UTC-7, no DST) is 09:30 UTC.
    let schedule = schedule_struct(
        IntervalType::Daily,
        Some("02:30"),
        "America/Phoenix",
        true,
        None,
        utc(2026, 11, 19, 10, 0),
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 11, 19, 10, 0)).unwrap(),
        Some(utc(2026, 11, 20, 9, 30))
    );
}
