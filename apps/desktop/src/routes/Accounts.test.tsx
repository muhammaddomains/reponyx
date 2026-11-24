import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
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
  setBrokerSetupToken: vi.fn(),
  startPairing: vi.fn(),
  validateGithubAppAccount: vi.fn(),
  validatePatAccount: vi.fn(),
}));

import {
  createBrokerSetupState,
  deletePatAccount,
  getBrokerStatus,
  listAccounts,
  listBrokerInstallations,
  startPairing,
} from "@reponyx/api-client";
import type { Account } from "@reponyx/shared";

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listAccounts).mockResolvedValue([]);
  vi.mocked(getBrokerStatus).mockResolvedValue({
    configured: true,
    reachable: true,
    baseUrl: "http://127.0.0.1:8765",
  });
  vi.mocked(listBrokerInstallations).mockResolvedValue([]);
  vi.mocked(createBrokerSetupState).mockResolvedValue({
    installUrl:
      "https://github.com/apps/reponyx/installations/new?state=abc",
    expiresAt: "2026-11-23T19:30:00Z",
  });
});

describe("Accounts page — GitHub App install", () => {
  it("shows the Install button and the no-installations hint when the broker is configured", async () => {
    render(<Accounts />);
    expect(
      await screen.findByRole("button", { name: "Install GitHub App" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(/No installations yet/),
    ).toBeInTheDocument();
  });

  it("opens the install flow and tells the user to press Refresh after installing", async () => {
    render(<Accounts />);
    const install = await screen.findByRole("button", {
      name: "Install GitHub App",
    });
    fireEvent.click(install);
    expect(createBrokerSetupState).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByText(/Install page opened in your browser/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/then press Refresh below/),
    ).toBeInTheDocument();
  });

  it("disables the Install button until the setup token is configured", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue({
      configured: false,
      reachable: true,
      baseUrl: "http://127.0.0.1:8765",
    });
    render(<Accounts />);
    const install = await screen.findByRole("button", {
      name: "Install GitHub App",
    });
    expect(install).toBeDisabled();
  });
});

describe("Accounts page — Delete account", () => {
  const revokedPatAccount: Account = {
    id: "00000000-0000-0000-0000-000000000001",
    provider: "github",
    providerAccountId: "42",
    username: "octocat",
    displayName: null,
    label: null,
    accountType: "personal",
    authenticationMethod: "fine_grained_pat",
    status: "revoked",
    patExpiresAt: null,
    patExpiryStatus: "unknown",
    patExpiryDaysLeft: null,
    lastValidatedAt: null,
    createdAt: "2026-11-24T00:00:00Z",
    updatedAt: "2026-11-24T00:00:00Z",
  };

  it("deletes the account after confirmation and shows a notice", async () => {
    vi.mocked(listAccounts).mockResolvedValue([revokedPatAccount]);
    vi.mocked(deletePatAccount).mockResolvedValue(true);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<Accounts />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete account" }),
    );
    expect(deletePatAccount).toHaveBeenCalledWith(revokedPatAccount.id);
    expect(
      await screen.findByText("Deleted the account octocat."),
    ).toBeInTheDocument();
    confirm.mockRestore();
  });

  it("does nothing when the confirmation is dismissed", async () => {
    vi.mocked(listAccounts).mockResolvedValue([revokedPatAccount]);
    vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<Accounts />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete account" }),
    );
    expect(deletePatAccount).not.toHaveBeenCalled();
  });
});

describe("Accounts page — pairing code", () => {
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
    render(<Accounts />);
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
    render(<Accounts />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Pair this device" }),
    );
    expect(await screen.findByText("ABC-DEF-GHI")).toBeInTheDocument();
    expect(screen.getByText(/it does not expire/)).toBeInTheDocument();
  });
});
