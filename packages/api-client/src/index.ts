/** Typed Tauri `invoke` wrapper used by the desktop frontend.
 *  Every command result is validated with zod before it is used. */

import { invoke } from "@tauri-apps/api/core";
import {
  appInfoSchema,
  dashboardCountsSchema,
  gitStatusSchema,
} from "@reponyx/shared";
import type { AppInfo, DashboardCounts, GitStatus } from "@reponyx/shared";

export async function getAppInfo(): Promise<AppInfo> {
  return appInfoSchema.parse(await invoke("get_app_info"));
}

export async function getGitStatus(): Promise<GitStatus> {
  return gitStatusSchema.parse(await invoke("get_git_status"));
}

export async function getDashboardCounts(): Promise<DashboardCounts> {
  return dashboardCountsSchema.parse(await invoke("get_dashboard_counts"));
}
