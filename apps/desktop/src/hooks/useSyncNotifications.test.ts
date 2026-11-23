import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import type { Settings, SyncJob } from "@reponyx/shared";
import { useSyncNotifications } from "./useSyncNotifications";

vi.mock("@reponyx/api-client", () => ({
  getRepositoryNames: vi.fn(),
  getSettings: vi.fn(),
  listSyncJobs: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn(),
}));

import {
  getRepositoryNames,
  getSettings,
  listSyncJobs,
} from "@reponyx/api-client";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

const settings = {
  defaultSyncMode: "mirror",
  concurrency: 5,
  storageRoot: null,
  theme: "system",
  notificationsEnabled: true,
  deviceIdentifier: null,
  dataDir: "/tmp/reponyx",
  appVersion: "0.1.0",
} as Settings;

function job(overrides: Partial<SyncJob>): SyncJob {
  return {
    id: "job-1",
    repositoryId: "repo-1",
    jobType: "mirror_clone",
    status: "completed",
    priority: 0,
    attemptCount: 0,
    startedAt: null,
    completedAt: "2026-11-23T11:00:00Z",
    errorCode: null,
    errorMessage: null,
    createdAt: "2026-11-23T10:00:00Z",
    availableAt: null,
    workerId: null,
    leaseExpiresAt: null,
    heartbeatAt: null,
    cancelRequestedAt: null,
    dedupeKey: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  vi.mocked(getSettings).mockResolvedValue(settings);
  vi.mocked(isPermissionGranted).mockResolvedValue(true);
  vi.mocked(requestPermission).mockResolvedValue("granted");
  vi.mocked(getRepositoryNames).mockResolvedValue({
    "repo-1": "muhammaddomains/reponyx",
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useSyncNotifications", () => {
  it("seeds silently, then notifies for a newly finished job", async () => {
    vi.mocked(listSyncJobs).mockResolvedValue([job({ id: "old" })]);
    const { unmount } = renderHook(() => useSyncNotifications());
    // Flush the effect's async start (settings + permission + seed poll).
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.mocked(listSyncJobs)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(sendNotification)).not.toHaveBeenCalled();

    vi.mocked(listSyncJobs).mockResolvedValue([
      job({ id: "old" }),
      job({ id: "new", status: "completed" }),
    ]);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(vi.mocked(sendNotification)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(sendNotification)).toHaveBeenCalledWith({
      title: "Sync finished",
      body: "Mirror clone finished for muhammaddomains/reponyx.",
    });
    unmount();
  });

  it("reports failures with the error message", async () => {
    vi.mocked(listSyncJobs).mockResolvedValue([]);
    const { unmount } = renderHook(() => useSyncNotifications());
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.mocked(listSyncJobs)).toHaveBeenCalledTimes(1);
    vi.mocked(listSyncJobs).mockResolvedValue([
      job({
        id: "bad",
        status: "failed",
        errorCode: "clone_failed",
        errorMessage: "authentication failed",
      }),
    ]);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(vi.mocked(sendNotification)).toHaveBeenCalledWith({
      title: "Sync failed",
      body: "Mirror clone failed for muhammaddomains/reponyx — authentication failed",
    });
    unmount();
  });

  it("stays quiet when notifications are disabled", async () => {
    vi.mocked(getSettings).mockResolvedValue({
      ...settings,
      notificationsEnabled: false,
    });
    vi.mocked(listSyncJobs).mockResolvedValue([job({ id: "new" })]);
    const { unmount } = renderHook(() => useSyncNotifications());
    await vi.advanceTimersByTimeAsync(90_000);
    expect(vi.mocked(sendNotification)).not.toHaveBeenCalled();
    unmount();
  });

  it("asks for permission once and stops when it is denied", async () => {
    vi.mocked(isPermissionGranted).mockResolvedValue(false);
    vi.mocked(requestPermission).mockResolvedValue("denied");
    vi.mocked(listSyncJobs).mockResolvedValue([job({ id: "new" })]);
    const { unmount } = renderHook(() => useSyncNotifications());
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.mocked(requestPermission)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(listSyncJobs)).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(90_000);
    expect(vi.mocked(listSyncJobs)).not.toHaveBeenCalled();
    expect(vi.mocked(sendNotification)).not.toHaveBeenCalled();
    unmount();
  });

  it("never notifies twice for the same job", async () => {
    vi.mocked(listSyncJobs).mockResolvedValue([job({ id: "once" })]);
    const { unmount } = renderHook(() => useSyncNotifications());
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.mocked(listSyncJobs)).toHaveBeenCalledTimes(1);
    // The first poll seeds; the next two polls see the same job.
    await vi.advanceTimersByTimeAsync(30_000);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(vi.mocked(sendNotification)).not.toHaveBeenCalled();
    unmount();
  });
});
