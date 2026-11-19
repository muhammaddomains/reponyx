//! Progress reporting for long Git operations (clone / fetch).
//!
//! Git writes `--progress` output to stderr as carriage-return separated
//! segments such as `Receiving objects:  45% (9/20), 12.34 KiB`. This
//! module parses those segments into structured events. Only numbers
//! Git itself emits are reported — percentages are never invented, and
//! lines that match no known pattern are passed through as `Other`
//! instead of being dropped.

use std::fmt;

/// One progress update from a running Git operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// `Counting objects: 100% (10/10), done.`
    CountingObjects { received: u64, total: Option<u64> },
    /// `Compressing objects: 100% (8/8), done.`
    CompressingObjects { percent: u8 },
    /// `Receiving objects:  45% (9/20), 12.34 KiB | 6.17 KiB/s`
    ReceivingObjects {
        received: u64,
        total: u64,
        percent: u8,
        /// Bytes received so far, when Git reports a size.
        bytes: Option<u64>,
    },
    /// `Resolving deltas: 100% (5/5), done.`
    ResolvingDeltas { percent: u8 },
    /// `Checking out files: 100% (3/3), done.`
    CheckingOut { percent: u8 },
    /// `Updating files: 100% (3/3), done.`
    UpdatingFiles { percent: u8 },
    /// A progress line that matched no known pattern, verbatim.
    Other(String),
}

impl fmt::Display for ProgressEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountingObjects { received, total } => match total {
                Some(t) => write!(f, "Counting objects: {received}/{t}"),
                None => write!(f, "Counting objects: {received}"),
            },
            Self::CompressingObjects { percent } => {
                write!(f, "Compressing objects: {percent}%")
            }
            Self::ReceivingObjects {
                received,
                total,
                percent,
                bytes,
            } => match bytes {
                Some(b) => write!(
                    f,
                    "Receiving objects: {percent}% ({received}/{total}), {b} bytes"
                ),
                None => write!(f, "Receiving objects: {percent}% ({received}/{total})"),
            },
            Self::ResolvingDeltas { percent } => write!(f, "Resolving deltas: {percent}%"),
            Self::CheckingOut { percent } => write!(f, "Checking out files: {percent}%"),
            Self::UpdatingFiles { percent } => write!(f, "Updating files: {percent}%"),
            Self::Other(line) => write!(f, "{line}"),
        }
    }
}

/// Parse one `--progress` segment into a [`ProgressEvent`].
/// Returns `None` for blank input; every other line yields an event
/// (known patterns parse to their variant, the rest to `Other`).
pub fn parse_progress_line(line: &str) -> Option<ProgressEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Progress from the remote is forwarded with a `remote: ` prefix.
    let body = trimmed.strip_prefix("remote: ").unwrap_or(trimmed);
    if let Some(rest) = body.strip_prefix("Counting objects:") {
        let (received, total) = parse_count_pair(rest);
        return Some(ProgressEvent::CountingObjects { received, total });
    }
    if let Some(rest) = body.strip_prefix("Compressing objects:") {
        return parse_percent(rest).map(|percent| ProgressEvent::CompressingObjects { percent });
    }
    if let Some(rest) = body.strip_prefix("Receiving objects:") {
        let (received, total) = parse_count_pair(rest);
        if let (Some(percent), Some(total)) = (parse_percent(rest), total) {
            return Some(ProgressEvent::ReceivingObjects {
                received,
                total,
                percent,
                bytes: parse_size(rest),
            });
        }
        // A "Receiving objects" line without counts is still progress.
        return Some(ProgressEvent::Other(trimmed.to_string()));
    }
    if let Some(rest) = body.strip_prefix("Resolving deltas:") {
        return parse_percent(rest).map(|percent| ProgressEvent::ResolvingDeltas { percent });
    }
    if let Some(rest) = body.strip_prefix("Checking out files:") {
        return parse_percent(rest).map(|percent| ProgressEvent::CheckingOut { percent });
    }
    if let Some(rest) = body.strip_prefix("Updating files:") {
        return parse_percent(rest).map(|percent| ProgressEvent::UpdatingFiles { percent });
    }
    Some(ProgressEvent::Other(trimmed.to_string()))
}

/// Parse the `N%` at the start of a progress segment, e.g. ` 45% (9/20)`.
fn parse_percent(segment: &str) -> Option<u8> {
    let pct = segment.find('%')?;
    let digits: String = segment[..pct]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Parse the `(received/total)` pair of a progress segment.
/// Returns `(received, Some(total))`, or `(0, None)` when absent.
/// `Counting objects: 10, done.` (no pair) yields `(10, None)` via the
/// leading number.
fn parse_count_pair(segment: &str) -> (u64, Option<u64>) {
    if let (Some(open), Some(close)) = (segment.find('('), segment.find(')')) {
        let inner = &segment[open + 1..close];
        let mut parts = inner.split('/');
        if let (Some(a), Some(b)) = (parts.next(), parts.next())
            && let (Ok(received), Ok(total)) = (a.trim().parse::<u64>(), b.trim().parse::<u64>())
        {
            return (received, Some(total));
        }
    }
    // Fallback: a bare leading number, e.g. `Enumerating objects: 10`.
    let first: String = segment
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (first.parse().unwrap_or(0), None)
}

/// Parse the `12.34 KiB` size after the count pair, e.g.
/// `Receiving objects:  45% (9/20), 12.34 KiB | 6.17 KiB/s`.
/// Returns bytes, or `None` when no size is present.
fn parse_size(segment: &str) -> Option<u64> {
    let after_pair = segment.split(')').nth(1)?;
    let size_text = after_pair.split('|').next()?.trim().trim_start_matches(',');
    let size_text = size_text.trim();
    let mut parts = size_text.split_whitespace();
    let number: f64 = parts.next()?.parse().ok()?;
    let unit = parts.next().unwrap_or("B");
    let multiplier: f64 = match unit {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_receiving_objects_with_size() {
        assert_eq!(
            parse_progress_line("Receiving objects:  45% (9/20), 12.34 KiB | 6.17 KiB/s"),
            Some(ProgressEvent::ReceivingObjects {
                received: 9,
                total: 20,
                percent: 45,
                bytes: Some(12_636),
            })
        );
    }

    #[test]
    fn parses_receiving_objects_done() {
        assert_eq!(
            parse_progress_line("Receiving objects: 100% (20/20), 24.68 KiB | 12.34 KiB/s, done."),
            Some(ProgressEvent::ReceivingObjects {
                received: 20,
                total: 20,
                percent: 100,
                bytes: Some(25_272),
            })
        );
    }

    #[test]
    fn parses_counting_and_compressing() {
        assert_eq!(
            parse_progress_line("remote: Counting objects: 100% (10/10), done."),
            Some(ProgressEvent::CountingObjects {
                received: 10,
                total: Some(10),
            })
        );
        assert_eq!(
            parse_progress_line("remote: Compressing objects: 100% (8/8), done."),
            Some(ProgressEvent::CompressingObjects { percent: 100 })
        );
    }

    #[test]
    fn parses_resolving_and_checkout() {
        assert_eq!(
            parse_progress_line("Resolving deltas: 100% (5/5), done."),
            Some(ProgressEvent::ResolvingDeltas { percent: 100 })
        );
        assert_eq!(
            parse_progress_line("Checking out files:  50% (3/6), done."),
            Some(ProgressEvent::CheckingOut { percent: 50 })
        );
        assert_eq!(
            parse_progress_line("Updating files: 100% (3/3), done."),
            Some(ProgressEvent::UpdatingFiles { percent: 100 })
        );
    }

    #[test]
    fn unknown_lines_pass_through_and_blanks_are_dropped() {
        assert_eq!(
            parse_progress_line("remote: Enumerating objects: 10, done."),
            Some(ProgressEvent::Other(
                "remote: Enumerating objects: 10, done.".to_string()
            ))
        );
        assert_eq!(parse_progress_line("   "), None);
        assert_eq!(parse_progress_line(""), None);
    }

    #[test]
    fn size_units_parse() {
        assert_eq!(parse_size(" 100% (1/1), 512 B | 1.00 KiB/s"), Some(512));
        assert_eq!(
            parse_size(" 100% (1/1), 2.50 MiB | 1.00 MiB/s"),
            Some(2_621_440)
        );
        assert_eq!(parse_size(" 100% (1/1)"), None);
    }
}
