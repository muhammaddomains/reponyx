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
  expiresAt: z.string().nullable(),
  installationId: z.string(),
  deviceIdentifier: z.string(),
});

export const brokerStatusSchema = z.object({
  configured: z.boolean(),
  reachable: z.boolean(),
  baseUrl: z.string(),
  appIdConfigured: z.boolean(),
  privateKeyConfigured: z.boolean(),
  envFileConfigured: z.boolean(),
});

export const brokerInstallationSchema = z.object({
  installationId: z.string(),
  setupAction: z.string(),
  accountLogin: z.string().nullable().optional(),
  installedAt: z.string(),
  pairedDevice: z.string().nullable().optional(),
});

export const brokerSetupStateSchema = z.object({
  installUrl: z.string(),
  expiresAt: z.string(),
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

export const repositoryPageSchema = z.object({
  items: z.array(repositorySchema),
  total: z.number(),
});

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

export const syncHistorySchema = z.object({
  id: z.string(),
  repositoryId: z.string(),
  jobId: z.string().nullable().optional(),
  operation: z.string(),
  result: z.enum(["success", "failed", "cancelled"]),
  durationMs: z.number().int().nullable().optional(),
  bytesReceived: z.number().int().nullable().optional(),
  startedAt: z.string(),
  completedAt: z.string().nullable().optional(),
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

/** Phase 8 Storage DTOs: camelCase, matching the Tauri
 *  `storage.rs` DTOs. */

export const storedRepositorySchema = z.object({
  repositoryId: z.string().uuid(),
  accountId: z.string().uuid(),
  fullName: z.string(),
  localPath: z.string(),
  bytes: z.number().int().nonnegative(),
  lastSyncedAt: z.string().nullable(),
});

export const orphanArchiveSchema = z.object({
  path: z.string(),
  kind: z.string(),
  bytes: z.number().int().nonnegative(),
  account: z.string(),
  owner: z.string(),
  name: z.string(),
});

export const missingArchiveSchema = z.object({
  repositoryId: z.string().uuid(),
  fullName: z.string(),
  localPath: z.string(),
});

export const renamedArchiveSchema = z.object({
  repositoryId: z.string().uuid(),
  fullName: z.string(),
  currentPath: z.string(),
  expectedPath: z.string(),
  kind: z.string(),
});

export const moveEntrySchema = z.object({
  from: z.string(),
  to: z.string(),
});

export const moveArchiveResultSchema = z.object({
  moved: z.array(moveEntrySchema),
  newLocalPath: z.string().nullable(),
});

export const storageOverviewSchema = z.object({
  root: z.string(),
  rootConfigured: z.boolean(),
  totalBytes: z.number().int().nonnegative().nullable(),
  availableBytes: z.number().int().nonnegative().nullable(),
  mirrorsBytes: z.number().int().nonnegative(),
  databaseBytes: z.number().int().nonnegative(),
  logsBytes: z.number().int().nonnegative(),
  storedRepositoryCount: z.number().int().nonnegative(),
  repositories: z.array(storedRepositorySchema),
  orphans: z.array(orphanArchiveSchema),
  orphansBytes: z.number().int().nonnegative(),
  missing: z.array(missingArchiveSchema),
  renamed: z.array(renamedArchiveSchema),
});

/** Phase 8 Settings DTO: camelCase, matching the Tauri
 *  `settings.rs` DTO. */

export const settingsSchema = z.object({
  defaultSyncMode: z.string().nullable(),
  concurrency: z.number().int().nullable(),
  storageRoot: z.string().nullable(),
  theme: z.string().nullable(),
  notificationsEnabled: z.boolean().nullable(),
  deviceIdentifier: z.string().nullable(),
  brokerAppId: z.string().nullable(),
  brokerPrivateKeyPath: z.string().nullable(),
  dataDir: z.string(),
  appVersion: z.string(),
});

export const diagnosticsExportSchema = z.object({
  path: z.string(),
  bytes: z.number().int().nonnegative(),
  generatedAt: z.string(),
});

export const databaseBackupSchema = z.object({
  path: z.string(),
  bytes: z.number().int().nonnegative(),
  createdAt: z.string(),
});

export const restoreStagedSchema = z.object({
  pendingPath: z.string(),
  agentWasRunning: z.boolean(),
});

export const configExportSchema = z.object({
  path: z.string(),
  bytes: z.number().int().nonnegative(),
});

export const skippedRefSchema = z.object({
  kind: z.string(),
  label: z.string(),
  reason: z.string(),
});

export const importReportSchema = z.object({
  settingsApplied: z.number().int().nonnegative(),
  repositoriesApplied: z.number().int().nonnegative(),
  schedulesCreated: z.number().int().nonnegative(),
  schedulesUpdated: z.number().int().nonnegative(),
  skipped: z.array(skippedRefSchema),
});

/**
 * Live progress events from the background agent, forwarded over IPC
 * as JSON and re-emitted by the desktop as `sync-progress` Tauri
 * events (Phase 8). Field names are snake_case, straight from the
 * Rust `SyncProgressEvent` serde shape — not the camelCase of the
 * Tauri command DTOs. Only the job variants the Synchronization
 * page renders are listed; anything else fails validation and is
 * ignored by the page.
 */
export const syncProgressEventSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("job_started"),
    job_id: z.string(),
    repository_id: z.string(),
    attempt: z.number().int(),
    operation: z.string(),
  }),
  z.object({
    type: z.literal("job_progress"),
    job_id: z.string(),
    stage: z.string(),
    detail: z.string().nullable().optional(),
    percent: z.number().int().min(0).max(100).nullable().optional(),
  }),
  z.object({
    type: z.literal("job_completed"),
    job_id: z.string(),
    repository_id: z.string(),
    duration_ms: z.number().int().nullable().optional(),
  }),
  z.object({
    type: z.literal("job_failed"),
    job_id: z.string(),
    repository_id: z.string(),
    error_code: z.string(),
    message: z.string(),
    will_retry: z.boolean(),
    next_attempt: z.number().int().nullable().optional(),
  }),
  z.object({
    type: z.literal("job_cancelled"),
    job_id: z.string(),
    repository_id: z.string(),
  }),
  z.object({
    type: z.literal("job_warning"),
    job_id: z.string(),
    repository_id: z.string(),
    message: z.string(),
  }),
]);
