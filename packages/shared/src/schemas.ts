import { z } from "zod";

/** Runtime validation for values arriving over the Tauri invoke boundary.
 *  Field names are snake_case, matching serde on the Rust side. */

export const dashboardCountsSchema = z.object({
  accounts: z.number().int().nonnegative(),
  repositories: z.number().int().nonnegative(),
  synced: z.number().int().nonnegative(),
  syncing: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(),
  pending_jobs: z.number().int().nonnegative(),
});

export const gitStatusSchema = z.object({
  available: z.boolean(),
  path: z.string().nullable().optional(),
  version: z.string().nullable().optional(),
  supported: z.boolean(),
  error: z.string().nullable().optional(),
});

export const appInfoSchema = z.object({
  name: z.string(),
  version: z.string(),
  description: z.string(),
});

/** Phase 5 auth DTOs: camelCase, matching the Tauri `auth.rs` DTOs. */

export const accountSchema = z.object({
  id: z.string(),
  provider: z.literal("github"),
  providerAccountId: z.string(),
  username: z.string(),
  displayName: z.string().nullable().optional(),
  label: z.string().nullable().optional(),
  accountType: z.enum(["personal", "organization"]),
  authenticationMethod: z.enum(["github_app", "fine_grained_pat"]),
  status: z.enum([
    "connected",
    "paused",
    "token_expired",
    "revoked",
    "error",
  ]),
  patExpiresAt: z.string().nullable().optional(),
  patExpiryStatus: z.enum(["unknown", "valid", "expiring_soon", "expired"]),
  patExpiryDaysLeft: z.number().int().nullable().optional(),
  lastValidatedAt: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const providerAccountInfoSchema = z.object({
  providerAccountId: z.string(),
  username: z.string(),
  displayName: z.string().nullable().optional(),
  avatarUrl: z.string().nullable().optional(),
});

export const patExpirySchema = z.object({
  status: z.enum(["unknown", "valid", "expiring_soon", "expired"]),
  daysLeft: z.number().int().nullable().optional(),
});

export const pairingInfoSchema = z.object({
  pairingCode: z.string(),
  expiresAt: z.string(),
  installationId: z.string(),
  deviceIdentifier: z.string(),
});

export const brokerStatusSchema = z.object({
  configured: z.boolean(),
  reachable: z.boolean(),
  baseUrl: z.string(),
});

export const brokerInstallationSchema = z.object({
  installationId: z.string(),
  setupAction: z.string(),
  accountLogin: z.string().nullable().optional(),
  installedAt: z.string(),
  pairedDevice: z.string().nullable().optional(),
});

/** Phase 7 scheduler DTOs: camelCase, matching the Tauri
 *  `schedules.rs` DTOs. */

export const scheduleSchema = z.object({
  id: z.string(),
  scope: z.enum(["global", "account", "repository"]),
  accountId: z.string().nullable().optional(),
  repositoryId: z.string().nullable().optional(),
  intervalType: z.enum(["interval_hours", "daily", "weekly", "custom"]),
  intervalValue: z.string().nullable().optional(),
  timezone: z.string(),
  enabled: z.boolean(),
  nextRunAt: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const scheduleWithNextRunSchema = scheduleSchema.extend({
  computedNextRun: z.string().nullable().optional(),
  nextRunError: z.string().nullable().optional(),
});

export const nextRunPreviewSchema = z.object({
  utc: z.string(),
  local: z.string(),
  timezone: z.string(),
});

export const scheduleMaterializeErrorSchema = z.object({
  scheduleId: z.string(),
  message: z.string(),
});

export const materializeReportSchema = z.object({
  checked: z.number().int().nonnegative(),
  primed: z.number().int().nonnegative(),
  createdJobIds: z.array(z.string()),
  skippedDuplicate: z.number().int().nonnegative(),
  skippedSyncDisabled: z.number().int().nonnegative(),
  skippedMissingRepository: z.number().int().nonnegative(),
  skippedOutOfWindow: z.number().int().nonnegative(),
  errors: z.array(scheduleMaterializeErrorSchema),
});

export const effectiveScheduleSchema = z.object({
  schedule: scheduleWithNextRunSchema,
  nextRuns: z.array(z.string()),
});

/** Phase 8 repository DTOs: camelCase, matching the Tauri
 *  `repositories.rs` DTOs. */

export const repositorySchema = z.object({
  id: z.string(),
  accountId: z.string(),
  providerRepositoryId: z.string(),
  owner: z.string(),
  name: z.string(),
  fullName: z.string(),
  visibility: z.enum(["public", "private", "internal"]),
  defaultBranch: z.string().nullable().optional(),
  cloneUrl: z.string(),
  archiveStatus: z.enum([
    "synced",
    "pending",
    "syncing",
    "failed",
    "paused",
    "disconnected",
    "remote_deleted",
  ]),
  syncEnabled: z.boolean(),
  syncMode: z.enum(["mirror", "working", "both"]),
  localPath: z.string().nullable().optional(),
  remoteUpdatedAt: z.string().nullable().optional(),
  lastSyncedAt: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const discoveryOutcomeSchema = z.object({
  created: z.number().int().nonnegative(),
  updated: z.number().int().nonnegative(),
  notSeen: z.array(z.string()),
});

/** Phase 8 synchronization DTOs: camelCase, matching the Tauri
 *  `synchronization.rs` DTOs. */

export const syncJobSchema = z.object({
  id: z.string(),
  repositoryId: z.string(),
  jobType: z.enum([
    "mirror_clone",
    "working_clone",
    "fetch",
    "lfs_fetch",
    "submodule_sync",
  ]),
  status: z.enum(["pending", "running", "completed", "failed", "cancelled"]),
  priority: z.number().int(),
  attemptCount: z.number().int().nonnegative(),
  startedAt: z.string().nullable().optional(),
  completedAt: z.string().nullable().optional(),
  errorCode: z.string().nullable().optional(),
  errorMessage: z.string().nullable().optional(),
  createdAt: z.string(),
  availableAt: z.string().nullable().optional(),
  workerId: z.string().nullable().optional(),
  leaseExpiresAt: z.string().nullable().optional(),
  heartbeatAt: z.string().nullable().optional(),
  cancelRequestedAt: z.string().nullable().optional(),
  dedupeKey: z.string().nullable().optional(),
});

export const jobStatusCountSchema = z.object({
  status: z.enum(["pending", "running", "completed", "failed", "cancelled"]),
  count: z.number().int().nonnegative(),
});

/** Phase 7 background-agent DTOs: camelCase, matching the Tauri
 *  `agent.rs` DTOs. */

export const agentStatusSchema = z.object({
  generation: z.number().int().nonnegative(),
  startedAt: z.string(),
  protocolVersion: z.number().int().nonnegative(),
  paused: z.boolean(),
  pendingJobs: z.number().int().nonnegative(),
  runningJobs: z.number().int().nonnegative(),
  workers: z.number().int().nonnegative(),
  lastMaterializeAt: z.string().nullable().optional(),
  lastMaterializeCreated: z.number().int().nullable().optional(),
});

export const agentHealthSchema = z.object({
  reachable: z.boolean(),
  status: agentStatusSchema.nullable().optional(),
  message: z.string(),
});
