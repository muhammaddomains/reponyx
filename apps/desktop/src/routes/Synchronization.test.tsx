import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHealth, SyncJob } from "@reponyx/shared";
import Synchronization from "./Synchronization";

const { progressListeners } = vi.hoisted(() => ({
  progressListeners: [] as Array<(event: { payload: unknown }) => void>,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    (_eventName: string, handler: (event: { payload: unknown }) => void) => {
      progressListeners.push(handler);
      return Promise.resolve(() => {});
    },
  ),
}));

vi.mock("@reponyx/api-client", () => ({
  agentEnqueue: vi.fn(),
  agentHealth: vi.fn(),
  agentPause: vi.fn(),
  agentResume: vi.fn(),
  cancelSyncJob: vi.fn(),
  countSyncJobs: vi.fn(),
  ensureAgentRunning: vi.fn(),
  getRepositoryNames: vi.fn(),
  listSyncJobs: vi.fn(),
}));

import {
  agentHealth,
  countSyncJobs,
  getRepositoryNames,
  listSyncJobs,
} from "@reponyx/api-client";

function job(overrides: Partial<SyncJob> & { id: string }): SyncJob {
  return {
    repositoryId: "repo-1",
    jobType: "fetch",
    status: "running",
    priority: 0,
    attemptCount: 1,
    startedAt: "2026-11-23T10:00:00Z",
    completedAt: null,
    errorCode: null,
    errorMessage: null,
    createdAt: "2026-11-23T10:00:00Z",
    availableAt: null,
    workerId: "worker-1",
    leaseExpiresAt: null,
    heartbeatAt: null,
    cancelRequestedAt: null,
    dedupeKey: null,
    ...overrides,
  };
}

const healthyAgent: AgentHealth = {
  reachable: true,
  status: {
    generation: 1,
    startedAt: "2026-11-23T09:00:00Z",
    protocolVersion: 1,
    paused: false,
    pendingJobs: 0,
    runningJobs: 1,
    workers: 2,
    lastMaterializeAt: null,
    lastMaterializeCreated: null,
  },
  message: "ok",
};

function emitProgress(payload: unknown) {
  act(() => {
    for (const listener of progressListeners) {
      listener({ payload });
    }
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  progressListeners.length = 0;
  vi.mocked(agentHealth).mockResolvedValue(healthyAgent);
  vi.mocked(countSyncJobs).mockResolvedValue([]);
  vi.mocked(getRepositoryNames).mockResolvedValue({ "repo-1": "owner/repo" });
  vi.mocked(listSyncJobs).mockImplementation(async (filter) => {
    if (filter?.status === "running") return [job({ id: "job-running" })];
    return [];
  });
});

describe("Synchronization page (live progress bars)", () => {
  it("shows a determinate bar from a job_progress event", async () => {
    render(<Synchronization />);
    expect(await screen.findByText("owner/repo")).toBeInTheDocument();

    emitProgress({
      type: "job_progress",
      job_id: "job-running",
      stage: "receiving objects",
      detail: "Receiving objects: 45%",
      percent: 45,
    });

    const bar = await screen.findByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "45");
    expect(screen.getByText("45%")).toBeInTheDocument();
    expect(screen.getByText(/receiving objects/)).toBeInTheDocument();
  });

  it("shows an indeterminate bar when git reports no percent", async () => {
    render(<Synchronization />);
    expect(await screen.findByText("owner/repo")).toBeInTheDocument();

    emitProgress({
      type: "job_progress",
      job_id: "job-running",
      stage: "counting objects",
      detail: "Counting objects: 12",
    });

    const bar = await screen.findByRole("progressbar");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByText(/counting objects/)).toBeInTheDocument();
  });

  it("clears the bar when the job completes", async () => {
    render(<Synchronization />);
    expect(await screen.findByText("owner/repo")).toBeInTheDocument();

    emitProgress({
      type: "job_progress",
      job_id: "job-running",
      stage: "receiving objects",
      percent: 80,
    });
    expect(await screen.findByRole("progressbar")).toBeInTheDocument();

    emitProgress({
      type: "job_completed",
      job_id: "job-running",
      repository_id: "repo-1",
      duration_ms: 1200,
    });
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });

  it("ignores events the schema rejects", async () => {
    render(<Synchronization />);
    expect(await screen.findByText("owner/repo")).toBeInTheDocument();

    emitProgress({ type: "job_progress", job_id: 123 });
    emitProgress({ type: "something_else", job_id: "job-running" });
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });
});
