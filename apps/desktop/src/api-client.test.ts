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
  clearSetting,
  createBrokerSetupState,
  createSchedule,
  removePatAccount,
  webExchangeDesktopCode,
  webLinkGithubAccount,
  webLinkPatAccount,
  webLogin,
  webLogout,
  webOpenDashboard,
  webOpenOAuth,
  webOpenRegister,
  webOpenUrl,
  webSessionStatus,
  webUnlinkPatAccount,
  webVerify2fa,
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

  it("clearSetting sends the key to clear_setting_command", async () => {
    invokeMock.mockResolvedValue("Storage root reset to the default.");
    const result = await clearSetting("storage.root");
    expect(invokeMock).toHaveBeenCalledWith("clear_setting_command", {
      key: "storage.root",
    });
    expect(result).toBe("Storage root reset to the default.");
  });
});

describe("web login commands (W3)", () => {
  const webUser = {
    id: "019c0000-0000-7000-8000-000000000003",
    email: "user@example.com",
    name: "Test User",
    createdAt: "2026-11-27T00:00:00Z",
  };

  const webAccount = {
    user: webUser,
    package: {
      name: "Free",
      maxLinkedAccounts: 2,
      maxDevices: 2,
      expiresAt: null,
    },
    usage: { linkedAccounts: 0, devices: 1 },
  };

  it("webLogin sends email/password and parses the 2FA step", async () => {
    invokeMock.mockResolvedValue({
      step: "2fa_required",
      pendingToken: "pending-token",
      user: null,
    });
    const result = await webLogin("user@example.com", "secret");
    expect(invokeMock).toHaveBeenCalledWith("web_login_command", {
      email: "user@example.com",
      password: "secret",
    });
    expect(result.step).toBe("2fa_required");
    expect(result.pendingToken).toBe("pending-token");
  });

  it("webVerify2fa sends pendingToken/code, not pending_token", async () => {
    invokeMock.mockResolvedValue({
      step: "ok",
      pendingToken: null,
      user: webUser,
    });
    const result = await webVerify2fa("pending-token", "123456");
    expect(invokeMock).toHaveBeenCalledWith("web_verify_2fa_command", {
      pendingToken: "pending-token",
      code: "123456",
    });
    expect(result.user?.email).toBe("user@example.com");
  });

  it("webSessionStatus parses the account payload", async () => {
    invokeMock.mockResolvedValue({
      authenticated: true,
      offline: false,
      sessionExpired: false,
      account: webAccount,
    });
    const result = await webSessionStatus();
    expect(invokeMock).toHaveBeenCalledWith(
      "web_session_status_command",
      undefined,
    );
    expect(result.account?.package.name).toBe("Free");
    expect(result.account?.usage.devices).toBe(1);
  });

  it("webLogout calls the logout command", async () => {
    invokeMock.mockResolvedValue(undefined);
    await webLogout();
    expect(invokeMock).toHaveBeenCalledWith(
      "web_logout_command",
      undefined,
    );
  });

  it("webOpenOAuth sends the provider", async () => {
    invokeMock.mockResolvedValue(undefined);
    await webOpenOAuth("github");
    expect(invokeMock).toHaveBeenCalledWith("web_open_oauth_command", {
      provider: "github",
    });
  });

  it("webOpenRegister and webOpenDashboard call their commands", async () => {
    invokeMock.mockResolvedValue(undefined);
    await webOpenRegister();
    expect(invokeMock).toHaveBeenCalledWith(
      "web_open_register_command",
      undefined,
    );
    await webOpenDashboard();
    expect(invokeMock).toHaveBeenCalledWith(
      "web_open_dashboard_command",
      undefined,
    );
  });

  it("webOpenUrl sends the URL", async () => {
    invokeMock.mockResolvedValue(undefined);
    await webOpenUrl("/register/complete?token=abc");
    expect(invokeMock).toHaveBeenCalledWith("web_open_url_command", {
      url: "/register/complete?token=abc",
    });
  });

  it("webExchangeDesktopCode sends code, not the token", async () => {
    invokeMock.mockResolvedValue({
      step: "ok",
      pendingToken: null,
      user: webUser,
    });
    const result = await webExchangeDesktopCode("desktop-code");
    expect(invokeMock).toHaveBeenCalledWith(
      "web_exchange_desktop_code_command",
      { code: "desktop-code" },
    );
    expect(result.user?.name).toBe("Test User");
  });
});

describe("web linked accounts (W4)", () => {
  const linkedAccount = {
    id: "019c0000-0000-7000-8000-000000000004",
    login: "octocat",
    authKind: "app",
  };

  it("webLinkGithubAccount sends the installation id", async () => {
    invokeMock.mockResolvedValue(linkedAccount);
    const result = await webLinkGithubAccount("777");
    expect(invokeMock).toHaveBeenCalledWith(
      "web_link_github_account_command",
      { installationId: "777" },
    );
    expect(result.login).toBe("octocat");
    expect(result.authKind).toBe("app");
  });

  it("webLinkPatAccount sends the login", async () => {
    invokeMock.mockResolvedValue({ ...linkedAccount, authKind: "pat" });
    const result = await webLinkPatAccount("octocat");
    expect(invokeMock).toHaveBeenCalledWith(
      "web_link_pat_account_command",
      { login: "octocat" },
    );
    expect(result.authKind).toBe("pat");
  });

  it("webUnlinkPatAccount sends the login", async () => {
    invokeMock.mockResolvedValue(undefined);
    await webUnlinkPatAccount("octocat");
    expect(invokeMock).toHaveBeenCalledWith(
      "web_unlink_pat_account_command",
      { login: "octocat" },
    );
  });
});
