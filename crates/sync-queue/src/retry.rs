//! Failure classification and exponential backoff with jitter.
//!
//! Retry policy (Phase 6 plan):
//! - Transient failures → backoff + jitter, new job row (never
//!   `running` → `pending`).
//! - Permanent auth / permission / not-found errors → no further retry
//!   after the current attempt is recorded as failed.

use rand::Rng;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::time::Duration;

/// How the queue should treat a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Network, rate limit, lock, timeout, generic git transient.
    Transient,
    /// Auth, permissions, missing repo, corruption — do not retry.
    Permanent,
    /// Caller cancelled the job or the pool is shutting down.
    Cancelled,
}

/// Classify a [`RepoNyxError`] for retry decisions.
pub fn classify_error(err: &RepoNyxError) -> FailureClass {
    match err.code {
        ErrorCode::OperationCancelled => FailureClass::Cancelled,
        code if code.is_retryable() => FailureClass::Transient,
        // Disk full is retryable in spirit (free space then retry) but
        // limited; treat as transient so the budget applies.
        ErrorCode::DiskSpaceInsufficient => FailureClass::Transient,
        _ => FailureClass::Permanent,
    }
}

/// Whether another attempt should be scheduled after this attempt index
/// (1-based, matching `SyncJob.attempt_count` after claim).
pub fn should_retry(class: FailureClass, attempt_count: i32, max_attempts: u32) -> bool {
    match class {
        FailureClass::Transient => attempt_count < max_attempts as i32,
        FailureClass::Permanent | FailureClass::Cancelled => false,
    }
}

/// Exponential backoff with full jitter.
///
/// `delay = random(0 ..= min(max_backoff, base * 2^(attempt-1)))`
///
/// `attempt` is 1-based (first failure → attempt 1 → base window).
pub fn compute_backoff(
    attempt: u32,
    base: Duration,
    max: Duration,
    rng: &mut impl rand::Rng,
) -> Duration {
    let exp = attempt.saturating_sub(1).min(16);
    let factor = 1u64 << exp;
    let capped_ms = base
        .as_millis()
        .saturating_mul(factor as u128)
        .min(max.as_millis());
    let capped_ms = u64::try_from(capped_ms).unwrap_or(u64::MAX);
    let jitter_ms = if capped_ms == 0 {
        0
    } else {
        rng.random_range(0..=capped_ms)
    };
    Duration::from_millis(jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn transient_codes_are_retryable_class() {
        let err = RepoNyxError::new(ErrorCode::NetworkUnavailable, "down");
        assert_eq!(classify_error(&err), FailureClass::Transient);
        let err = RepoNyxError::new(ErrorCode::RateLimitExceeded, "429");
        assert_eq!(classify_error(&err), FailureClass::Transient);
        let err = RepoNyxError::new(ErrorCode::RepositoryLocked, "lock");
        assert_eq!(classify_error(&err), FailureClass::Transient);
    }

    #[test]
    fn permanent_auth_is_not_retried() {
        let err = RepoNyxError::new(ErrorCode::AuthenticationRevoked, "revoked");
        assert_eq!(classify_error(&err), FailureClass::Permanent);
        assert!(!should_retry(FailureClass::Permanent, 1, 6));
    }

    #[test]
    fn attempt_budget_stops_transient_retries() {
        assert!(should_retry(FailureClass::Transient, 1, 6));
        assert!(should_retry(FailureClass::Transient, 5, 6));
        assert!(!should_retry(FailureClass::Transient, 6, 6));
    }

    #[test]
    fn backoff_is_bounded_and_non_negative() {
        let mut rng = StdRng::seed_from_u64(42);
        let base = Duration::from_secs(2);
        let max = Duration::from_secs(300);
        for attempt in 1..=10 {
            let d = compute_backoff(attempt, base, max, &mut rng);
            assert!(d <= max);
        }
    }
}
