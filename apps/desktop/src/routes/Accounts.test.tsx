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
  getSettings: vi.fn(),
  listAccounts: vi.fn(),
  listBrokerInstallations: vi.fn(),
  pickBrokerKeyFile: vi.fn(),
  reconnectPatAccount: vi.fn(),
  removePatAccount: vi.fn(),
  replacePatAccount: vi.fn(),
  revokeDevicePairing: vi.fn(),
  setSetting: vi.fn(),
  startPairing: vi.fn(),
  validateGithubAppAccount: vi.fn(),
  validatePatAccount: vi.fn(),
}));

import {
  deletePatAccount,
  listAccounts,
  validatePatAccount,
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
  it("deletes the account after confirmation and shows a notice", async () => {
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
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});
