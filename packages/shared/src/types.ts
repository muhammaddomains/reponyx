/** Domain types mirroring `crates/core` (Rust). Keep both in sync. */

export type AuthenticationMethod = "github_app" | "fine_grained_pat";
export type Provider = "github";
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

export interface Account {
  id: string;
  provider: Provider;
  providerAccountId: string;
  username: string;
  displayName?: string | null;
  label?: string | null;
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

export interface AppInfo {
  name: string;
  version: string;
  description: string;
}
