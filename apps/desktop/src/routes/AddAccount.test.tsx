import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import AddAccount from "./AddAccount";

vi.mock("@reponyx/api-client", () => ({
  addPatAccount: vi.fn(),
  completePairing: vi.fn(),
  createBrokerSetupState: vi.fn(),
  deletePatAccount: vi.fn(),
  getBrokerStatus: vi.fn(),
  listAccounts: vi.fn(),
  listBrokerInstallations: vi.fn(),
  reconnectPatAccount: vi.fn(),
  removePatAccount: vi.fn(),
  replacePatAccount: vi.fn(),
  revokeDevicePairing: vi.fn(),
  startPairing: vi.fn(),
  validateGithubAppAccount: vi.fn(),
  validatePatAccount: vi.fn(),
  webLinkGithubAccount: vi.fn(),
  webLinkPatAccount: vi.fn(),
}));

import {
  addPatAccount,
  completePairing,
  createBrokerSetupState,
  getBrokerStatus,
  listBrokerInstallations,
  startPairing,
  webLinkGithubAccount,
  webLinkPatAccount,
} from "@reponyx/api-client";
import type { Account } from "@reponyx/shared";

function renderAddAccount() {
  render(
    <MemoryRouter initialEntries={["/accounts/add"]}>
      <AddAccount />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getBrokerStatus).mockResolvedValue({
    configured: true,
    reachable: true,
    baseUrl: "http://127.0.0.1:8765",
    envFileConfigured: false,
  });
  vi.mocked(listBrokerInstallations).mockResolvedValue([]);
  vi.mocked(createBrokerSetupState).mockResolvedValue({
    installUrl:
      "https://github.com/apps/reponyx/installations/new?state=abc",
    expiresAt: "2026-11-23T19:30:00Z",
  });
  vi.mocked(webLinkGithubAccount).mockResolvedValue({
    id: "019c0000-0000-7000-8000-000000000004",
    login: "octocat",
    authKind: "app",
  });
  vi.mocked(webLinkPatAccount).mockResolvedValue({
    id: "019c0000-0000-7000-8000-000000000005",
    login: "octocat",
    authKind: "pat",
  });
});

describe("Add account page — tabs", () => {
  it("shows the Add account tab active", async () => {
    renderAddAccount();
    await screen.findByRole("button", { name: "Install GitHub App" });
    expect(
      screen.getByRole("link", { name: "Add account" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.getByRole("link", { name: "Connected accounts" }),
    ).toHaveAttribute("href", "/accounts");
  });
});

describe("Add account page — PAT form", () => {
  const addedAccount: Account = {
    id: "00000000-0000-0000-0000-000000000001",
    provider: "github",
    providerAccountId: "42",
    username: "octocat",
    displayName: null,
    label: null,
    accountType: "personal",
    authenticationMethod: "fine_grained_pat",
    status: "connected",
    patExpiresAt: null,
    patExpiryStatus: "unknown",
    patExpiryDaysLeft: null,
    lastValidatedAt: null,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  };

  it("adds a PAT account, links it to the web, and shows a notice", async () => {
    vi.mocked(addPatAccount).mockResolvedValue(addedAccount);
    renderAddAccount();
    const patInput = await screen.findByPlaceholderText(
      "github_pat_… or ghp_…",
    );
    fireEvent.change(patInput, { target: { value: "github_pat_test" } });
    fireEvent.click(screen.getByRole("button", { name: "Add PAT account" }));
    expect(addPatAccount).toHaveBeenCalledWith({
      pat: "github_pat_test",
      label: null,
      accountType: "personal",
      expiresAt: null,
    });
    expect(
      await screen.findByText(
        "Added PAT account octocat — linked to your web account.",
      ),
    ).toBeInTheDocument();
    expect(webLinkPatAccount).toHaveBeenCalledWith("octocat");
  });

  it("keeps the PAT account when the web link fails, and says the link will be retried", async () => {
    vi.mocked(addPatAccount).mockResolvedValue(addedAccount);
    vi.mocked(webLinkPatAccount).mockRejectedValue(
      new Error("could not reach the web"),
    );
    renderAddAccount();
    const patInput = await screen.findByPlaceholderText(
      "github_pat_… or ghp_…",
    );
    fireEvent.change(patInput, { target: { value: "github_pat_test" } });
    fireEvent.click(screen.getByRole("button", { name: "Add PAT account" }));
    expect(
      await screen.findByText(/Added PAT account octocat\. Web link failed/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/will be retried when the Accounts page loads/),
    ).toBeInTheDocument();
  });

  it("disables the Add button while the PAT field is empty", async () => {
    renderAddAccount();
    await screen.findByPlaceholderText("github_pat_… or ghp_…");
    expect(
      screen.getByRole("button", { name: "Add PAT account" }),
    ).toBeDisabled();
  });
});

describe("Add account page — GitHub App install", () => {
  it("shows the Install button and the no-installations hint when the broker is configured", async () => {
    renderAddAccount();
    expect(
      await screen.findByRole("button", { name: "Install GitHub App" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(/No installations yet/),
    ).toBeInTheDocument();
  });

  it("opens the install flow and watches for the chosen installation", async () => {
    renderAddAccount();
    const install = await screen.findByRole("button", {
      name: "Install GitHub App",
    });
    fireEvent.click(install);
    expect(createBrokerSetupState).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByText(/Install page opened in your browser/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/watches for the installation you choose/),
    ).toBeInTheDocument();
  });

  it("shows the installation when it appears after the Install click, without Refresh", async () => {
    const installation = {
      installationId: "777",
      setupAction: "install",
      accountLogin: "octocat",
      installedAt: "2026-11-24T00:00:00Z",
      pairedDevice: null,
    };
    renderAddAccount();
    const install = await screen.findByRole("button", {
      name: "Install GitHub App",
    });
    vi.useFakeTimers();
    try {
      vi.mocked(listBrokerInstallations).mockResolvedValue([installation]);
      fireEvent.click(install);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(
        screen.getByText(/watches for the installation you choose/),
      ).toBeInTheDocument();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2000);
      });
      expect(screen.getByText("Installation 777 (octocat)")).toBeInTheDocument();
      expect(
        screen.getByText(/is here — pair this device/),
      ).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("disables the Install button until the broker is configured", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue({
      configured: false,
      reachable: true,
      baseUrl: "http://127.0.0.1:8765",
      envFileConfigured: false,
    });
    renderAddAccount();
    const install = await screen.findByRole("button", {
      name: "Install GitHub App",
    });
    expect(install).toBeDisabled();
  });
});

describe("Add account page — pairing code", () => {
  const installation = {
    installationId: "777",
    setupAction: "install",
    accountLogin: "octocat",
    installedAt: "2026-11-24T00:00:00Z",
    pairedDevice: null,
  };

  it("shows the account login next to the installation and the expiry when the broker returns one", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([installation]);
    vi.mocked(startPairing).mockResolvedValue({
      pairingCode: "ABC-DEF-GHI",
      expiresAt: "2026-11-24T10:30:00Z",
      installationId: "777",
      deviceIdentifier: "device-1",
    });
    renderAddAccount();
    expect(
      await screen.findByText("Installation 777 (octocat)"),
    ).toBeInTheDocument();
    fireEvent.click(
      await screen.findByRole("button", { name: "Pair this device" }),
    );
    expect(await screen.findByText("ABC-DEF-GHI")).toBeInTheDocument();
    expect(screen.getByText(/Expires/)).toBeInTheDocument();
  });

  it("says the code does not expire when the broker returns no expiry", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([installation]);
    vi.mocked(startPairing).mockResolvedValue({
      pairingCode: "ABC-DEF-GHI",
      expiresAt: null,
      installationId: "777",
      deviceIdentifier: "device-1",
    });
    renderAddAccount();
    fireEvent.click(
      await screen.findByRole("button", { name: "Pair this device" }),
    );
    expect(await screen.findByText("ABC-DEF-GHI")).toBeInTheDocument();
    expect(screen.getByText(/it does not expire/)).toBeInTheDocument();
  });

  it("links the installation to the web after completing the pairing", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([installation]);
    vi.mocked(startPairing).mockResolvedValue({
      pairingCode: "ABC-DEF-GHI",
      expiresAt: null,
      installationId: "777",
      deviceIdentifier: "device-1",
    });
    vi.mocked(completePairing).mockResolvedValue({
      id: "00000000-0000-0000-0000-000000000002",
      provider: "github",
      providerAccountId: "43",
      username: "octocat",
      displayName: null,
      label: null,
      accountType: "personal",
      authenticationMethod: "github_app",
      status: "connected",
      patExpiresAt: null,
      patExpiryStatus: "unknown",
      patExpiryDaysLeft: null,
      lastValidatedAt: null,
      createdAt: "2026-11-24T00:00:00Z",
      updatedAt: "2026-11-24T00:00:00Z",
    });
    renderAddAccount();
    fireEvent.click(
      await screen.findByRole("button", { name: "Pair this device" }),
    );
    await screen.findByText("ABC-DEF-GHI");
    fireEvent.click(
      screen.getByRole("button", { name: "Complete pairing" }),
    );
    expect(
      await screen.findByText(
        "Paired and added GitHub App account octocat — linked to your web account.",
      ),
    ).toBeInTheDocument();
    expect(webLinkGithubAccount).toHaveBeenCalledWith("777");
  });
});

describe("Add account page — broker down", () => {
  it("shows the broker as not reachable but keeps the PAT form", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue({
      configured: true,
      reachable: false,
      baseUrl: "http://127.0.0.1:8765",
      envFileConfigured: false,
    });
    renderAddAccount();
    expect(
      await screen.findByText(/is not reachable\./),
    ).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText("github_pat_… or ghp_…"),
    ).toBeInTheDocument();
  });
});

describe("Add account page — zero-config broker (W4)", () => {
  it("never shows a setup form — no App id, key file, or token field", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue({
      configured: false,
      reachable: false,
      baseUrl: "http://127.0.0.1:8765",
      envFileConfigured: false,
    });
    renderAddAccount();
    await screen.findByRole("button", { name: "Install GitHub App" });
    expect(
      screen.queryByRole("button", { name: "Save broker setup" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByPlaceholderText("GitHub App id, e.g. 5896712"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Choose file…" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByPlaceholderText("Broker setup token"),
    ).not.toBeInTheDocument();
  });
});
