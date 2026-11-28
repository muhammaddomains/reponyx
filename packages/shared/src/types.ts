/** Domain types mirroring `crates/core` (Rust). Keep both in sync. */

export type AuthenticationMethod = "github_app" | "fine_grained_pat";
export type Provider = "github";
export type AccountType = "personal" | "organization";
export type AccountStatus =
  | "connected"
  | "paused"
  | "token_expired"
  | "revoked"
  | "error";
export type Visibility = "public" | "private" | "internal";
export type SyncMode = "mirror" | "working" | "both";
export type ArchiveStatus =
  | "synced"
  | "pending"
  | "syncing"
  | "failed"
  | "paused"
  | "disconnected"
  | "remote_deleted";
export type JobStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";
export type JobType =
  | "mirror_clone"
  | "working_clone"
  | "fetch"
  | "lfs_fetch"
  | "submodule_sync";
export type ScheduleScope = "global" | "account" | "repository";
export type IntervalType =
  | "interval_hours"
  | "daily"
  | "weekly"
  | "custom";
export type SyncResult = "success" | "failed" | "cancelled";
export type DevicePairingStatus = "pending" | "paired" | "revoked";
export type InstallationType = "user" | "organization";
export type InstallationStatus = "active" | "suspended" | "deleted";

export interface Account {
  id: string;
  provider: Provider;
  providerAccountId: string;
  username: string;
  displayName?: string | null;
  label?: string | null;
  accountType: AccountType;
  authenticationMethod: AuthenticationMethod;
  status: AccountStatus;
  /** User-supplied fine-grained PAT expiry (PAT accounts only). */
  patExpiresAt?: string | null;
  /** Computed from `patExpiresAt`: unknown when no expiry is known. */
  patExpiryStatus: "unknown" | "valid" | "expiring_soon" | "expired";
  patExpiryDaysLeft?: number | null;
  lastValidatedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ProviderAccountInfo {
  providerAccountId: string;
  username: string;
  displayName?: string | null;
  avatarUrl?: string | null;
}

export interface PatExpiry {
  status: "unknown" | "valid" | "expiring_soon" | "expired";
  daysLeft?: number | null;
}

export interface PairingInfo {
  pairingCode: string;
  /** `null`: pairing codes never expire. */
  expiresAt: string | null;
  installationId: string;
  deviceIdentifier: string;
}

export interface BrokerStatus {
  configured: boolean;
  reachable: boolean;
  baseUrl: string;
  envFileConfigured: boolean;
}

export interface BrokerInstallation {
  installationId: string;
  setupAction: string;
  accountLogin?: string | null;
  installedAt: string;
  pairedDevice?: string | null;
}

export interface BrokerSetupState {
  installUrl: string;
  expiresAt: string;
}

export interface Repository {
  id: string;
  accountId: string;
  providerRepositoryId: string;
  owner: string;
  name: string;
  fullName: string;
  visibility: Visibility;
  defaultBranch?: string | null;
  cloneUrl: string;
  archiveStatus: ArchiveStatus;
  syncEnabled: boolean;
  syncMode: SyncMode;
  localPath?: string | null;
  remoteUpdatedAt?: string | null;
  lastSyncedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** One page of repositories, with the total the filter matches.
 *  The Repositories page loads pages of 100 and appends them. */
export interface RepositoryPage {
  items: Repository[];
  total: number;
}

export interface SyncJob {
  id: string;
  repositoryId: string;
  jobType: JobType;
  status: JobStatus;
  priority: number;
  attemptCount: number;
  startedAt?: string | null;
  completedAt?: string | null;
  errorCode?: string | null;
  errorMessage?: string | null;
  createdAt: string;
  availableAt?: string | null;
  workerId?: string | null;
  leaseExpiresAt?: string | null;
  heartbeatAt?: string | null;
  cancelRequestedAt?: string | null;
  dedupeKey?: string | null;
}

export interface JobStatusCount {
  status: JobStatus;
  count: number;
}

export interface AgentStatus {
  generation: number;
  startedAt: string;
  protocolVersion: number;
  paused: boolean;
  pendingJobs: number;
  runningJobs: number;
  workers: number;
  lastMaterializeAt?: string | null;
  lastMaterializeCreated?: number | null;
}

export interface AgentHealth {
  reachable: boolean;
  status?: AgentStatus | null;
  message: string;
}

export interface Schedule {
  id: string;
  scope: ScheduleScope;
  accountId?: string | null;
  repositoryId?: string | null;
  intervalType: IntervalType;
  intervalValue?: string | null;
  timezone: string;
  enabled: boolean;
  nextRunAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SyncHistory {
  id: string;
  repositoryId: string;
  jobId?: string | null;
  operation: string;
  result: SyncResult;
  durationMs?: number | null;
  bytesReceived?: number | null;
  startedAt: string;
  completedAt?: string | null;
}

/** Phase 7 (scheduler part) DTOs: camelCase, matching the Tauri
 *  `schedules.rs` DTOs. */

/** A schedule plus the next run computed just now; `nextRunError`
 *  is set instead when the cadence/timezone no longer parses. */
export interface ScheduleWithNextRun extends Schedule {
  computedNextRun?: string | null;
  nextRunError?: string | null;
}

export interface NextRunPreview {
  /** UTC, RFC 3339. */
  utc: string;
  /** Same instant, formatted in `timezone`. */
  local: string;
  timezone: string;
}

export interface ScheduleMaterializeError {
  scheduleId: string;
  message: string;
}

export interface MaterializeReport {
  checked: number;
  primed: number;
  createdJobIds: string[];
  skippedDuplicate: number;
  skippedSyncDisabled: number;
  skippedMissingRepository: number;
  skippedOutOfWindow: number;
  errors: ScheduleMaterializeError[];
}

export interface EffectiveSchedule {
  schedule: ScheduleWithNextRun;
  /** Next runs, UTC RFC 3339. */
  nextRuns: string[];
}

export interface NewScheduleInput {
  scope: ScheduleScope;
  accountId?: string | null;
  repositoryId?: string | null;
  intervalType: IntervalType;
  intervalValue?: string | null;
  timezone: string;
  enabled: boolean;
}

/** Outcome of one discovery run for one account: rows created or
 *  refreshed, and local rows discovery did not return (never
 *  deleted, only reported). */
export interface DiscoveryOutcome {
  created: number;
  updated: number;
  notSeen: string[];
}

/** `id` is a plain string: V1 seeds settings with stable ids such as `setting-storage-root`. */
export interface Setting {
  id: string;
  key: string;
  value: string;
  updatedAt: string;
}

export interface Device {
  id: string;
  deviceName: string;
  deviceIdentifier: string;
  pairingStatus: DevicePairingStatus;
  createdAt: string;
  lastSeenAt?: string | null;
}

/** Append-only: no update/delete API exists for audit logs. */
export interface AuditLog {
  id: string;
  actor: string;
  action: string;
  resourceType: string;
  resourceId?: string | null;
  metadata: string;
  createdAt: string;
}

export interface Installation {
  id: string;
  accountId: string;
  installationId: string;
  installationType: InstallationType;
  installationStatus: InstallationStatus;
  permissionsJson: string;
  createdAt: string;
  updatedAt: string;
}

/** Rust `DashboardCounts` serializes with snake_case field names. */
export interface DashboardCounts {
  accounts: number;
  repositories: number;
  synced: number;
  syncing: number;
  failed: number;
  pending_jobs: number;
}

export interface GitStatus {
  available: boolean;
  path?: string | null;
  version?: string | null;
  supported: boolean;
  error?: string | null;
}

export interface LfsStatus {
  installed: boolean;
  path?: string | null;
  version?: string | null;
  error?: string | null;
}

export interface AppInfo {
  name: string;
  version: string;
  description: string;
}

export interface StoredRepository {
  repositoryId: string;
  accountId: string;
  fullName: string;
  localPath: string;
  bytes: number;
  lastSyncedAt: string | null;
}

/** An archive directory on disk with no repository row (Phase 9). */
export interface OrphanArchive {
  path: string;
  kind: string;
  bytes: number;
  account: string;
  owner: string;
  name: string;
}

/** A repository row whose archive is gone from disk (Phase 9). */
export interface MissingArchive {
  repositoryId: string;
  fullName: string;
  localPath: string;
}

/** A repository row whose archive directory still carries a
 *  pre-rename name (Phase 9). */
export interface RenamedArchive {
  repositoryId: string;
  fullName: string;
  currentPath: string;
  expectedPath: string;
  kind: string;
}

export interface StorageOverview {
  root: string;
  rootConfigured: boolean;
  totalBytes: number | null;
  availableBytes: number | null;
  mirrorsBytes: number;
  databaseBytes: number;
  logsBytes: number;
  storedRepositoryCount: number;
  repositories: StoredRepository[];
  orphans: OrphanArchive[];
  orphansBytes: number;
  missing: MissingArchive[];
  renamed: RenamedArchive[];
}

/** One side moved by `moveArchiveForRename` (Phase 9). */
export interface MoveEntry {
  from: string;
  to: string;
}

export interface MoveArchiveResult {
  moved: MoveEntry[];
  newLocalPath: string | null;
}

export interface Settings {
  defaultSyncMode: string | null;
  concurrency: number | null;
  storageRoot: string | null;
  theme: string | null;
  locale: string | null;
  notificationsEnabled: boolean | null;
  deviceIdentifier: string | null;
  brokerAppId: string | null;
  brokerPrivateKeyPath: string | null;
  dataDir: string;
  appVersion: string;
}

/** The result of a diagnostics export (Phase 9). */
export interface DiagnosticsExport {
  path: string;
  bytes: number;
  generatedAt: string;
}

/** One database backup file (Phase 9). */
export interface DatabaseBackup {
  path: string;
  bytes: number;
  createdAt: string;
}

/** A staged restore (Phase 9): applied at the next startup. */
export interface RestoreStaged {
  pendingPath: string;
  agentWasRunning: boolean;
}

/** The result of a config export (Phase 9). */
export interface ConfigExport {
  path: string;
  bytes: number;
}

/** One entry a config import skipped (Phase 9). */
export interface SkippedRef {
  kind: string;
  label: string;
  reason: string;
}

/** What a config import did (Phase 9). */
export interface ImportReport {
  settingsApplied: number;
  repositoriesApplied: number;
  schedulesCreated: number;
  schedulesUpdated: number;
  skipped: SkippedRef[];
}

/** Live progress event from the background agent (Phase 8). */
export type SyncProgressEvent =
  | {
      type: "job_started";
      job_id: string;
      repository_id: string;
      attempt: number;
      operation: string;
    }
  | {
      type: "job_progress";
      job_id: string;
      stage: string;
      detail?: string | null;
      percent?: number | null;
    }
  | {
      type: "job_completed";
      job_id: string;
      repository_id: string;
      duration_ms?: number | null;
    }
  | {
      type: "job_failed";
      job_id: string;
      repository_id: string;
      error_code: string;
      message: string;
      will_retry: boolean;
      next_attempt?: number | null;
    }
  | { type: "job_cancelled"; job_id: string; repository_id: string }
  | {
      type: "job_warning";
      job_id: string;
      repository_id: string;
      message: string;
    };

/** Web-platform login (plan §4, W3). The desktop talks to the web
 *  (`https://reponyx.bond`) for login, 2FA, session validation and
 *  the account view. The session token itself never reaches the
 *  frontend — these types carry the user and the package only. */

export interface WebUser {
  id: string;
  email: string;
  name: string | null;
  createdAt: string | null;
}

export interface WebPackage {
  name: string;
  maxLinkedAccounts: number;
  maxDevices: number;
  expiresAt: string | null;
}

export interface WebUsage {
  linkedAccounts: number;
  devices: number;
}

export interface WebAccount {
  user: WebUser;
  package: WebPackage;
  usage: WebUsage;
}

export interface WebSessionStatus {
  authenticated: boolean;
  /** The web could not be reached; the stored session is trusted
   *  locally and `account` is the last-known one. */
  offline: boolean;
  /** The web answered 401: the session is dead and was deleted. */
  sessionExpired: boolean;
  account: WebAccount | null;
}

export interface WebLogin {
  /** `"ok"` or `"2fa_required"`. */
  step: string;
  /** Present only for the 2FA step: proves the password step passed. */
  pendingToken: string | null;
  /** Present once the login completed (never the session token). */
  user: WebUser | null;
}

/** A GitHub account linked to the signed-in web account (W4).
 *  `id` is the web row id — the unlink call needs it. */
export interface WebLinkedAccount {
  id: string;
  login: string;
  authKind: string;
}
