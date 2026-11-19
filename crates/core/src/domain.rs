//! Domain types shared across the workspace.
//!
//! These mirror `packages/shared` on the TypeScript side. Keep both in
//! sync when a field changes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How an account authenticates. Mixed auth is supported: different
/// accounts on the same installation may use different methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationMethod {
    GitHubApp,
    FineGrainedPat,
}

/// Provider identity. Only GitHub exists in Phase 1; the enum is the
/// extension point for GitLab / Bitbucket (see `provider-abstractions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    GitHub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Connected,
    Paused,
    TokenExpired,
    Revoked,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub provider: Provider,
    pub provider_account_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub label: Option<String>,
    pub authentication_method: AuthenticationMethod,
    pub status: AccountStatus,
    pub last_validated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// `git clone --mirror` — the default archival mode.
    Mirror,
    /// Ordinary working clone.
    Working,
    /// One mirror archive plus one working clone, separate locations.
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveStatus {
    Synced,
    Pending,
    Syncing,
    Failed,
    Paused,
    Disconnected,
    RemoteDeleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: Uuid,
    pub account_id: Uuid,
    /// Provider-stable id; never identify a repo by display name alone.
    pub provider_repository_id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub visibility: Visibility,
    pub default_branch: Option<String>,
    pub clone_url: String,
    pub archive_status: ArchiveStatus,
    pub sync_enabled: bool,
    pub sync_mode: SyncMode,
    pub local_path: Option<String>,
    pub remote_updated_at: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    MirrorClone,
    WorkingClone,
    Fetch,
    LfsFetch,
    SubmoduleSync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncJob {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub job_type: JobType,
    pub status: JobStatus,
    pub priority: i32,
    pub attempt_count: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardCounts {
    pub accounts: i64,
    pub repositories: i64,
    pub synced: i64,
    pub syncing: i64,
    pub failed: i64,
    pub pending_jobs: i64,
}

/// Git availability as reported by `reponyx-git-engine`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// `true` when the detected version meets the minimum (>= 2.40).
    pub supported: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_counts_default_to_zero() {
        let c = DashboardCounts::default();
        assert_eq!(c.accounts, 0);
        assert_eq!(c.repositories, 0);
        assert_eq!(c.failed, 0);
    }

    #[test]
    fn sync_mode_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&SyncMode::Mirror).unwrap(),
            "\"mirror\""
        );
        assert_eq!(serde_json::to_string(&SyncMode::Both).unwrap(), "\"both\"");
    }
}
