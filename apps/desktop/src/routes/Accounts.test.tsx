import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import Accounts from "./Accounts";

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
  webUnlinkPatAccount: vi.fn(),
}));

import {
  deletePatAccount,
  listAccounts,
  listBrokerInstallations,
  removePatAccount,
  validatePatAccount,
  webLinkGithubAccount,
  webLinkPatAccount,
  webUnlinkPatAccount,
} from "@reponyx/api-client";
import type { Account } from "@reponyx/shared";

function patAccount(overrides: Partial<Account> = {}): Account {
  return {
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
    ...overrides,
  };
}

function appAccount(overrides: Partial<Account> = {}): Account {
  return patAccount({
    id: "00000000-0000-0000-0000-000000000002",
    providerAccountId: "43",
    username: "hubot",
    displayName: "Hubot Org",
    accountType: "organization",
    authenticationMethod: "github_app",
    status: "revoked",
    ...overrides,
  });
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderAccounts(accounts: Account[]) {
  vi.mocked(listAccounts).mockResolvedValue(accounts);
  render(
    <MemoryRouter initialEntries={["/accounts"]}>
      <LocationProbe />
      <Accounts />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listAccounts).mockResolvedValue([]);
  vi.mocked(listBrokerInstallations).mockResolvedValue([]);
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
  vi.mocked(webUnlinkPatAccount).mockResolvedValue(undefined);
});

describe("Accounts page — Connected accounts tab", () => {
  it("shows the summary line and both tabs, with Connected active", async () => {
    renderAccounts([patAccount(), appAccount()]);
    expect(await screen.findByText("octocat")).toBeInTheDocument();
    expect(screen.getByText("hubot")).toBeInTheDocument();
    expect(screen.getByText("1 connected · 1 revoked")).toBeInTheDocument();
    const connectedTab = screen.getByRole("link", {
      name: "Connected accounts",
    });
    expect(connectedTab).toHaveAttribute("aria-current", "page");
    expect(
      screen.getByRole("link", { name: "Add account" }),
    ).toHaveAttribute("href", "/accounts/add");
  });

  it("filters by username search", async () => {
    renderAccounts([patAccount(), appAccount()]);
    await screen.findByText("octocat");
    fireEvent.change(
      screen.getByLabelText("Search accounts by username"),
      { target: { value: "octo" } },
    );
    expect(screen.getByText("octocat")).toBeInTheDocument();
    expect(screen.queryByText("hubot")).not.toBeInTheDocument();
  });

  it("filters by kind", async () => {
    renderAccounts([patAccount(), appAccount()]);
    await screen.findByText("octocat");
    fireEvent.change(screen.getByLabelText("Filter by kind"), {
      target: { value: "github_app" },
    });
    expect(screen.queryByText("octocat")).not.toBeInTheDocument();
    expect(screen.getByText("hubot")).toBeInTheDocument();
  });

  it("filters by status", async () => {
    renderAccounts([patAccount(), appAccount()]);
    await screen.findByText("octocat");
    fireEvent.change(screen.getByLabelText("Filter by status"), {
      target: { value: "revoked" },
    });
    expect(screen.queryByText("octocat")).not.toBeInTheDocument();
    expect(screen.getByText("hubot")).toBeInTheDocument();
  });

  it("shows the no-match empty state and clears the filters", async () => {
    renderAccounts([patAccount(), appAccount()]);
    await screen.findByText("octocat");
    fireEvent.change(
      screen.getByLabelText("Search accounts by username"),
      { target: { value: "zzz" } },
    );
    expect(
      await screen.findByText("No accounts match this search."),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(screen.getByText("octocat")).toBeInTheDocument();
    expect(screen.getByText("hubot")).toBeInTheDocument();
  });

  it("shows the no-accounts empty state with a button to the Add tab", async () => {
    renderAccounts([]);
    expect(
      await screen.findByText("No accounts yet — add your first account."),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Add your first account" }),
    );
    expect(screen.getByTestId("location")).toHaveTextContent("/accounts/add");
  });

  it("validates a PAT account and shows a notice", async () => {
    renderAccounts([patAccount()]);
    vi.mocked(validatePatAccount).mockResolvedValue(patAccount());
    fireEvent.click(await screen.findByRole("button", { name: "Validate" }));
    expect(validatePatAccount).toHaveBeenCalledWith(patAccount().id);
    expect(
      await screen.findByText("Validated as octocat."),
    ).toBeInTheDocument();
  });
});

describe("Accounts page — Delete account", () => {
  it("deletes the account after confirmation, unlinks it from the web, and shows a notice", async () => {
    renderAccounts([patAccount({ status: "revoked" })]);
    vi.mocked(deletePatAccount).mockResolvedValue(true);
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete account" }),
    );
    const dialog = screen.getByRole("alertdialog", {
      name: "Delete account",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Delete account" }),
    );
    expect(deletePatAccount).toHaveBeenCalledWith(patAccount().id);
    expect(
      await screen.findByText("Deleted the account octocat."),
    ).toBeInTheDocument();
    expect(webUnlinkPatAccount).toHaveBeenCalledWith("octocat");
  });

  it("keeps the delete when the web unlink fails, and says so", async () => {
    renderAccounts([patAccount({ status: "revoked" })]);
    vi.mocked(deletePatAccount).mockResolvedValue(true);
    vi.mocked(webUnlinkPatAccount).mockRejectedValue(
      new Error("could not reach the web"),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete account" }),
    );
    const dialog = screen.getByRole("alertdialog", {
      name: "Delete account",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Delete account" }),
    );
    expect(
      await screen.findByText(/Deleted the account octocat, but the web link could not be removed/),
    ).toBeInTheDocument();
  });

  it("does nothing when the confirmation is dismissed", async () => {
    renderAccounts([patAccount({ status: "revoked" })]);
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete account" }),
    );
    const dialog = screen.getByRole("alertdialog", {
      name: "Delete account",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(deletePatAccount).not.toHaveBeenCalled();
    expect(webUnlinkPatAccount).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});

describe("Accounts page — Remove PAT", () => {
  it("removes the PAT after confirmation and unlinks it from the web", async () => {
    renderAccounts([patAccount()]);
    vi.mocked(removePatAccount).mockResolvedValue(patAccount());
    fireEvent.click(await screen.findByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("alertdialog", {
      name: "Remove PAT account",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(removePatAccount).toHaveBeenCalledWith(patAccount().id);
    expect(
      await screen.findByText("Removed the PAT for octocat."),
    ).toBeInTheDocument();
    expect(webUnlinkPatAccount).toHaveBeenCalledWith("octocat");
  });
});

describe("Accounts page — web link reconcile (W4)", () => {
  const pairedInstallation = {
    installationId: "777",
    setupAction: "install",
    accountLogin: "octocat",
    installedAt: "2026-11-24T00:00:00Z",
    pairedDevice: "device-1",
  };

  it("links every paired installation and PAT account on load", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([
      pairedInstallation,
    ]);
    renderAccounts([patAccount(), appAccount()]);
    await screen.findByText("octocat");
    expect(webLinkGithubAccount).toHaveBeenCalledWith("777");
    expect(webLinkPatAccount).toHaveBeenCalledWith("octocat");
  });

  it("skips installations that are not paired", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([
      { ...pairedInstallation, pairedDevice: null },
    ]);
    renderAccounts([patAccount()]);
    await screen.findByText("octocat");
    expect(webLinkGithubAccount).not.toHaveBeenCalled();
    expect(webLinkPatAccount).toHaveBeenCalledWith("octocat");
  });

  it("surfaces a limit failure as a notice and keeps the account syncing locally", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([
      pairedInstallation,
    ]);
    vi.mocked(webLinkGithubAccount).mockRejectedValue({
      code: "limit_exceeded",
      message: "You have reached your package limit for linked accounts",
    });
    renderAccounts([patAccount()]);
    expect(
      await screen.findByText(
        /Some accounts could not be linked to the web — octocat: limit_exceeded/,
      ),
    ).toBeInTheDocument();
  });

  it("stays silent when the web is unreachable", async () => {
    vi.mocked(listBrokerInstallations).mockResolvedValue([
      pairedInstallation,
    ]);
    vi.mocked(webLinkGithubAccount).mockRejectedValue({
      code: "WebUnreachable",
      message: "Could not reach reponyx.bond",
    });
    renderAccounts([patAccount()]);
    await screen.findByText("octocat");
    expect(
      screen.queryByText(/could not be linked to the web/),
    ).not.toBeInTheDocument();
  });
});
