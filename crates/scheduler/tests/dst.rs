//! DST tests: spring-forward gaps and fall-back ambiguity.
//!
//! Rules under test (see `crates/scheduler/src/next_run.rs`):
//! - Gap (scheduled local time does not exist): fire at the first
//!   valid local time after the gap.
//! - Ambiguous (scheduled local time occurs twice): fire once, at
//!   the first occurrence.
//!
//! America/Phoenix has no DST and is the control: the same local
//! time always exists exactly once.

#[path = "common.rs"]
mod common;

use common::{schedule_struct, utc};
use reponyx_core::IntervalType;
use reponyx_scheduler::{next_run, next_runs};

fn daily(time: &str, timezone: &str) -> reponyx_core::Schedule {
    schedule_struct(
        IntervalType::Daily,
        Some(time),
        timezone,
        true,
        None,
        utc(2026, 1, 1, 0, 0),
    )
}

#[test]
fn new_york_spring_forward_gap_fires_after_the_gap() {
    // DST starts Sunday 2026-03-08: 02:00 EST jumps to 03:00 EDT.
    // 02:30 that day does not exist; the first valid time after the
    // gap is 03:00 EDT = 07:00 UTC.
    let schedule = daily("02:30", "America/New_York");
    assert_eq!(
        next_run(&schedule, utc(2026, 3, 7, 12, 0)).unwrap(),
        Some(utc(2026, 3, 8, 7, 0))
    );
    // The day before is normal: 02:30 EST = 07:30 UTC.
    assert_eq!(
        next_run(&schedule, utc(2026, 3, 6, 12, 0)).unwrap(),
        Some(utc(2026, 3, 7, 7, 30))
    );
    // And the day after is normal EDT: 02:30 EDT = 06:30 UTC.
    assert_eq!(
        next_run(&schedule, utc(2026, 3, 8, 12, 0)).unwrap(),
        Some(utc(2026, 3, 9, 6, 30))
    );
}

#[test]
fn new_york_fall_back_ambiguity_fires_at_first_occurrence() {
    // DST ends Sunday 2026-11-01: 02:00 EDT falls back to 01:00 EST.
    // 01:30 occurs twice; the first is 01:30 EDT = 05:30 UTC.
    let schedule = daily("01:30", "America/New_York");
    assert_eq!(
        next_run(&schedule, utc(2026, 10, 31, 12, 0)).unwrap(),
        Some(utc(2026, 11, 1, 5, 30))
    );
    // The next run is the following day, now on standard time:
    // 01:30 EST = 06:30 UTC. The schedule fires once per day, never
    // twice for the ambiguous hour.
    assert_eq!(
        next_runs(&schedule, utc(2026, 10, 31, 12, 0), 2).unwrap(),
        vec![utc(2026, 11, 1, 5, 30), utc(2026, 11, 2, 6, 30)]
    );
}

#[test]
fn london_spring_forward_gap() {
    // BST starts Sunday 2026-03-29: 01:00 GMT jumps to 02:00 BST.
    // 01:30 that day does not exist; the first valid time after the
    // gap is 02:00 BST = 01:00 UTC.
    let schedule = daily("01:30", "Europe/London");
    assert_eq!(
        next_run(&schedule, utc(2026, 3, 28, 12, 0)).unwrap(),
        Some(utc(2026, 3, 29, 1, 0))
    );
}

#[test]
fn london_fall_back_ambiguity() {
    // BST ends Sunday 2026-10-25: 02:00 BST falls back to 01:00 GMT.
    // 01:30 occurs twice; the first is 01:30 BST = 00:30 UTC.
    let schedule = daily("01:30", "Europe/London");
    assert_eq!(
        next_run(&schedule, utc(2026, 10, 24, 12, 0)).unwrap(),
        Some(utc(2026, 10, 25, 0, 30))
    );
}

#[test]
fn phoenix_has_no_dst_control() {
    // America/Phoenix is UTC-7 year-round: 02:30 always exists and
    // is always 09:30 UTC, including on US transition days.
    let schedule = daily("02:30", "America/Phoenix");
    assert_eq!(
        next_run(&schedule, utc(2026, 3, 7, 12, 0)).unwrap(),
        Some(utc(2026, 3, 8, 9, 30))
    );
    assert_eq!(
        next_run(&schedule, utc(2026, 10, 31, 12, 0)).unwrap(),
        Some(utc(2026, 11, 1, 9, 30))
    );
}
