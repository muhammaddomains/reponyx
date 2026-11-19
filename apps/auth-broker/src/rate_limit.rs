//! Per-key token-bucket rate limiting.
//!
//! A hand-rolled bucket (no extra dependency): each key holds a
//! token count refilled continuously at `refill_per_sec`, capped at
//! `capacity`. `check` consumes one token and returns the wait until
//! a token is available when the bucket is empty. Limits are
//! enforced per client key (IP, or device for token minting), so one
//! abusive client cannot exhaust the broker for everyone else.
//!
//! Pairing codes have their own attempt limit (5 wrong codes
//! invalidates the code) on top of the per-IP bucket.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    updated: Instant,
}

/// A token bucket per key.
#[derive(Debug, Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume one token for `key`. `Ok(())` when allowed;
    /// `Err(wait)` with the duration until a token is available
    /// when the bucket is empty.
    pub fn check(&self, key: &str, capacity: f64, refill_per_sec: f64) -> Result<(), Duration> {
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: capacity,
            updated: now,
        });
        let elapsed = now.duration_since(bucket.updated).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_per_sec).min(capacity);
        bucket.updated = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait_secs = (1.0 - bucket.tokens) / refill_per_sec;
            Err(Duration::from_secs_f64(wait_secs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_capacity_then_blocks() {
        let limiter = RateLimiter::new();
        // Capacity 2, refill 1 per second: two immediate requests pass.
        assert!(limiter.check("k", 2.0, 1.0).is_ok());
        assert!(limiter.check("k", 2.0, 1.0).is_ok());
        // Third is blocked, with a wait of about 1 s.
        let wait = limiter.check("k", 2.0, 1.0).unwrap_err();
        assert!(wait <= Duration::from_secs(1));
        // A different key has its own bucket.
        assert!(limiter.check("other", 2.0, 1.0).is_ok());
    }
}
