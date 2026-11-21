//! In-memory token-bucket rate limiter for job starts.
//!
//! Two buckets are enforced together: one global bucket and one
//! per-account bucket. Each bucket holds up to one minute of tokens
//! (burst capacity) and refills continuously. A limit of 0 disables
//! that bucket. This is the queue-level gate from the Phase 6 plan;
//! GitHub API rate limits are still handled by the provider crate.
//!
//! The limiter only gates *starting* a job. A job that is refused is
//! deferred by the worker (`defer_job`), never failed, and the
//! consumed attempt is refunded.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token bucket: `capacity` tokens max, refilling at
/// `per_minute / 60` tokens per second.
#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl Bucket {
    fn new(per_minute: u32, now: Instant) -> Self {
        Self {
            tokens: per_minute as f64,
            last_refill: now,
        }
    }

    fn refill(&mut self, per_minute: u32, now: Instant) {
        let rate_per_sec = per_minute as f64 / 60.0;
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate_per_sec).min(per_minute as f64);
        self.last_refill = now;
    }

    /// Seconds until one token is available (0 when available now).
    fn wait_secs(&self, per_minute: u32) -> f64 {
        if self.tokens >= 1.0 || per_minute == 0 {
            0.0
        } else {
            (1.0 - self.tokens) / (per_minute as f64 / 60.0)
        }
    }
}

#[derive(Debug, Default)]
struct LimiterState {
    global: Option<Bucket>,
    per_account: HashMap<String, Bucket>,
}

/// Thread-safe job-start rate limiter.
#[derive(Debug)]
pub struct RateLimiter {
    global_per_minute: u32,
    per_account_per_minute: u32,
    state: Mutex<LimiterState>,
}

impl RateLimiter {
    pub fn new(global_per_minute: u32, per_account_per_minute: u32) -> Self {
        Self {
            global_per_minute,
            per_account_per_minute,
            state: Mutex::new(LimiterState::default()),
        }
    }

    /// Try to spend one token from the global bucket and one from the
    /// account bucket. On success both are spent. On failure nothing
    /// is spent and the returned duration is how long the caller
    /// should wait before the job may start.
    pub fn try_acquire(&self, account_key: &str) -> Result<(), Duration> {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        if self.global_per_minute > 0 {
            let bucket = state
                .global
                .get_or_insert_with(|| Bucket::new(self.global_per_minute, now));
            bucket.refill(self.global_per_minute, now);
            let wait = bucket.wait_secs(self.global_per_minute);
            if wait > 0.0 {
                return Err(Duration::from_secs_f64(wait));
            }
        }
        if self.per_account_per_minute > 0 {
            let bucket = state
                .per_account
                .entry(account_key.to_string())
                .or_insert_with(|| Bucket::new(self.per_account_per_minute, now));
            bucket.refill(self.per_account_per_minute, now);
            let wait = bucket.wait_secs(self.per_account_per_minute);
            if wait > 0.0 {
                return Err(Duration::from_secs_f64(wait));
            }
        }

        if let Some(bucket) = state.global.as_mut() {
            bucket.tokens -= 1.0;
        }
        if let Some(bucket) = state.per_account.get_mut(account_key) {
            bucket.tokens -= 1.0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_up_to_capacity_then_waits() {
        let limiter = RateLimiter::new(2, 0);
        assert!(limiter.try_acquire("a").is_ok());
        assert!(limiter.try_acquire("a").is_ok());
        let wait = limiter.try_acquire("a").unwrap_err();
        assert!(wait > Duration::from_secs(0));
        assert!(wait <= Duration::from_secs(31));
    }

    #[test]
    fn per_account_buckets_are_independent() {
        let limiter = RateLimiter::new(0, 1);
        assert!(limiter.try_acquire("a").is_ok());
        assert!(limiter.try_acquire("a").is_err());
        assert!(limiter.try_acquire("b").is_ok());
    }

    #[test]
    fn zero_limit_is_unlimited() {
        let limiter = RateLimiter::new(0, 0);
        for _ in 0..1000 {
            assert!(limiter.try_acquire("a").is_ok());
        }
    }
}
