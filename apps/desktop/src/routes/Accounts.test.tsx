import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import Accounts from "./Accounts";

vi.mock("@reponyx/api-client", () => ({
  addPatAccount: vi.fn(),
  completePairing: vi.fn(),
  createBrokerSetupState: vi.fn(),
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
  getBrokerStatus,
  listAccounts,
  listBrokerInstallations,
} from "@reponyx/api-client";

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
