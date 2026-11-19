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
  lastValidatedAt?: string | null;
  createdAt: string;
  updatedAt: string;
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
