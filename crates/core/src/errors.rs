//! Structured errors (spec §17).
//!
//! Every error carries a stable machine-readable code, a human-readable
//! message, technical diagnostics, whether it is retryable, and a
//! suggested action. Callers must not invent ad-hoc error strings for
//! conditions listed here.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable error codes. The string form is the wire/stable identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ErrorCode {
    AuthenticationExpired,
    AuthenticationRevoked,
    InsufficientPermissions,
    RepositoryNotFound,
    RepositoryDeletedRemotely,
    RateLimitExceeded,
    NetworkUnavailable,
    GitNotFound,
    GitVersionUnsupported,
    GitCloneFailed,
    GitFetchFailed,
    GitLfsFailed,
    SubmoduleAccessDenied,
    DiskSpaceInsufficient,
    RepositoryLocked,
    RepositoryCorrupted,
    RepositoryPathInvalid,
    BackgroundAgentUnavailable,
    DatabaseError,
    CredentialStoreError,
    InvalidInput,
    OperationCancelled,
    OperationTimedOut,
    InternalError,
}

impl ErrorCode {
    /// Stable string identifier, e.g. `AuthenticationExpired`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthenticationExpired => "AuthenticationExpired",
            Self::AuthenticationRevoked => "AuthenticationRevoked",
            Self::InsufficientPermissions => "InsufficientPermissions",
            Self::RepositoryNotFound => "RepositoryNotFound",
            Self::RepositoryDeletedRemotely => "RepositoryDeletedRemotely",
            Self::RateLimitExceeded => "RateLimitExceeded",
            Self::NetworkUnavailable => "NetworkUnavailable",
            Self::GitNotFound => "GitNotFound",
            Self::GitVersionUnsupported => "GitVersionUnsupported",
            Self::GitCloneFailed => "GitCloneFailed",
            Self::GitFetchFailed => "GitFetchFailed",
            Self::GitLfsFailed => "GitLfsFailed",
            Self::SubmoduleAccessDenied => "SubmoduleAccessDenied",
            Self::DiskSpaceInsufficient => "DiskSpaceInsufficient",
            Self::RepositoryLocked => "RepositoryLocked",
            Self::RepositoryCorrupted => "RepositoryCorrupted",
            Self::RepositoryPathInvalid => "RepositoryPathInvalid",
            Self::BackgroundAgentUnavailable => "BackgroundAgentUnavailable",
            Self::DatabaseError => "DatabaseError",
            Self::CredentialStoreError => "CredentialStoreError",
            Self::InvalidInput => "InvalidInput",
            Self::OperationCancelled => "OperationCancelled",
            Self::OperationTimedOut => "OperationTimedOut",
            Self::InternalError => "InternalError",
        }
    }

    /// Whether retrying the same operation later may succeed.
    /// Permanent auth/permission errors are never retried indefinitely.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimitExceeded
                | Self::NetworkUnavailable
                | Self::GitFetchFailed
                | Self::GitCloneFailed
                | Self::GitLfsFailed
                | Self::RepositoryLocked
                | Self::OperationTimedOut
                | Self::DatabaseError
        )
    }

    /// Actionable guidance shown to the user.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            Self::AuthenticationExpired => "Reconnect the account to refresh its credential.",
            Self::AuthenticationRevoked => {
                "Re-authorize the account; the previous grant was revoked."
            }
            Self::InsufficientPermissions => {
                "Check the GitHub App installation / PAT permissions (Contents: Read, Metadata: Read)."
            }
            Self::RepositoryNotFound => {
                "Verify the repository still exists and the account can access it."
            }
            Self::RepositoryDeletedRemotely => {
                "The remote repository was deleted. The local archive is kept; delete it only if you confirm."
            }
            Self::RateLimitExceeded => "Wait for the rate-limit window, then retry.",
            Self::NetworkUnavailable => "Check the network connection and retry.",
            Self::GitNotFound => "Install Git (>= 2.40) and ensure it is on PATH.",
            Self::GitVersionUnsupported => "Upgrade Git to version 2.40 or newer.",
            Self::GitCloneFailed => "Check the repository URL, access, and disk space, then retry.",
            Self::GitFetchFailed => "Check network and access, then retry the fetch.",
            Self::GitLfsFailed => "Check LFS availability and retry; see the sync log for details.",
            Self::SubmoduleAccessDenied => {
                "Grant access to the submodule repository or disable submodule sync for this repo."
            }
            Self::DiskSpaceInsufficient => "Free disk space on the storage volume and retry.",
            Self::RepositoryLocked => "Another operation is using this repository; wait and retry.",
            Self::RepositoryCorrupted => {
                "Run repository recovery/diagnostics; do not delete the archive without confirmation."
            }
            Self::RepositoryPathInvalid => {
                "The repository path failed validation; check the storage settings."
            }
            Self::BackgroundAgentUnavailable => {
                "Start the RepoNyx background agent and check its status in Settings > Background."
            }
            Self::DatabaseError => "Check the database file and disk, then restart RepoNyx.",
            Self::CredentialStoreError => {
                "Check the OS credential store and reconnect the account."
            }
            Self::InvalidInput => "Correct the input and try again.",
            Self::OperationCancelled => "The operation was cancelled; run it again if needed.",
            Self::OperationTimedOut => {
                "The operation timed out; retry, or increase the timeout in Settings."
            }
            Self::InternalError => {
                "Report this error with the diagnostics export (credentials excluded)."
            }
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The single error type for RepoNyx operations.
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
#[error("[{code}] {message}")]
pub struct RepoNyxError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl RepoNyxError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Retryability is a property of the code, not of the instance.
    pub fn is_retryable(&self) -> bool {
        self.code.is_retryable()
    }

    pub fn suggested_action(&self) -> &'static str {
        self.code.suggested_action()
    }
}

impl From<std::io::Error> for RepoNyxError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorCode::InternalError, "I/O operation failed").with_details(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable_strings() {
        assert_eq!(
            ErrorCode::AuthenticationExpired.as_str(),
            "AuthenticationExpired"
        );
        assert_eq!(ErrorCode::GitFetchFailed.as_str(), "GitFetchFailed");
        assert_eq!(
            ErrorCode::DiskSpaceInsufficient.as_str(),
            "DiskSpaceInsufficient"
        );
    }

    #[test]
    fn permanent_auth_errors_are_not_retryable() {
        assert!(!ErrorCode::AuthenticationExpired.is_retryable());
        assert!(!ErrorCode::AuthenticationRevoked.is_retryable());
        assert!(!ErrorCode::InsufficientPermissions.is_retryable());
        assert!(!ErrorCode::RepositoryNotFound.is_retryable());
    }

    #[test]
    fn transient_errors_are_retryable() {
        assert!(ErrorCode::RateLimitExceeded.is_retryable());
        assert!(ErrorCode::NetworkUnavailable.is_retryable());
        assert!(ErrorCode::RepositoryLocked.is_retryable());
    }

    #[test]
    fn every_code_has_a_suggested_action() {
        let codes = [
            ErrorCode::AuthenticationExpired,
            ErrorCode::AuthenticationRevoked,
            ErrorCode::InsufficientPermissions,
            ErrorCode::RepositoryNotFound,
            ErrorCode::RepositoryDeletedRemotely,
            ErrorCode::RateLimitExceeded,
            ErrorCode::NetworkUnavailable,
            ErrorCode::GitNotFound,
            ErrorCode::GitVersionUnsupported,
            ErrorCode::GitCloneFailed,
            ErrorCode::GitFetchFailed,
            ErrorCode::GitLfsFailed,
            ErrorCode::SubmoduleAccessDenied,
            ErrorCode::DiskSpaceInsufficient,
            ErrorCode::RepositoryLocked,
            ErrorCode::RepositoryCorrupted,
            ErrorCode::RepositoryPathInvalid,
            ErrorCode::BackgroundAgentUnavailable,
            ErrorCode::DatabaseError,
            ErrorCode::CredentialStoreError,
            ErrorCode::InvalidInput,
            ErrorCode::OperationCancelled,
            ErrorCode::OperationTimedOut,
            ErrorCode::InternalError,
        ];
        for code in codes {
            assert!(
                !code.suggested_action().is_empty(),
                "{code:?} has no action"
            );
            assert!(!code.as_str().is_empty());
        }
    }

    #[test]
    fn error_serializes_with_code_and_message() {
        let err = RepoNyxError::new(ErrorCode::GitFetchFailed, "fetch failed");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("GitFetchFailed"));
        assert!(json.contains("fetch failed"));
    }
}
