import { describe, expect, it, vi } from "vitest";

/** Update check wiring (Phase 4, audit E3). The updater plugin is
 *  configured but nothing called it before, so these tests pin the
 *  mapping from the plugin's `check()` result to what the Settings
 *  page shows: null means up to date, an Update means available,
 *  a throw means the check failed (not a crash). The plugin module
 *  is mocked — the real check hits the network. */

const { checkMock } = vi.hoisted(() => ({ checkMock: vi.fn() }));

vi.mock("@tauri-apps/plugin-updater", () => ({ check: checkMock }));

import {
  checkForUpdates,
  downloadAndInstall,
  type Update,
} from "./updater";

function fakeUpdate(overrides: Partial<Update> = {}): Update {
  return {
    version: "0.1.1",
    currentVersion: "0.1.0",
    download: vi.fn().mockResolvedValue(undefined),
    install: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as Update;
}

describe("checkForUpdates", () => {
  it("reports up-to-date when the plugin returns null", async () => {
    checkMock.mockResolvedValue(null);
    expect(await checkForUpdates()).toEqual({ status: "up-to-date" });
  });

  it("reports available with the update when one exists", async () => {
    const update = fakeUpdate();
    checkMock.mockResolvedValue(update);
    expect(await checkForUpdates()).toEqual({
      status: "available",
      update,
    });
  });

  it("reports failed with the message instead of throwing", async () => {
    checkMock.mockRejectedValue(new Error("network unreachable"));
    expect(await checkForUpdates()).toEqual({
      status: "failed",
      message: "network unreachable",
    });
  });
});

describe("downloadAndInstall", () => {
  it("downloads first, then installs", async () => {
    const update = fakeUpdate();
    const onEvent = vi.fn();
    await downloadAndInstall(update, onEvent);
    expect(update.download).toHaveBeenCalledWith(onEvent);
    expect(update.install).toHaveBeenCalledTimes(1);
    const downloadOrder = (
      update.download as unknown as { mock: { invocationCallOrder: number[] } }
    ).mock.invocationCallOrder[0];
    const installOrder = (
      update.install as unknown as { mock: { invocationCallOrder: number[] } }
    ).mock.invocationCallOrder[0];
    expect(downloadOrder).toBeLessThan(installOrder);
  });
});
