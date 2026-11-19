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
