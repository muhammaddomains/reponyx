import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/** Regression tests for the Tauri invoke argument names.
 *
 *  Tauri 2 commands take camelCase argument names by default
 *  (tauri-macros `ArgumentCase::Camel`); the api-client used to send
 *  the Rust snake_case parameter names, so every command with a
 *  multi-word argument failed at runtime with
 *  "missing required key <camelCase>" — while the unit tests, which
 *  mocked the api-client itself, never saw it. These tests mock
 *  `invoke` instead, so the argument keys are asserted for real. */

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  addPatAccount,
  agentCancelJob,
  createBrokerSetupState,
  createSchedule,
  removePatAccount,
} from "@reponyx/api-client";

const account = {
  id: "019c0000-0000-7000-8000-000000000001",
  provider: "github",
  providerAccountId: "123",
  username: "octo",
  accountType: "personal",
  authenticationMethod: "fine_grained_pat",
  status: "connected",
  patExpiryStatus: "valid",
  createdAt: "2026-11-23T10:00:00Z",
  updatedAt: "2026-11-23T10:00:00Z",
};

const schedule = {
  id: "019c0000-0000-7000-8000-000000000002",
  scope: "global",
  accountId: null,
  repositoryId: null,
  intervalType: "interval_hours",
  intervalValue: "6",
  timezone: "America/Phoenix",
  enabled: true,
  nextRunAt: null,
  createdAt: "2026-11-23T10:00:00Z",
  updatedAt: "2026-11-23T10:00:00Z",
  computedNextRun: null,
  nextRunError: null,
};

beforeEach(() => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  invokeMock.mockReset();
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("Tauri invoke argument names are camelCase", () => {
  it("addPatAccount sends accountType/expiresAt, not account_type/expires_at", async () => {
    invokeMock.mockResolvedValue(account);
    await addPatAccount({
      pat: "ghp_test",
      label: "main",
      accountType: "personal",
      expiresAt: "2026-12-23T00:00:00Z",
    });
    expect(invokeMock).toHaveBeenCalledWith("add_pat_account_command", {
      pat: "ghp_test",
      label: "main",
      accountType: "personal",
      expiresAt: "2026-12-23T00:00:00Z",
    });
  });

  it("removePatAccount sends accountId, not account_id", async () => {
    invokeMock.mockResolvedValue(account);
    await removePatAccount("019c0000-0000-7000-8000-000000000001");
    expect(invokeMock).toHaveBeenCalledWith("remove_pat_account_command", {
      accountId: "019c0000-0000-7000-8000-000000000001",
    });
  });

  it("createSchedule sends camelCase keys", async () => {
    invokeMock.mockResolvedValue(schedule);
    await createSchedule({
      scope: "global",
      accountId: null,
      repositoryId: null,
      intervalType: "interval_hours",
      intervalValue: "6",
      timezone: "America/Phoenix",
      enabled: true,
    });
    expect(invokeMock).toHaveBeenCalledWith("create_schedule_command", {
      scope: "global",
      accountId: null,
      repositoryId: null,
      intervalType: "interval_hours",
      intervalValue: "6",
      timezone: "America/Phoenix",
      enabled: true,
    });
  });

  it("createBrokerSetupState calls the setup-state command and parses the install URL", async () => {
    invokeMock.mockResolvedValue({
      installUrl:
        "https://github.com/apps/reponyx/installations/new?state=abc",
      expiresAt: "2026-11-23T19:30:00Z",
    });
    const result = await createBrokerSetupState();
    expect(invokeMock).toHaveBeenCalledWith(
      "create_broker_setup_state_command",
      undefined,
    );
    expect(result.installUrl).toContain("github.com/apps/reponyx");
  });

  it("agentCancelJob sends jobId, not job_id", async () => {
    invokeMock.mockResolvedValue(undefined);
    await agentCancelJob("job-1");
    expect(invokeMock).toHaveBeenCalledWith("agent_cancel_command", {
      jobId: "job-1",
    });
  });
});
