//! Options shared by the long-running Git operations (clone / fetch).
//!
//! Every long operation takes a [`SyncOptions`]: a timeout, a lock
//! acquisition timeout, an optional [`CancellationToken`], an optional
//! progress sink, and an optional credential helper. Short local reads
//! (status, ref listing, health checks) take a plain `Duration`
//! timeout instead — they finish in milliseconds and do not need a
//! cancellation token.

use crate::credentials::CredentialHelper;
use crate::progress::ProgressEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Default overall timeout for a sync operation (5 minutes).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// Default time to wait for another operation on the same repository
/// to release its lock (60 seconds).
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// Options for a long Git operation.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// Overall timeout for the operation. On expiry the child process
    /// is killed and the error is `OperationTimedOut`.
    pub timeout: Duration,
    /// Time to wait for the per-repository lock before failing with
    /// `RepositoryLocked`.
    pub lock_timeout: Duration,
    /// Cancellation token. When cancelled, the child process is killed,
    /// the lock is released, and the error is `OperationCancelled`.
    /// `None` means the operation cannot be cancelled.
    pub cancel: Option<CancellationToken>,
    /// Where progress events go. `None` means progress is not reported.
    pub progress: Option<mpsc::Sender<ProgressEvent>>,
    /// Credential helper to install on the Git command, if any. The
    /// helper (and its temporary directory) lives as long as this
    /// `Arc` is held; drop it when the operation is done.
    pub credential: Option<Arc<CredentialHelper>>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
            cancel: None,
            progress: None,
            credential: None,
        }
    }
}

impl SyncOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_lock_timeout(mut self, lock_timeout: Duration) -> Self {
        self.lock_timeout = lock_timeout;
        self
    }

    pub fn with_cancel(mut self, token: CancellationToken) -> Self {
        self.cancel = Some(token);
        self
    }

    pub fn with_progress(mut self, sender: mpsc::Sender<ProgressEvent>) -> Self {
        self.progress = Some(sender);
        self
    }

    pub fn with_credential(mut self, helper: Arc<CredentialHelper>) -> Self {
        self.credential = Some(helper);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let opts = SyncOptions::default();
        assert_eq!(opts.timeout, Duration::from_secs(300));
        assert_eq!(opts.lock_timeout, Duration::from_secs(60));
        assert!(opts.cancel.is_none());
        assert!(opts.progress.is_none());
        assert!(opts.credential.is_none());
    }
}
