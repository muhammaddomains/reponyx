import { describe, expect, it } from "vitest";
import {
  dashboardCountsSchema,
  repositorySchema,
  scheduleSchema,
  settingsSchema,
  syncJobSchema,
} from "@reponyx/shared";

/** Zod validation of the payloads arriving over the Tauri invoke
 *  boundary. A bad payload must fail loudly at the boundary, never
 *  render as a half-filled page. */

const validRepository = {
  id: "11111111-1111-1111-1111-111111111111",
  accountId: "22222222-2222-2222-2222-222222222222",
  providerRepositoryId: "123456",
  owner: "muhammaddomains",
  name: "reponyx",
  fullName: "muhammaddomains/reponyx",
  visibility: "public",
  defaultBranch: "main",
  cloneUrl: "https://github.com/muhammaddomains/reponyx.git",
  archiveStatus: "synced",
  syncEnabled: true,
  syncMode: "mirror",
  localPath: null,
  remoteUpdatedAt: null,
  lastSyncedAt: null,
  createdAt: "2026-11-23T00:00:00Z",
  updatedAt: "2026-11-23T00:00:00Z",
};

describe("repositorySchema", () => {
  it("accepts a complete repository", () => {
    expect(repositorySchema.safeParse(validRepository).success).toBe(true);
  });

  it("rejects an unknown visibility", () => {
    const parsed = repositorySchema.safeParse({
      ...validRepository,
      visibility: "secret",
    });
    expect(parsed.success).toBe(false);
  });

  it("rejects an unknown archive status", () => {
    const parsed = repositorySchema.safeParse({
      ...validRepository,
      archiveStatus: "archived",
    });
    expect(parsed.success).toBe(false);
  });

  it("rejects a missing clone URL", () => {
    expect(
      repositorySchema.safeParse({ ...validRepository, cloneUrl: undefined })
        .success,
    ).toBe(false);
  });
});

describe("scheduleSchema", () => {
  const validSchedule = {
    id: "33333333-3333-3333-3333-333333333333",
    scope: "global",
    accountId: null,
    repositoryId: null,
    intervalType: "daily",
    intervalValue: "09:00",
    timezone: "America/Phoenix",
    enabled: true,
    nextRunAt: null,
    createdAt: "2026-11-23T00:00:00Z",
    updatedAt: "2026-11-23T00:00:00Z",
  };

  it("accepts a global daily schedule", () => {
    expect(scheduleSchema.safeParse(validSchedule).success).toBe(true);
  });

  it("rejects an unknown scope or interval type", () => {
    expect(
      scheduleSchema.safeParse({ ...validSchedule, scope: "machine" })
        .success,
    ).toBe(false);
    expect(
      scheduleSchema.safeParse({
        ...validSchedule,
        intervalType: "hourly",
      }).success,
    ).toBe(false);
  });
});

describe("syncJobSchema", () => {
  const validJob = {
    id: "44444444-4444-4444-4444-444444444444",
    repositoryId: "11111111-1111-1111-1111-111111111111",
    jobType: "mirror_clone",
    status: "pending",
    priority: 0,
    attemptCount: 0,
    startedAt: null,
    completedAt: null,
    errorCode: null,
    errorMessage: null,
    createdAt: "2026-11-23T00:00:00Z",
    availableAt: null,
    workerId: null,
    leaseExpiresAt: null,
    heartbeatAt: null,
    cancelRequestedAt: null,
    dedupeKey: null,
  };

  it("accepts a pending mirror-clone job", () => {
    expect(syncJobSchema.safeParse(validJob).success).toBe(true);
  });

  it("rejects a negative attempt count", () => {
    expect(
      syncJobSchema.safeParse({ ...validJob, attemptCount: -1 }).success,
    ).toBe(false);
  });

  it("rejects an unknown job status", () => {
    expect(
      syncJobSchema.safeParse({ ...validJob, status: "stuck" }).success,
    ).toBe(false);
  });
});

describe("dashboardCountsSchema", () => {
  it("accepts zeros", () => {
    expect(
      dashboardCountsSchema.safeParse({
        accounts: 0,
        repositories: 0,
        synced: 0,
        syncing: 0,
        failed: 0,
        pending_jobs: 0,
      }).success,
    ).toBe(true);
  });

  it("rejects negative counts", () => {
    expect(
      dashboardCountsSchema.safeParse({
        accounts: 1,
        repositories: -1,
        synced: 0,
        syncing: 0,
        failed: 0,
        pending_jobs: 0,
      }).success,
    ).toBe(false);
  });
});

describe("settingsSchema", () => {
  it("accepts nulls for every unset setting", () => {
    expect(
      settingsSchema.safeParse({
        defaultSyncMode: null,
        concurrency: null,
        storageRoot: null,
        theme: null,
        locale: null,
        notificationsEnabled: null,
        deviceIdentifier: null,
        brokerAppId: null,
        brokerPrivateKeyPath: null,
        dataDir: "/tmp/reponyx",
        appVersion: "0.1.0",
      }).success,
    ).toBe(true);
  });

  it("rejects a non-integer concurrency", () => {
    expect(
      settingsSchema.safeParse({
        defaultSyncMode: null,
        concurrency: 2.5,
        storageRoot: null,
        theme: null,
        locale: null,
        notificationsEnabled: true,
        deviceIdentifier: null,
        brokerAppId: null,
        brokerPrivateKeyPath: null,
        dataDir: "/tmp/reponyx",
        appVersion: "0.1.0",
      }).success,
    ).toBe(false);
  });

  it("rejects a non-boolean notifications flag", () => {
    expect(
      settingsSchema.safeParse({
        defaultSyncMode: null,
        concurrency: null,
        storageRoot: null,
        theme: null,
        locale: null,
        notificationsEnabled: "yes",
        deviceIdentifier: null,
        brokerAppId: null,
        brokerPrivateKeyPath: null,
        dataDir: "/tmp/reponyx",
        appVersion: "0.1.0",
      }).success,
    ).toBe(false);
  });
});
