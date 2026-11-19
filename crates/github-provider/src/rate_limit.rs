//! GitHub rate-limit state (`X-RateLimit-*` / `Retry-After`).
//!
//! Every API response carries `X-RateLimit-Limit`,
//! `X-RateLimit-Remaining` and `X-RateLimit-Reset` (epoch seconds when
//! the window resets). Secondary limits answer 429 (or 403) with a
//! `Retry-After` header. The client keeps the latest observed state
//! and exposes it so callers (and the UI, in a later phase) can show
//! it; the client itself sleeps only for short waits (see
//! [`RateLimitState::wait_duration`]).

use chrono::{DateTime, TimeZone, Utc};
use std::time::Duration;

/// The latest observed rate-limit state. All fields are `Option`:
/// a response without the headers leaves the state unknown rather
/// than inventing numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RateLimitState {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    /// When the current window resets, if the server said so.
    pub reset_at: Option<DateTime<Utc>>,
}

impl RateLimitState {
    /// Parse the rate-limit headers of one response. Headers that are
    /// absent or unparsable leave the corresponding field unchanged
    /// (a partial update), except `reset_at`, which is cleared only
    /// when a new reset value parses.
    pub fn update_from_headers(&mut self, headers: &reqwest::header::HeaderMap) {
        if let Some(limit) = parse_header_u64(headers, "x-ratelimit-limit") {
            self.limit = Some(limit);
        }
        if let Some(remaining) = parse_header_u64(headers, "x-ratelimit-remaining") {
            self.remaining = Some(remaining);
        }
        if let Some(reset) = parse_header_u64(headers, "x-ratelimit-reset") {
            self.reset_at = Utc.timestamp_opt(reset as i64, 0).single();
        }
    }

    /// How long to wait before retrying after a rate-limit response.
    /// Prefers `Retry-After` (seconds) when present; otherwise the
    /// time until `reset_at`. Returns `None` when neither is known —
    /// the caller then falls back to the client's default backoff.
    /// The result is never negative.
    pub fn wait_duration(&self, retry_after: Option<&str>) -> Option<Duration> {
        if let Some(seconds) = retry_after.and_then(|v| v.trim().parse::<u64>().ok()) {
            return Some(Duration::from_secs(seconds));
        }
        let reset_at = self.reset_at?;
        let now = Utc::now();
        if reset_at <= now {
            Some(Duration::from_secs(0))
        } else {
            Some(Duration::from_secs(
                (reset_at - now).num_seconds().max(0) as u64
            ))
        }
    }
}

fn parse_header_u64(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderMap;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                reqwest::header::HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn parses_rate_limit_headers() {
        let mut state = RateLimitState::default();
        let map = headers(&[
            ("x-ratelimit-limit", "5000"),
            ("x-ratelimit-remaining", "4999"),
            ("x-ratelimit-reset", "1800000000"),
        ]);
        state.update_from_headers(&map);
        assert_eq!(state.limit, Some(5000));
        assert_eq!(state.remaining, Some(4999));
        assert_eq!(state.reset_at, Utc.timestamp_opt(1_800_000_000, 0).single());
    }

    #[test]
    fn unparsable_headers_leave_state_alone() {
        let mut state = RateLimitState {
            limit: Some(60),
            remaining: Some(59),
            reset_at: None,
        };
        let map = headers(&[
            ("x-ratelimit-limit", "not-a-number"),
            ("x-ratelimit-remaining", ""),
        ]);
        state.update_from_headers(&map);
        assert_eq!(state.limit, Some(60));
        assert_eq!(state.remaining, Some(59));
    }

    #[test]
    fn wait_prefers_retry_after() {
        let state = RateLimitState {
            limit: Some(60),
            remaining: Some(0),
            reset_at: Some(Utc::now() + chrono::Duration::seconds(120)),
        };
        assert_eq!(state.wait_duration(Some("5")), Some(Duration::from_secs(5)));
    }

    #[test]
    fn wait_uses_reset_when_no_retry_after() {
        let state = RateLimitState {
            limit: Some(60),
            remaining: Some(0),
            reset_at: Some(Utc::now() + chrono::Duration::seconds(90)),
        };
        let wait = state.wait_duration(None).unwrap();
        assert!(wait.as_secs() >= 85 && wait.as_secs() <= 90);
    }

    #[test]
    fn wait_is_none_without_any_signal() {
        let state = RateLimitState::default();
        assert_eq!(state.wait_duration(None), None);
        assert_eq!(state.wait_duration(Some("garbage")), None);
    }
}
