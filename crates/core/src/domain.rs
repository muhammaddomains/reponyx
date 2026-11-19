//! Domain types shared across the workspace.
//!
//! These mirror `packages/shared` on the TypeScript side. Keep both in
//! sync when a field changes.

use crate::errors::{ErrorCode, RepoNyxError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// String-backed enums stored as snake_case TEXT in SQLite.
/// `as_str` is the stored form; `FromStr` parses it back. Unknown
/// stored values are `InvalidInput`, never silently defaulted.
macro_rules! string_enum {
    ($name:ident, $($variant:ident => $s:literal),+ $(,)?) => {
        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $s,)+
                }
            }
        }

        impl std::str::FromStr for $name {
            type Err = RepoNyxError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok(Self::$variant),)+
                    _ => Err(RepoNyxError::new(
                        ErrorCode::InvalidInput,
                        format!("unknown {} value: {s}", stringify!($name)),
                    )),
                }
            }
        }
    };
}

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

/// Personal account vs. GitHub organization account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Personal,
    Organization,
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
    pub account_type: AccountType,
    pub authentication_method: AuthenticationMethod,
    pub status: AccountStatus,
    pub last_validated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AccountStatus {
    /// Statuses a normal status change may move to.
    ///
    /// `revoked` and `token_expired` are terminal-ish: they never move
    /// back to `connected` here. Only an explicit reconnect (the
    /// `reconnect_account` repository call, which also refreshes
    /// `last_validated_at`) restores them.
    pub fn allowed_transitions(&self) -> &'static [AccountStatus] {
        match self {
            Self::Connected => &[Self::Paused, Self::TokenExpired, Self::Revoked, Self::Error],
            Self::Paused => &[
                Self::Connected,
                Self::TokenExpired,
                Self::Revoked,
                Self::Error,
            ],
            // No `Connected`: reconnect only.
            Self::TokenExpired => &[Self::Paused, Self::Revoked, Self::Error],
            // Terminal: reconnect only.
            Self::Revoked => &[],
            // Transient: revalidation may restore it.
            Self::Error => &[
                Self::Connected,
                Self::Paused,
                Self::TokenExpired,
                Self::Revoked,
            ],
        }
    }
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

impl JobStatus {
    /// Valid next statuses. `completed`, `failed` and `cancelled` are
    /// terminal; a job is never re-run by moving it back to `pending`
    /// — a new job is created instead.
    pub fn allowed_transitions(&self) -> &'static [JobStatus] {
        match self {
            Self::Pending => &[Self::Running, Self::Cancelled],
            Self::Running => &[Self::Completed, Self::Failed, Self::Cancelled],
            Self::Completed | Self::Failed | Self::Cancelled => &[],
        }
    }
}

/// What a schedule applies to. Precedence for a repository is
/// repository > account > global (see `effective_schedule_for` in
/// `reponyx-database`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleScope {
    Global,
    Account,
    Repository,
}

/// How a schedule's cadence is expressed. `interval_value` holds the
/// cadence detail: hours as a number for `interval_hours`, a `HH:MM`
/// time for `daily`, a weekday + time for `weekly`, free-form for
/// `custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalType {
    IntervalHours,
    Daily,
    Weekly,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: Uuid,
    pub scope: ScheduleScope,
    pub account_id: Option<Uuid>,
    pub repository_id: Option<Uuid>,
    pub interval_type: IntervalType,
    pub interval_value: Option<String>,
    /// IANA timezone name, e.g. `UTC` or `America/Phoenix`.
    pub timezone: String,
    pub enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Outcome of one sync operation, as recorded in `sync_history`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncResult {
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHistory {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub job_id: Option<Uuid>,
    /// Operation that ran, e.g. `mirror_clone` or `fetch`.
    pub operation: String,
    pub result: SyncResult,
    pub duration_ms: Option<i64>,
    pub bytes_received: Option<i64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// One key/value setting. `id` is a plain string: the V1 migration
/// seeds settings with stable ids such as `setting-storage-root`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub id: String,
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePairingStatus {
    Pending,
    Paired,
    Revoked,
}

impl DevicePairingStatus {
    /// Valid next statuses. `revoked` is terminal: a revoked device
    /// must be registered again, never re-paired in place.
    pub fn allowed_transitions(&self) -> &'static [DevicePairingStatus] {
        match self {
            Self::Pending => &[Self::Paired, Self::Revoked],
            Self::Paired => &[Self::Revoked],
            Self::Revoked => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub device_name: String,
    /// Stable per-device identifier (e.g. a machine id), unique.
    pub device_identifier: String,
    pub pairing_status: DevicePairingStatus,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// One audit-log entry. Audit logs are append-only by design:
/// `reponyx-database` exposes insert + list only, no update/delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    /// JSON object text, default `{}`.
    pub metadata: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationType {
    User,
    Organization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationStatus {
    Active,
    Suspended,
    Deleted,
}

impl InstallationStatus {
    /// Valid next statuses. `deleted` is terminal.
    pub fn allowed_transitions(&self) -> &'static [InstallationStatus] {
        match self {
            Self::Active => &[Self::Suspended, Self::Deleted],
            Self::Suspended => &[Self::Active, Self::Deleted],
            Self::Deleted => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    pub id: Uuid,
    pub account_id: Uuid,
    /// GitHub installation id.
    pub installation_id: String,
    pub installation_type: InstallationType,
    pub installation_status: InstallationStatus,
    /// JSON object text describing the granted permissions.
    pub permissions_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Stored (snake_case) forms for every string-backed enum.
string_enum!(AuthenticationMethod, GitHubApp => "github_app", FineGrainedPat => "fine_grained_pat");
string_enum!(Provider, GitHub => "github");
string_enum!(AccountType, Personal => "personal", Organization => "organization");
string_enum!(AccountStatus, Connected => "connected", Paused => "paused", TokenExpired => "token_expired", Revoked => "revoked", Error => "error");
string_enum!(Visibility, Public => "public", Private => "private", Internal => "internal");
string_enum!(SyncMode, Mirror => "mirror", Working => "working", Both => "both");
string_enum!(ArchiveStatus, Synced => "synced", Pending => "pending", Syncing => "syncing", Failed => "failed", Paused => "paused", Disconnected => "disconnected", RemoteDeleted => "remote_deleted");
string_enum!(JobStatus, Pending => "pending", Running => "running", Completed => "completed", Failed => "failed", Cancelled => "cancelled");
string_enum!(JobType, MirrorClone => "mirror_clone", WorkingClone => "working_clone", Fetch => "fetch", LfsFetch => "lfs_fetch", SubmoduleSync => "submodule_sync");
string_enum!(ScheduleScope, Global => "global", Account => "account", Repository => "repository");
string_enum!(IntervalType, IntervalHours => "interval_hours", Daily => "daily", Weekly => "weekly", Custom => "custom");
string_enum!(SyncResult, Success => "success", Failed => "failed", Cancelled => "cancelled");
string_enum!(DevicePairingStatus, Pending => "pending", Paired => "paired", Revoked => "revoked");
string_enum!(InstallationType, User => "user", Organization => "organization");
string_enum!(InstallationStatus, Active => "active", Suspended => "suspended", Deleted => "deleted");

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

/// Git LFS availability as reported by `reponyx-git-engine`.
/// Detection only — LFS object sync itself is Phase 9.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsStatus {
    /// `true` when a `git-lfs` executable was found and its version
    /// could be parsed.
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
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

    #[test]
    fn string_enums_round_trip() {
        use std::str::FromStr;
        assert_eq!(Provider::from_str("github").unwrap(), Provider::GitHub);
        assert_eq!(Provider::GitHub.as_str(), "github");
        assert_eq!(
            AuthenticationMethod::from_str("fine_grained_pat").unwrap(),
            AuthenticationMethod::FineGrainedPat
        );
        assert_eq!(
            AccountType::from_str("organization").unwrap(),
            AccountType::Organization
        );
        assert_eq!(
            AccountStatus::from_str("token_expired").unwrap(),
            AccountStatus::TokenExpired
        );
        assert_eq!(ArchiveStatus::RemoteDeleted.as_str(), "remote_deleted");
        assert_eq!(JobType::LfsFetch.as_str(), "lfs_fetch");
        assert_eq!(
            ScheduleScope::from_str("repository").unwrap(),
            ScheduleScope::Repository
        );
        assert_eq!(IntervalType::IntervalHours.as_str(), "interval_hours");
        assert_eq!(SyncResult::Cancelled.as_str(), "cancelled");
        assert_eq!(
            DevicePairingStatus::from_str("revoked").unwrap(),
            DevicePairingStatus::Revoked
        );
        assert_eq!(
            InstallationStatus::from_str("suspended").unwrap(),
            InstallationStatus::Suspended
        );
        assert!(Provider::from_str("gitlab").is_err());
        assert!(AccountStatus::from_str("connected ").is_err());
    }

    #[test]
    fn account_transitions_terminal_states_need_reconnect() {
        // Revoked and token_expired never go back to connected directly.
        assert!(
            !AccountStatus::Revoked
                .allowed_transitions()
                .contains(&AccountStatus::Connected)
        );
        assert!(
            !AccountStatus::TokenExpired
                .allowed_transitions()
                .contains(&AccountStatus::Connected)
        );
        assert!(
            AccountStatus::Connected
                .allowed_transitions()
                .contains(&AccountStatus::Paused)
        );
        assert!(
            AccountStatus::Error
                .allowed_transitions()
                .contains(&AccountStatus::Connected)
        );
        assert!(AccountStatus::Revoked.allowed_transitions().is_empty());
    }

    #[test]
    fn job_device_and_installation_transitions() {
        assert!(
            JobStatus::Pending
                .allowed_transitions()
                .contains(&JobStatus::Running)
        );
        assert!(
            !JobStatus::Pending
                .allowed_transitions()
                .contains(&JobStatus::Completed)
        );
        assert!(JobStatus::Completed.allowed_transitions().is_empty());
        assert!(JobStatus::Failed.allowed_transitions().is_empty());
        assert!(
            DevicePairingStatus::Pending
                .allowed_transitions()
                .contains(&DevicePairingStatus::Paired)
        );
        assert!(
            !DevicePairingStatus::Revoked
                .allowed_transitions()
                .contains(&DevicePairingStatus::Paired)
        );
        assert!(
            InstallationStatus::Suspended
                .allowed_transitions()
                .contains(&InstallationStatus::Active)
        );
        assert!(InstallationStatus::Deleted.allowed_transitions().is_empty());
    }

    #[test]
    fn new_domain_types_serialize_snake_case() {
        let schedule = Schedule {
            id: Uuid::new_v4(),
            scope: ScheduleScope::Repository,
            account_id: None,
            repository_id: Some(Uuid::new_v4()),
            interval_type: IntervalType::IntervalHours,
            interval_value: Some("6".to_string()),
            timezone: "UTC".to_string(),
            enabled: true,
            next_run_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&schedule).unwrap();
        assert!(json.contains("\"repository\""));
        assert!(json.contains("\"interval_hours\""));
        assert!(json.contains("\"interval_value\":\"6\""));
    }
}
