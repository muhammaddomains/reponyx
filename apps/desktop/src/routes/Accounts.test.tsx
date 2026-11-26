import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
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
  createBrokerSetupState,
  deletePatAccount,
  getBrokerStatus,
  getSettings,
  listAccounts,
  listBrokerInstallations,
  pickBrokerKeyFile,
  setSetting,
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
    appIdConfigured: true,
    privateKeyConfigured: true,
    envFileConfigured: false,
  });
  vi.mocked(getSettings).mockResolvedValue({
    defaultSyncMode: null,
    concurrency: null,
    storageRoot: null,
    theme: null,
    notificationsEnabled: null,
    deviceIdentifier: null,
    brokerAppId: null,
    brokerPrivateKeyPath: null,
    dataDir: "",
    appVersion: "0.1.0",
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

  it("disables the Install button until the broker is configured", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue({
      configured: false,
      reachable: true,
      baseUrl: "http://127.0.0.1:8765",
      appIdConfigured: true,
      privateKeyConfigured: true,
      envFileConfigured: false,
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
    render(<Accounts />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete account" }),
    );
    const dialog = screen.getByRole("alertdialog", {
      name: "Delete account",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Delete account" }),
    );
    expect(deletePatAccount).toHaveBeenCalledWith(revokedPatAccount.id);
    expect(
      await screen.findByText("Deleted the account octocat."),
    ).toBeInTheDocument();
  });

  it("does nothing when the confirmation is dismissed", async () => {
    vi.mocked(listAccounts).mockResolvedValue([revokedPatAccount]);
    render(<Accounts />);
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

describe("Accounts page — broker down", () => {
  const connectedPatAccount: Account = {
    id: "00000000-0000-0000-0000-000000000002",
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

  it("still shows the accounts when the installations call rejects", async () => {
    vi.mocked(listAccounts).mockResolvedValue([connectedPatAccount]);
    vi.mocked(getBrokerStatus).mockResolvedValue({
      configured: true,
      reachable: false,
      baseUrl: "http://127.0.0.1:8765",
      appIdConfigured: true,
      privateKeyConfigured: true,
      envFileConfigured: false,
    });
    vi.mocked(listBrokerInstallations).mockRejectedValue(
      new Error("NetworkUnavailable: cannot reach the Auth Broker"),
    );
    render(<Accounts />);
    expect(await screen.findByText("octocat")).toBeInTheDocument();
    expect(
      await screen.findByText(/is not reachable\./),
    ).toBeInTheDocument();
    expect(screen.queryByText(/No accounts yet/)).not.toBeInTheDocument();
  });
});

describe("Accounts page — in-app broker setup", () => {
  const unconfigured = {
    configured: false,
    reachable: false,
    baseUrl: "http://127.0.0.1:8765",
    appIdConfigured: false,
    privateKeyConfigured: false,
    envFileConfigured: false,
  };

  it("shows the setup form when the broker has no configuration", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue(unconfigured);
    render(<Accounts />);
    expect(
      await screen.findByRole("button", { name: "Save broker setup" }),
    ).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText("GitHub App id, e.g. 5896712"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Choose file…" }),
    ).toBeInTheDocument();
    // The setup token is automatic (.env file, else generated per
    // install): no token field and no Generate button anywhere.
    expect(
      screen.queryByPlaceholderText("Broker setup token"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Generate" }),
    ).not.toBeInTheDocument();
  });

  it("hides the setup form once the broker is configured", async () => {
    render(<Accounts />);
    await screen.findByRole("button", { name: "Install GitHub App" });
    expect(
      screen.queryByRole("button", { name: "Save broker setup" }),
    ).not.toBeInTheDocument();
  });

  it("saves the App id and key path from the form", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue(unconfigured);
    vi.mocked(pickBrokerKeyFile).mockResolvedValue("C:\\keys\\reponyx.pem");
    vi.mocked(setSetting).mockResolvedValue("saved");
    render(<Accounts />);
    fireEvent.change(
      await screen.findByPlaceholderText("GitHub App id, e.g. 5896712"),
      { target: { value: "5896712" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
    expect(
      await screen.findByDisplayValue("C:\\keys\\reponyx.pem"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save broker setup" }));
    expect(
      await screen.findByText(/Broker setup saved/),
    ).toBeInTheDocument();
    expect(vi.mocked(setSetting)).toHaveBeenCalledWith(
      "broker.app_id",
      "5896712",
    );
    expect(vi.mocked(setSetting)).toHaveBeenCalledWith(
      "broker.private_key_path",
      "C:\\keys\\reponyx.pem",
    );
  });

  it("never shows a setup-token field, configured or not", async () => {
    vi.mocked(getBrokerStatus).mockResolvedValue(unconfigured);
    render(<Accounts />);
    await screen.findByRole("button", { name: "Save broker setup" });
    expect(
      screen.queryByPlaceholderText("Broker setup token"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Save setup token" }),
    ).not.toBeInTheDocument();
  });
});
