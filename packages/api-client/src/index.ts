/** Typed Tauri `invoke` wrapper used by the desktop frontend.
 *  Every command result is validated with zod before it is used. */

import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import {
  accountSchema,
  appInfoSchema,
  brokerInstallationSchema,
  brokerStatusSchema,
  dashboardCountsSchema,
  effectiveScheduleSchema,
  gitStatusSchema,
  materializeReportSchema,
  nextRunPreviewSchema,
  pairingInfoSchema,
  patExpirySchema,
  providerAccountInfoSchema,
  scheduleWithNextRunSchema,
} from "@reponyx/shared";
import type {
  Account,
  AppInfo,
  BrokerInstallation,
  BrokerStatus,
  DashboardCounts,
  EffectiveSchedule,
  GitStatus,
  MaterializeReport,
  NewScheduleInput,
  NextRunPreview,
  PairingInfo,
  PatExpiry,
  ProviderAccountInfo,
  ScheduleWithNextRun,
} from "@reponyx/shared";

export async function getAppInfo(): Promise<AppInfo> {
  return appInfoSchema.parse(await invoke("get_app_info"));
}

export async function getGitStatus(): Promise<GitStatus> {
  return gitStatusSchema.parse(await invoke("get_git_status"));
}

export async function getDashboardCounts(): Promise<DashboardCounts> {
  return dashboardCountsSchema.parse(await invoke("get_dashboard_counts"));
}

/** Phase 5 auth commands (Tauri `auth.rs`). Invoke argument names
 *  are snake_case, matching the Rust parameter names. */

export async function listAccounts(): Promise<Account[]> {
  return z.array(accountSchema).parse(await invoke("list_accounts_command"));
}

export async function addPatAccount(input: {
  pat: string;
  label?: string | null;
  account_type: "personal" | "organization";
  expires_at?: string | null;
}): Promise<Account> {
  return accountSchema.parse(await invoke("add_pat_account_command", input));
}

export async function replacePatAccount(input: {
  account_id: string;
  pat: string;
  expires_at?: string | null;
}): Promise<Account> {
  return accountSchema.parse(
    await invoke("replace_pat_account_command", input),
  );
}

export async function removePatAccount(accountId: string): Promise<Account> {
  return accountSchema.parse(
    await invoke("remove_pat_account_command", { account_id: accountId }),
  );
}

export async function reconnectPatAccount(accountId: string): Promise<Account> {
  return accountSchema.parse(
    await invoke("reconnect_pat_account_command", { account_id: accountId }),
  );
}

export async function validatePatAccount(
  accountId: string,
): Promise<ProviderAccountInfo> {
  return providerAccountInfoSchema.parse(
    await invoke("validate_pat_account_command", { account_id: accountId }),
  );
}

export async function validateGithubAppAccount(
  accountId: string,
): Promise<ProviderAccountInfo> {
  return providerAccountInfoSchema.parse(
    await invoke("validate_github_app_account_command", {
      account_id: accountId,
    }),
  );
}

export async function getPatExpiryStatus(
  accountId: string,
): Promise<PatExpiry> {
  return patExpirySchema.parse(
    await invoke("get_pat_expiry_status_command", { account_id: accountId }),
  );
}

export async function setBrokerSetupToken(token: string): Promise<void> {
  await invoke("set_broker_setup_token_command", { token });
}

export async function getBrokerStatus(): Promise<BrokerStatus> {
  return brokerStatusSchema.parse(await invoke("get_broker_status_command"));
}

export async function listBrokerInstallations(): Promise<BrokerInstallation[]> {
  return z
    .array(brokerInstallationSchema)
    .parse(await invoke("list_broker_installations_command"));
}

export async function startPairing(
  installationId: string,
): Promise<PairingInfo> {
  return pairingInfoSchema.parse(
    await invoke("start_pairing_command", { installation_id: installationId }),
  );
}

export async function completePairing(
  pairingCode: string,
): Promise<Account> {
  return accountSchema.parse(
    await invoke("complete_pairing_command", { pairing_code: pairingCode }),
  );
}

export async function revokeDevicePairing(
  deviceIdentifier: string,
): Promise<boolean> {
  return z
    .boolean()
    .parse(
      await invoke("revoke_device_pairing_command", {
        device_identifier: deviceIdentifier,
      }),
    );
}

/** Phase 7 scheduler commands (Tauri `schedules.rs`). Invoke
 *  argument names are snake_case, matching the Rust parameter names.
 *  No scheduler UI exists yet (Phase 8); these are the typed calls
 *  it will use. */

export async function listSchedules(): Promise<ScheduleWithNextRun[]> {
  return z
    .array(scheduleWithNextRunSchema)
    .parse(await invoke("list_schedules_command"));
}

export async function createSchedule(
  input: NewScheduleInput,
): Promise<ScheduleWithNextRun> {
  return scheduleWithNextRunSchema.parse(
    await invoke("create_schedule_command", {
      scope: input.scope,
      account_id: input.accountId ?? null,
      repository_id: input.repositoryId ?? null,
      interval_type: input.intervalType,
      interval_value: input.intervalValue ?? null,
      timezone: input.timezone,
      enabled: input.enabled,
    }),
  );
}

export async function updateSchedule(input: {
  id: string;
  interval_type?: string | null;
  interval_value?: string | null;
  timezone?: string | null;
  enabled?: boolean | null;
}): Promise<ScheduleWithNextRun> {
  return scheduleWithNextRunSchema.parse(
    await invoke("update_schedule_command", {
      id: input.id,
      interval_type: input.interval_type ?? null,
      interval_value: input.interval_value ?? null,
      timezone: input.timezone ?? null,
      enabled: input.enabled ?? null,
    }),
  );
}

export async function deleteSchedule(id: string): Promise<boolean> {
  return z.boolean().parse(await invoke("delete_schedule_command", { id }));
}

export async function previewNextRuns(input: {
  schedule_id: string;
  count: number;
  display_timezone?: string | null;
}): Promise<NextRunPreview[]> {
  return z.array(nextRunPreviewSchema).parse(
    await invoke("preview_next_runs_command", {
      schedule_id: input.schedule_id,
      count: input.count,
      display_timezone: input.display_timezone ?? null,
    }),
  );
}

export async function materializeDue(): Promise<MaterializeReport> {
  return materializeReportSchema.parse(
    await invoke("materialize_due_command"),
  );
}

export async function effectiveSchedule(
  repositoryId: string,
): Promise<EffectiveSchedule | null> {
  return effectiveScheduleSchema.nullable().parse(
    await invoke("effective_schedule_command", {
      repository_id: repositoryId,
    }),
  );
}
