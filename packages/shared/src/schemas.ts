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
