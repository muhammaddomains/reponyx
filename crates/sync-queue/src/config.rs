//! Queue configuration.
//!
//! Defaults match the Phase 6 plan and the V1-seeded setting
//! `sync.concurrency = 5`. Values are clamped so a bad setting cannot
//! open an unbounded worker pool.

use reponyx_database::settings::get_setting;
use sqlx::SqlitePool;
use std::time::Duration;

/// Runtime configuration for the sync queue.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum concurrent workers. Default 5, hard max 10.
    pub max_concurrency: usize,
    /// How long an idle worker sleeps between claim attempts.
    pub claim_poll: Duration,
    /// Maximum attempts (including the first). After this, permanent fail.
    pub max_attempts: u32,
    /// Base delay for exponential backoff.
    pub base_backoff: Duration,
    /// Cap on backoff delay.
    pub max_backoff: Duration,
    /// Per-job overall timeout passed to the runner / git-engine.
    pub job_timeout: Duration,
    /// Running jobs older than this are treated as abandoned on recovery.
    pub recovery_grace: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 5,
            claim_poll: Duration::from_millis(250),
            max_attempts: 6,
            base_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(300),
            job_timeout: Duration::from_secs(600),
            recovery_grace: Duration::from_secs(30),
        }
    }
}

impl QueueConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clamp concurrency into `1..=10`.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.clamp(1, 10);
        self
    }

    pub fn with_max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = n.max(1);
        self
    }

    pub fn with_claim_poll(mut self, d: Duration) -> Self {
        self.claim_poll = d;
        self
    }

    pub fn with_job_timeout(mut self, d: Duration) -> Self {
        self.job_timeout = d;
        self
    }

    pub fn with_recovery_grace(mut self, d: Duration) -> Self {
        self.recovery_grace = d;
        self
    }

    /// Load `sync.concurrency` from settings when present; other fields
    /// keep their defaults. Invalid / out-of-range values fall back to
    /// the default concurrency rather than panicking.
    pub async fn from_settings(pool: &SqlitePool) -> Self {
        let mut cfg = Self::default();
        if let Ok(Some(setting)) = get_setting(pool, "sync.concurrency")
            && let Ok(n) = setting.value.parse::<usize>()
        {
            cfg.max_concurrency = n.clamp(1, 10);
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_is_clamped() {
        assert_eq!(QueueConfig::default().with_concurrency(0).max_concurrency, 1);
        assert_eq!(
            QueueConfig::default().with_concurrency(100).max_concurrency,
            10
        );
        assert_eq!(QueueConfig::default().with_concurrency(5).max_concurrency, 5);
    }
}
