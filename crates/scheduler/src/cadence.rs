//! Cadence parsing: how a [`Schedule`](reponyx_core::Schedule)'s
//! `interval_value` is read.
//!
//! `interval_value` is the cadence detail. Two forms are accepted for
//! every interval type; the JSON form is the full expressiveness, the
//! plain form is the human-friendly shorthand the Phase 2 domain docs
//! describe:
//!
//! | `interval_type` | Plain form | JSON form (value starts with `{`) |
//! |---|---|---|
//! | `interval_hours` | `"6"` — integer hours | `{"hours": 6}` |
//! | `daily` | `"02:30"` — `HH:MM` | `{"time": "02:30"}` |
//! | `weekly` | `"1,3,5 02:30"` — ISO weekday numbers then `HH:MM` | `{"time": "02:30", "weekdays": [1, 3, 5]}` |
//! | `custom` | `"90"` — integer minutes | `{"minutes": 90}` |
//!
//! The JSON form additionally accepts:
//! - `"weekdays": [1, 2, 3, 4, 5]` — ISO weekday numbers, 1 = Monday
//!   .. 7 = Sunday. Applies to `daily` (only those days), `weekly`
//!   (required), and the interval types (only those days).
//! - `"window": {"start": "01:00", "end": "05:00"}` — a local-time
//!   window; a schedule only fires inside it. `end <= start` means the
//!   window crosses midnight (e.g. `22:00`–`02:00`); `start == end` is
//!   an empty window and is rejected.
//!
//! Bounds: hours are 1..=168 (one week), minutes 1..=10080 (one
//! week). An invalid value is [`ErrorCode::InvalidInput`], never a
//! panic and never a silently defaulted cadence — a schedule whose
//! cadence cannot be parsed has no next run.

use reponyx_core::{ErrorCode, IntervalType, RepoNyxError};

fn invalid(message: impl Into<String>) -> RepoNyxError {
    RepoNyxError::new(ErrorCode::InvalidInput, message)
}

/// A wall-clock time of day, `HH:MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

impl TimeOfDay {
    /// Parse `HH:MM` (one or two digits for the hour, two for the
    /// minute). Hours are 0..=23, minutes 0..=59.
    pub fn parse(value: &str) -> Result<Self, RepoNyxError> {
        let (hour, minute) = value
            .split_once(':')
            .ok_or_else(|| invalid(format!("time {value:?} must be HH:MM")))?;
        if minute.len() != 2 || hour.is_empty() || hour.len() > 2 {
            return Err(invalid(format!("time {value:?} must be HH:MM")));
        }
        let hour: u8 = hour
            .parse()
            .map_err(|_| invalid(format!("time {value:?} has an invalid hour")))?;
        let minute: u8 = minute
            .parse()
            .map_err(|_| invalid(format!("time {value:?} has an invalid minute")))?;
        if hour > 23 {
            return Err(invalid(format!("time {value:?}: hour must be 0-23")));
        }
        if minute > 59 {
            return Err(invalid(format!("time {value:?}: minute must be 0-59")));
        }
        Ok(Self { hour, minute })
    }
}

impl std::fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

/// A local-time window `[start, end)`. When `end <= start` the window
/// crosses midnight (e.g. `22:00`–`02:00`); `start == end` is an empty
/// window and is rejected at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    pub start: TimeOfDay,
    pub end: TimeOfDay,
}

impl TimeWindow {
    /// `true` when the given time of day falls inside the window.
    pub fn contains(&self, time: TimeOfDay) -> bool {
        if self.start < self.end {
            time >= self.start && time < self.end
        } else {
            // Crosses midnight: [start, 24:00) ∪ [00:00, end).
            time >= self.start || time < self.end
        }
    }
}

/// What kind of cadence a schedule has, after parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadenceKind {
    /// Every N hours, anchored on the schedule's `next_run_at`
    /// (or `created_at` when that is unset).
    EveryHours(u32),
    /// Once a day at the given local time.
    Daily(TimeOfDay),
    /// On the given weekdays at the given local time.
    Weekly(TimeOfDay),
    /// Every N minutes, anchored like `EveryHours`.
    EveryMinutes(u32),
}

/// A parsed schedule cadence: the kind plus the optional weekday
/// filter and time window that further restrict it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cadence {
    pub kind: CadenceKind,
    /// ISO weekday numbers (1 = Monday .. 7 = Sunday), sorted and
    /// deduplicated. `None` means every day.
    pub weekdays: Option<Vec<u8>>,
    /// Local-time window the run must fall inside. `None` means any
    /// time of day.
    pub window: Option<TimeWindow>,
}

fn parse_weekdays(value: &[u8]) -> Result<Vec<u8>, RepoNyxError> {
    let mut days: Vec<u8> = Vec::with_capacity(value.len());
    for day in value {
        if !(1..=7).contains(day) {
            return Err(invalid(format!(
                "weekday {day} is out of range (ISO 1 = Monday .. 7 = Sunday)"
            )));
        }
        if !days.contains(day) {
            days.push(*day);
        }
    }
    if days.is_empty() {
        return Err(invalid("weekdays must not be empty"));
    }
    days.sort_unstable();
    Ok(days)
}

fn parse_json_weekdays(value: &serde_json::Value) -> Result<Option<Vec<u8>>, RepoNyxError> {
    let Some(days) = value.get("weekdays") else {
        return Ok(None);
    };
    let Some(days) = days.as_array() else {
        return Err(invalid(
            "\"weekdays\" must be an array of ISO weekday numbers",
        ));
    };
    let mut raw = Vec::with_capacity(days.len());
    for day in days {
        let Some(day) = day.as_u64() else {
            return Err(invalid(
                "\"weekdays\" must contain only ISO weekday numbers (1 = Monday .. 7 = Sunday)",
            ));
        };
        if day > u8::MAX as u64 {
            return Err(invalid(format!("weekday {day} is out of range")));
        }
        raw.push(day as u8);
    }
    Ok(Some(parse_weekdays(&raw)?))
}

fn parse_json_window(value: &serde_json::Value) -> Result<Option<TimeWindow>, RepoNyxError> {
    let Some(window) = value.get("window") else {
        return Ok(None);
    };
    let Some(start) = window.get("start").and_then(|v| v.as_str()) else {
        return Err(invalid("\"window\" needs a \"start\" time (HH:MM)"));
    };
    let Some(end) = window.get("end").and_then(|v| v.as_str()) else {
        return Err(invalid("\"window\" needs an \"end\" time (HH:MM)"));
    };
    let window = TimeWindow {
        start: TimeOfDay::parse(start)?,
        end: TimeOfDay::parse(end)?,
    };
    if window.start == window.end {
        return Err(invalid(
            "\"window\" start and end are equal — the window would be empty",
        ));
    }
    Ok(Some(window))
}

fn json_u64(value: &serde_json::Value, field: &str) -> Result<u64, RepoNyxError> {
    value
        .get(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| invalid(format!("{field:?} must be a positive integer")))
}

fn json_time(value: &serde_json::Value) -> Result<TimeOfDay, RepoNyxError> {
    let Some(time) = value.get("time").and_then(|v| v.as_str()) else {
        return Err(invalid("\"time\" must be HH:MM"));
    };
    TimeOfDay::parse(time)
}

fn parse_json_cadence(interval_type: IntervalType, value: &str) -> Result<Cadence, RepoNyxError> {
    let parsed: serde_json::Value = serde_json::from_str(value)
        .map_err(|e| invalid(format!("interval_value is not valid JSON: {e}")))?;
    if !parsed.is_object() {
        return Err(invalid("interval_value JSON must be an object"));
    };
    let weekdays = parse_json_weekdays(&parsed)?;
    let window = parse_json_window(&parsed)?;
    let kind = match interval_type {
        IntervalType::IntervalHours => {
            let hours = json_u64(&parsed, "hours")?;
            if hours == 0 || hours > 168 {
                return Err(invalid("\"hours\" must be 1..=168"));
            }
            CadenceKind::EveryHours(hours as u32)
        }
        IntervalType::Daily => CadenceKind::Daily(json_time(&parsed)?),
        IntervalType::Weekly => CadenceKind::Weekly(json_time(&parsed)?),
        IntervalType::Custom => {
            let minutes = json_u64(&parsed, "minutes")?;
            if minutes == 0 || minutes > 10_080 {
                return Err(invalid("\"minutes\" must be 1..=10080"));
            }
            CadenceKind::EveryMinutes(minutes as u32)
        }
    };
    let cadence = Cadence {
        kind,
        weekdays,
        window,
    };
    if matches!(kind, CadenceKind::Weekly(_)) && cadence.weekdays.is_none() {
        return Err(invalid(
            "a weekly schedule needs \"weekdays\" (ISO 1 = Monday .. 7 = Sunday)",
        ));
    }
    Ok(cadence)
}

fn parse_plain_cadence(interval_type: IntervalType, value: &str) -> Result<Cadence, RepoNyxError> {
    let value = value.trim();
    let cadence = match interval_type {
        IntervalType::IntervalHours => {
            let hours: u32 = value
                .parse()
                .map_err(|_| invalid(format!("interval_value {value:?} must be integer hours")))?;
            if hours == 0 || hours > 168 {
                return Err(invalid("hours must be 1..=168"));
            }
            Cadence {
                kind: CadenceKind::EveryHours(hours),
                weekdays: None,
                window: None,
            }
        }
        IntervalType::Daily => Cadence {
            kind: CadenceKind::Daily(TimeOfDay::parse(value)?),
            weekdays: None,
            window: None,
        },
        IntervalType::Weekly => {
            let (days, time) = value.split_once(char::is_whitespace).ok_or_else(|| {
                invalid(format!(
                    "weekly interval_value {value:?} must be \"<weekdays> HH:MM\", e.g. \"1,3,5 02:30\""
                ))
            })?;
            let mut raw = Vec::new();
            for day in days.split(',') {
                let day: u8 = day.trim().parse().map_err(|_| {
                    invalid(format!(
                        "weekly weekdays {days:?} must be ISO numbers (1 = Monday .. 7 = Sunday)"
                    ))
                })?;
                raw.push(day);
            }
            Cadence {
                kind: CadenceKind::Weekly(TimeOfDay::parse(time.trim())?),
                weekdays: Some(parse_weekdays(&raw)?),
                window: None,
            }
        }
        IntervalType::Custom => {
            let minutes: u32 = value.parse().map_err(|_| {
                invalid(format!("interval_value {value:?} must be integer minutes"))
            })?;
            if minutes == 0 || minutes > 10_080 {
                return Err(invalid("minutes must be 1..=10080"));
            }
            Cadence {
                kind: CadenceKind::EveryMinutes(minutes),
                weekdays: None,
                window: None,
            }
        }
    };
    Ok(cadence)
}

/// Parse a schedule's `interval_value` into a [`Cadence`]. A missing
/// value, an unparsable value, or a value that violates the bounds is
/// `InvalidInput`.
pub fn parse_cadence(
    interval_type: IntervalType,
    interval_value: Option<&str>,
) -> Result<Cadence, RepoNyxError> {
    let Some(value) = interval_value else {
        return Err(invalid("schedule has no interval_value"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("schedule interval_value must not be empty"));
    }
    if value.starts_with('{') {
        parse_json_cadence(interval_type, value)
    } else {
        parse_plain_cadence(interval_type, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_interval_hours() {
        let cadence = parse_cadence(IntervalType::IntervalHours, Some("6")).unwrap();
        assert_eq!(cadence.kind, CadenceKind::EveryHours(6));
        assert_eq!(cadence.weekdays, None);
        assert_eq!(cadence.window, None);
    }

    #[test]
    fn plain_daily_and_weekly() {
        let daily = parse_cadence(IntervalType::Daily, Some("02:30")).unwrap();
        assert_eq!(
            daily.kind,
            CadenceKind::Daily(TimeOfDay {
                hour: 2,
                minute: 30
            })
        );
        let weekly = parse_cadence(IntervalType::Weekly, Some("1,3,5 02:30")).unwrap();
        assert_eq!(
            weekly.kind,
            CadenceKind::Weekly(TimeOfDay {
                hour: 2,
                minute: 30
            })
        );
        assert_eq!(weekly.weekdays, Some(vec![1, 3, 5]));
    }

    #[test]
    fn json_form_with_weekdays_and_window() {
        let cadence = parse_cadence(
            IntervalType::Daily,
            Some(r#"{"time": "02:30", "weekdays": [1, 2, 3, 4, 5], "window": {"start": "01:00", "end": "05:00"}}"#),
        )
        .unwrap();
        assert_eq!(
            cadence.kind,
            CadenceKind::Daily(TimeOfDay {
                hour: 2,
                minute: 30
            })
        );
        assert_eq!(cadence.weekdays, Some(vec![1, 2, 3, 4, 5]));
        assert_eq!(
            cadence.window,
            Some(TimeWindow {
                start: TimeOfDay { hour: 1, minute: 0 },
                end: TimeOfDay { hour: 5, minute: 0 },
            })
        );
    }

    #[test]
    fn invalid_values_are_invalid_input() {
        for (kind, value) in [
            (IntervalType::IntervalHours, "0"),
            (IntervalType::IntervalHours, "169"),
            (IntervalType::IntervalHours, "six"),
            (IntervalType::Daily, "24:00"),
            (IntervalType::Daily, "2:60"),
            (IntervalType::Daily, "0230"),
            (IntervalType::Weekly, "02:30"),
            (IntervalType::Weekly, "0 02:30"),
            (IntervalType::Weekly, "8 02:30"),
            (IntervalType::Custom, "0"),
            (IntervalType::Custom, "10081"),
        ] {
            let err = parse_cadence(kind, Some(value)).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "{kind:?} {value:?}");
        }
        assert!(parse_cadence(IntervalType::Daily, None).is_err());
        assert!(parse_cadence(IntervalType::Daily, Some("  ")).is_err());
        assert!(
            parse_cadence(
                IntervalType::Daily,
                Some(r#"{"time": "02:30", "window": {"start": "01:00", "end": "01:00"}}"#)
            )
            .is_err()
        );
        assert!(parse_cadence(IntervalType::Weekly, Some(r#"{"time": "02:30"}"#)).is_err());
    }

    #[test]
    fn window_contains_midnight_crossing() {
        let window = TimeWindow {
            start: TimeOfDay {
                hour: 22,
                minute: 0,
            },
            end: TimeOfDay { hour: 2, minute: 0 },
        };
        assert!(window.contains(TimeOfDay {
            hour: 23,
            minute: 0
        }));
        assert!(window.contains(TimeOfDay {
            hour: 1,
            minute: 59
        }));
        assert!(!window.contains(TimeOfDay { hour: 2, minute: 0 }));
        assert!(!window.contains(TimeOfDay {
            hour: 12,
            minute: 0
        }));
        let day = TimeWindow {
            start: TimeOfDay { hour: 1, minute: 0 },
            end: TimeOfDay { hour: 5, minute: 0 },
        };
        assert!(day.contains(TimeOfDay { hour: 1, minute: 0 }));
        assert!(!day.contains(TimeOfDay { hour: 5, minute: 0 }));
        assert!(!day.contains(TimeOfDay {
            hour: 0,
            minute: 59
        }));
    }
}
