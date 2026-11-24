/** Typed Tauri `invoke` wrapper used by the desktop frontend.
 *  Every command result is validated with zod before it is used.
 *
 *  Web-preview guard: the app also runs as a plain Vite dev server in
 *  a browser (localhost:1420), where there is no Tauri backend.
 *  `invoke` would then throw the cryptic `Cannot read properties of
 *  undefined (reading 'invoke')` because `window.__TAURI_INTERNALS__`
 *  does not exist. Every call goes through `invokeCommand`, which
 *  detects that case first and throws a clear, actionable error. */

import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import {
  accountSchema,
  agentHealthSchema,
  appInfoSchema,
  brokerInstallationSchema,
  brokerStatusSchema,
  configExportSchema,
  dashboardCountsSchema,
  databaseBackupSchema,
  diagnosticsExportSchema,
  discoveryOutcomeSchema,
  effectiveScheduleSchema,
  gitStatusSchema,
  importReportSchema,
  jobStatusCountSchema,
  materializeReportSchema,
  missingArchiveSchema,
  moveArchiveResultSchema,
  nextRunPreviewSchema,
  orphanArchiveSchema,
  pairingInfoSchema,
  patExpirySchema,
  providerAccountInfoSchema,
  renamedArchiveSchema,
  repositoryPageSchema,
  repositorySchema,
  restoreStagedSchema,
  scheduleWithNextRunSchema,
  settingsSchema,
  storageOverviewSchema,
  syncHistorySchema,
  syncJobSchema,
} from "@reponyx/shared";
import type {
  Account,
  AgentHealth,
  AppInfo,
  BrokerInstallation,
  BrokerStatus,
  ConfigExport,
  DashboardCounts,
  DatabaseBackup,
  DiagnosticsExport,
  DiscoveryOutcome,
  EffectiveSchedule,
  GitStatus,
  ImportReport,
  JobStatus,
  JobStatusCount,
  MaterializeReport,
  MoveArchiveResult,
  NewScheduleInput,
  NextRunPreview,
  PairingInfo,
  PatExpiry,
  ProviderAccountInfo,
  Repository,
  RepositoryPage,
  RestoreStaged,
  ScheduleWithNextRun,
  Settings,
  StorageOverview,
  SyncHistory,
  SyncJob,
} from "@reponyx/shared";

export async function getAppInfo(): Promise<AppInfo> {
  return appInfoSchema.parse(await invokeCommand("get_app_info"));
}

/** True only inside the Tauri webview, where the backend exists. */
export function isTauriAvailable(): boolean {
  if (typeof window === "undefined") return false;
  const internals = (
    window as unknown as { __TAURI_INTERNALS__?: unknown }
  ).__TAURI_INTERNALS__;
  return internals !== undefined && internals !== null;
}

/** Call a Tauri command, or fail with a clear message when the page
 *  is running in a plain browser (web preview) instead of the
 *  desktop app. Without this guard, `invoke` throws the cryptic
 *  `Cannot read properties of undefined (reading 'invoke')`. */
async function invokeCommand(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<unknown> {
  if (!isTauriAvailable()) {
    throw new Error(
      "Tauri backend is unavailable — this page is running in a plain browser (web preview) instead of the RepoNyx desktop app",
    );
  }
  return invoke(cmd, args);
}

export async function getGitStatus(): Promise<GitStatus> {
  return gitStatusSchema.parse(await invokeCommand("get_git_status"));
}

export async function getDashboardCounts(): Promise<DashboardCounts> {
  return dashboardCountsSchema.parse(await invokeCommand("get_dashboard_counts"));
}

/** Phase 5 auth commands (Tauri `auth.rs`). Invoke argument names
 *  are camelCase, matching Tauri's default argument case (the Rust parameter names are snake_case). */

export async function listAccounts(): Promise<Account[]> {
  return z.array(accountSchema).parse(await invokeCommand("list_accounts_command"));
}

export async function addPatAccount(input: {
  pat: string;
  label?: string | null;
  accountType: "personal" | "organization";
  expiresAt?: string | null;
}): Promise<Account> {
  return accountSchema.parse(await invokeCommand("add_pat_account_command", input));
}

export async function replacePatAccount(input: {
  accountId: string;
  pat: string;
  expiresAt?: string | null;
}): Promise<Account> {
  return accountSchema.parse(
    await invokeCommand("replace_pat_account_command", input),
  );
}

export async function removePatAccount(accountId: string): Promise<Account> {
  return accountSchema.parse(
    await invokeCommand("remove_pat_account_command", { accountId }),
  );
}

export async function reconnectPatAccount(accountId: string): Promise<Account> {
  return accountSchema.parse(
    await invokeCommand("reconnect_pat_account_command", { accountId }),
  );
}

export async function validatePatAccount(
  accountId: string,
): Promise<ProviderAccountInfo> {
  return providerAccountInfoSchema.parse(
    await invokeCommand("validate_pat_account_command", { accountId }),
  );
}

export async function validateGithubAppAccount(
  accountId: string,
): Promise<ProviderAccountInfo> {
  return providerAccountInfoSchema.parse(
    await invokeCommand("validate_github_app_account_command", {
      accountId,
    }),
  );
}

export async function getPatExpiryStatus(
  accountId: string,
): Promise<PatExpiry> {
  return patExpirySchema.parse(
    await invokeCommand("get_pat_expiry_status_command", { accountId }),
  );
}

export async function setBrokerSetupToken(token: string): Promise<void> {
  await invokeCommand("set_broker_setup_token_command", { token });
}

export async function getBrokerStatus(): Promise<BrokerStatus> {
  return brokerStatusSchema.parse(await invokeCommand("get_broker_status_command"));
}

export async function listBrokerInstallations(): Promise<BrokerInstallation[]> {
  return z
    .array(brokerInstallationSchema)
    .parse(await invokeCommand("list_broker_installations_command"));
}

export async function startPairing(
  installationId: string,
): Promise<PairingInfo> {
  return pairingInfoSchema.parse(
    await invokeCommand("start_pairing_command", { installationId }),
  );
}

export async function completePairing(
  pairingCode: string,
): Promise<Account> {
  return accountSchema.parse(
    await invokeCommand("complete_pairing_command", { pairingCode }),
  );
}

export async function revokeDevicePairing(
  deviceIdentifier: string,
): Promise<boolean> {
  return z
    .boolean()
    .parse(
      await invokeCommand("revoke_device_pairing_command", {
        deviceIdentifier,
      }),
    );
}

/** Phase 7 scheduler commands (Tauri `schedules.rs`). Invoke
 *  argument names are camelCase, matching Tauri's default argument case (the Rust parameter names are snake_case).
 *  The Schedules page (`apps/desktop/src/routes/Schedules.tsx`,
 *  Phase 8) is the typed UI for these calls. */

export async function listSchedules(): Promise<ScheduleWithNextRun[]> {
  return z
    .array(scheduleWithNextRunSchema)
    .parse(await invokeCommand("list_schedules_command"));
}

export async function createSchedule(
  input: NewScheduleInput,
): Promise<ScheduleWithNextRun> {
  return scheduleWithNextRunSchema.parse(
    await invokeCommand("create_schedule_command", {
      scope: input.scope,
      accountId: input.accountId ?? null,
      repositoryId: input.repositoryId ?? null,
      intervalType: input.intervalType,
      intervalValue: input.intervalValue ?? null,
      timezone: input.timezone,
      enabled: input.enabled,
    }),
  );
}

export async function updateSchedule(input: {
  id: string;
  intervalType?: string | null;
  intervalValue?: string | null;
  timezone?: string | null;
  enabled?: boolean | null;
}): Promise<ScheduleWithNextRun> {
  return scheduleWithNextRunSchema.parse(
    await invokeCommand("update_schedule_command", {
      id: input.id,
      intervalType: input.intervalType ?? null,
      intervalValue: input.intervalValue ?? null,
      timezone: input.timezone ?? null,
      enabled: input.enabled ?? null,
    }),
  );
}

export async function deleteSchedule(id: string): Promise<boolean> {
  return z.boolean().parse(await invokeCommand("delete_schedule_command", { id }));
}

export async function previewNextRuns(input: {
  scheduleId: string;
  count: number;
  displayTimezone?: string | null;
}): Promise<NextRunPreview[]> {
  return z.array(nextRunPreviewSchema).parse(
    await invokeCommand("preview_next_runs_command", {
      scheduleId: input.scheduleId,
      count: input.count,
      displayTimezone: input.displayTimezone ?? null,
    }),
  );
}

export async function materializeDue(): Promise<MaterializeReport> {
  return materializeReportSchema.parse(
    await invokeCommand("materialize_due_command"),
  );
}

export async function effectiveSchedule(
  repositoryId: string,
): Promise<EffectiveSchedule | null> {
  return effectiveScheduleSchema.nullable().parse(
    await invokeCommand("effective_schedule_command", {
      repositoryId,
    }),
  );
}

/** Phase 8 repository commands (Tauri `repositories.rs`). Invoke
 *  argument names are camelCase, matching Tauri's default argument case (the Rust parameter names are snake_case).
 *  The Repositories page (`apps/desktop/src/routes/Repositories.tsx`)
 *  is the typed UI for these calls. */

export async function listRepositories(input?: {
  accountId?: string | null;
  archiveStatus?: string | null;
  syncEnabled?: boolean | null;
  search?: string | null;
  limit?: number | null;
  offset?: number | null;
}): Promise<RepositoryPage> {
  return repositoryPageSchema.parse(
    await invokeCommand("list_repositories_command", {
      accountId: input?.accountId ?? null,
      archiveStatus: input?.archiveStatus ?? null,
      syncEnabled: input?.syncEnabled ?? null,
      search: input?.search ?? null,
      limit: input?.limit ?? null,
      offset: input?.offset ?? null,
    }),
  );
}

/** Full names for the repositories actually on screen, keyed by
 *  id. The Synchronization and Activity pages show rows from other
 *  tables and only need these names — they no longer load the
 *  whole repository table for a lookup. */
export async function getRepositoryNames(
  ids: string[],
): Promise<Record<string, string>> {
  return z.record(z.string(), z.string()).parse(
    await invokeCommand("get_repository_names_command", { ids }),
  );
}

export async function discoverRepositories(
  accountId: string,
): Promise<DiscoveryOutcome> {
  return discoveryOutcomeSchema.parse(
    await invokeCommand("discover_repositories_command", {
      accountId,
    }),
  );
}

export async function updateRepository(input: {
  id: string;
  syncEnabled?: boolean | null;
  syncMode?: string | null;
}): Promise<Repository> {
  return repositorySchema.parse(
    await invokeCommand("update_repository_command", {
      id: input.id,
      syncEnabled: input.syncEnabled ?? null,
      syncMode: input.syncMode ?? null,
    }),
  );
}

export async function deleteRepository(id: string): Promise<boolean> {
  return z
    .boolean()
    .parse(await invokeCommand("delete_repository_command", { id }));
}

export async function openRepositoryLocation(id: string): Promise<void> {
  await invokeCommand("open_repository_location_command", { id });
}

export async function openRepositoryInBrowser(id: string): Promise<void> {
  await invokeCommand("open_repository_in_browser_command", { id });
}

/** Phase 8 synchronization commands (Tauri `synchronization.rs`).
 *  Job rows are read straight from SQLite, so this page works
 *  whether or not the background agent is running. */

export async function listSyncJobs(input?: {
  status?: string | null;
  repositoryId?: string | null;
  limit?: number | null;
}): Promise<SyncJob[]> {
  return z
    .array(syncJobSchema)
    .parse(
      await invokeCommand("list_sync_jobs_command", {
        status: input?.status ?? null,
        repositoryId: input?.repositoryId ?? null,
        limit: input?.limit ?? null,
      }),
    );
}

export async function countSyncJobs(): Promise<JobStatusCount[]> {
  return z
    .array(jobStatusCountSchema)
    .parse(await invokeCommand("count_sync_jobs_command"));
}

export async function cancelSyncJob(id: string): Promise<SyncJob> {
  return syncJobSchema.parse(
    await invokeCommand("cancel_sync_job_command", { id }),
  );
}

export async function listRecentHistory(
  limit?: number | null,
): Promise<SyncHistory[]> {
  return z
    .array(syncHistorySchema)
    .parse(
      await invokeCommand("list_recent_history_command", {
        limit: limit ?? null,
      }),
    );
}

export async function listHistoryForRepository(
  repositoryId: string,
  limit?: number | null,
): Promise<SyncHistory[]> {
  return z
    .array(syncHistorySchema)
    .parse(
      await invokeCommand("list_history_for_repository_command", {
        repositoryId,
        limit: limit ?? null,
      }),
    );
}

/** Phase 8 Storage commands (Tauri `storage.rs`). Sizes are
 *  computed on demand by the backend, so a first load can take a
 *  moment on a large archive. */

export async function getStorageOverview(): Promise<StorageOverview> {
  return storageOverviewSchema.parse(
    await invokeCommand("get_storage_overview_command"),
  );
}

export async function openStorageRoot(): Promise<void> {
  await invokeCommand("open_storage_root_command");
}

/** Phase 9 Storage management (Tauri `storage.rs`). Each delete
 *  is re-verified server-side before anything is removed; the
 *  number returned is the bytes freed. */
export async function deleteOrphan(path: string): Promise<number> {
  return z
    .number()
    .int()
    .nonnegative()
    .parse(await invokeCommand("delete_orphan_command", { path }));
}

export async function deleteLocalArchive(
  repositoryId: string,
): Promise<number> {
  return z
    .number()
    .int()
    .nonnegative()
    .parse(
      await invokeCommand("delete_local_archive_command", {
        repositoryId,
      }),
    );
}

export async function moveArchiveForRename(
  repositoryId: string,
): Promise<MoveArchiveResult> {
  return moveArchiveResultSchema.parse(
    await invokeCommand("move_archive_for_rename_command", {
      repositoryId,
    }),
  );
}

/** Phase 8 Settings commands (Tauri `settings.rs`). Only the four
 *  keys the Settings page offers are writable; the backend
 *  validates each value before storing it. */

export async function getSettings(): Promise<Settings> {
  return settingsSchema.parse(await invokeCommand("get_settings_command"));
}

export async function setSetting(key: string, value: string): Promise<string> {
  return z
    .string()
    .parse(await invokeCommand("set_setting_command", { key, value }));
}

/** Phase 9 diagnostics + database backup commands (Tauri
 *  `diagnostics.rs`). The export is a redacted JSON file with no
 *  credentials; a restore is staged and applied at the next
 *  startup, after the agent has been stopped. */
export async function exportDiagnostics(): Promise<DiagnosticsExport> {
  return diagnosticsExportSchema.parse(
    await invokeCommand("export_diagnostics_command"),
  );
}

export async function createDatabaseBackup(): Promise<DatabaseBackup> {
  return databaseBackupSchema.parse(
    await invokeCommand("create_database_backup_command"),
  );
}

export async function listDatabaseBackups(): Promise<DatabaseBackup[]> {
  return z
    .array(databaseBackupSchema)
    .parse(await invokeCommand("list_database_backups_command"));
}

export async function deleteDatabaseBackup(path: string): Promise<void> {
  await invokeCommand("delete_database_backup_command", { path });
}

export async function restoreDatabaseBackup(
  path: string,
): Promise<RestoreStaged> {
  return restoreStagedSchema.parse(
    await invokeCommand("restore_database_backup_command", { path }),
  );
}

export async function openSupportFolder(
  folder: "diagnostics" | "backups",
): Promise<void> {
  await invokeCommand("open_support_folder_command", { folder });
}

/** Phase 9 config export. Opens a Save dialog; `null` when the
 *  user cancels it. */
export async function exportConfig(): Promise<ConfigExport | null> {
  return configExportSchema
    .nullable()
    .parse(await invokeCommand("export_config_command"));
}

/** Phase 9 config import. Opens an Open dialog; `null` when the
 *  user cancels it. */
export async function importConfig(): Promise<ImportReport | null> {
  return importReportSchema
    .nullable()
    .parse(await invokeCommand("import_config_command"));
}

/** Phase 7 background-agent commands (Tauri `agent.rs`). The agent
 *  process is separate from the UI; these calls go over IPC, so a
 *  missing agent surfaces as a real error, never fabricated health. */

export async function agentHealth(): Promise<AgentHealth> {
  return agentHealthSchema.parse(await invokeCommand("agent_health_command"));
}

export async function agentPing(): Promise<[number, number]> {
  return z
    .tuple([z.number().int().nonnegative(), z.number().int().nonnegative()])
    .parse(await invokeCommand("agent_ping_command"));
}

export async function agentPause(): Promise<void> {
  await invokeCommand("agent_pause_command");
}

export async function agentResume(): Promise<void> {
  await invokeCommand("agent_resume_command");
}

export async function agentEnqueue(
  repositoryId: string,
): Promise<string | null> {
  return z
    .string()
    .nullable()
    .parse(
      await invokeCommand("agent_enqueue_command", {
        repositoryId,
      }),
    );
}

export async function agentCancelJob(jobId: string): Promise<void> {
  await invokeCommand("agent_cancel_command", { jobId });
}

export async function ensureAgentRunning(): Promise<AgentHealth> {
  return agentHealthSchema.parse(
    await invokeCommand("ensure_agent_running_command"),
  );
}

export async function registerAgentAutostart(): Promise<string> {
  return z
    .string()
    .parse(await invokeCommand("register_agent_autostart_command"));
}

export async function unregisterAgentAutostart(): Promise<string> {
  return z
    .string()
    .parse(await invokeCommand("unregister_agent_autostart_command"));
}
