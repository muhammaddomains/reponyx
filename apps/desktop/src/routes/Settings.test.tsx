import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { Settings as SettingsData } from "@reponyx/shared";
import Settings from "./Settings";

vi.mock("@reponyx/api-client", () => ({
  createDatabaseBackup: vi.fn(),
  deleteDatabaseBackup: vi.fn(),
  exportConfig: vi.fn(),
  exportDiagnostics: vi.fn(),
  getSettings: vi.fn(),
  importConfig: vi.fn(),
  listDatabaseBackups: vi.fn(),
  openSupportFolder: vi.fn(),
  restoreDatabaseBackup: vi.fn(),
  setSetting: vi.fn(),
}));

import {
  getSettings,
  listDatabaseBackups,
  setSetting,
} from "@reponyx/api-client";

const loadedSettings = {
  defaultSyncMode: "mirror",
  concurrency: 5,
  storageRoot: null,
  theme: "system",
  notificationsEnabled: true,
  deviceIdentifier: null,
  dataDir: "C:\\Users\\test\\AppData\\Roaming\\RepoNyx",
  appVersion: "0.1.0",
} as SettingsData;

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getSettings).mockResolvedValue(loadedSettings);
  vi.mocked(listDatabaseBackups).mockResolvedValue([]);
  vi.mocked(setSetting).mockResolvedValue("true");
});

describe("Settings page", () => {
  it("shows a loading state, then the real sections", async () => {
    render(<Settings />);
    expect(screen.getByText("Loading settings…")).toBeInTheDocument();
    expect(await screen.findByText("General")).toBeInTheDocument();
    expect(screen.getByText("Sync")).toBeInTheDocument();
    expect(screen.getByText("Storage")).toBeInTheDocument();
    expect(screen.getByText("Advanced")).toBeInTheDocument();
    expect(screen.getByText("Diagnostics & backup")).toBeInTheDocument();
    expect(screen.getByText("Import & export")).toBeInTheDocument();
  });

  it("shows the loaded values in the form", async () => {
    render(<Settings />);
    await screen.findByText("General");
    expect(
      screen.getByLabelText("Theme") as HTMLSelectElement,
    ).toHaveValue("system");
    expect(
      screen.getByLabelText("Concurrent syncs") as HTMLInputElement,
    ).toHaveValue(5);
    expect(screen.getByText("0.1.0")).toBeInTheDocument();
  });

  it("shows the notifications toggle from the loaded settings", async () => {
    render(<Settings />);
    await screen.findByText("General");
    const toggle = screen.getByLabelText(
      "Notify me when a sync finishes or fails",
    ) as HTMLInputElement;
    expect(toggle.checked).toBe(true);
  });

  it("saves the notifications toggle when it changes", async () => {
    render(<Settings />);
    await screen.findByText("General");
    fireEvent.click(
      screen.getByLabelText("Notify me when a sync finishes or fails"),
    );
    expect(vi.mocked(setSetting)).toHaveBeenCalledWith(
      "notifications.enabled",
      "false",
    );
    expect(
      await screen.findByText("Notifications saved."),
    ).toBeInTheDocument();
  });

  it("shows an error state when settings fail to load", async () => {
    vi.mocked(getSettings).mockRejectedValue(
      new Error("backend unreachable"),
    );
    render(<Settings />);
    expect(await screen.findByText("backend unreachable")).toBeInTheDocument();
    expect(screen.queryByText("General")).not.toBeInTheDocument();
  });

  it("saves the theme when the select changes", async () => {
    render(<Settings />);
    await screen.findByText("General");
    fireEvent.change(screen.getByLabelText("Theme"), {
      target: { value: "dark" },
    });
    expect(vi.mocked(setSetting)).toHaveBeenCalledWith("ui.theme", "dark");
    expect(await screen.findByText("Theme saved.")).toBeInTheDocument();
  });

  it("rejects an out-of-range concurrency without saving", async () => {
    render(<Settings />);
    await screen.findByText("General");
    const input = screen.getByLabelText("Concurrent syncs");
    const row = input.closest("div") as HTMLElement;
    fireEvent.change(input, { target: { value: "0" } });
    fireEvent.click(within(row).getByRole("button", { name: "Save" }));
    expect(
      await screen.findByText(
        "Concurrent syncs must be a whole number between 1 and 10.",
      ),
    ).toBeInTheDocument();
    expect(vi.mocked(setSetting)).not.toHaveBeenCalledWith(
      "sync.concurrency",
      expect.anything(),
    );
  });

  it("saves a valid concurrency", async () => {
    render(<Settings />);
    await screen.findByText("General");
    const input = screen.getByLabelText("Concurrent syncs");
    const row = input.closest("div") as HTMLElement;
    fireEvent.change(input, { target: { value: "3" } });
    fireEvent.click(within(row).getByRole("button", { name: "Save" }));
    expect(vi.mocked(setSetting)).toHaveBeenCalledWith(
      "sync.concurrency",
      "3",
    );
    expect(
      await screen.findByText("Concurrent syncs saved."),
    ).toBeInTheDocument();
  });
});
