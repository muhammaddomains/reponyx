//! Timezone parsing and display. All stored times are UTC; a
//! schedule's `timezone` is an IANA name, resolved through
//! `chrono-tz` (never hand-rolled DST tables).

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::str::FromStr;

/// Parse an IANA timezone name (`UTC`, `America/Phoenix`,
/// `Europe/London`, ...). An unknown name is `InvalidInput` — a
/// schedule with an unparseable timezone has no next run.
pub fn parse_timezone(name: &str) -> Result<Tz, RepoNyxError> {
    Tz::from_str(name.trim()).map_err(|_| {
        RepoNyxError::new(
            ErrorCode::InvalidInput,
            format!("unknown timezone: {name:?}"),
        )
    })
}

/// Format a UTC instant as local time in the given timezone, e.g.
/// `2026-11-19 02:30:00 MST`. Used for display only — storage stays
/// UTC.
pub fn display_time(utc: DateTime<Utc>, timezone: &str) -> Result<String, RepoNyxError> {
    let tz = parse_timezone(timezone)?;
    Ok(utc
        .with_timezone(&tz)
        .format("%Y-%m-%d %H:%M:%S %Z")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_and_named_zones_parse() {
        assert_eq!(parse_timezone("UTC").unwrap(), Tz::UTC);
        assert_eq!(
            parse_timezone("America/Phoenix").unwrap(),
            Tz::America__Phoenix
        );
        assert_eq!(
            parse_timezone("America/New_York").unwrap(),
            Tz::America__New_York
        );
    }

    #[test]
    fn unknown_zone_is_invalid_input() {
        let err = parse_timezone("Mars/Olympus").unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(parse_timezone("").is_err());
        assert!(parse_timezone("  ").is_err());
    }

    #[test]
    fn display_uses_the_named_zone() {
        use chrono::TimeZone;
        let utc = Utc.with_ymd_and_hms(2026, 11, 19, 10, 26, 40).unwrap();
        assert_eq!(display_time(utc, "UTC").unwrap(), "2026-11-19 10:26:40 UTC");
        // America/Phoenix is UTC-7 year-round (no DST).
        assert_eq!(
            display_time(utc, "America/Phoenix").unwrap(),
            "2026-11-19 03:26:40 MST"
        );
    }
}
