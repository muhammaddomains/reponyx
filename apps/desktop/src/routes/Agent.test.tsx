import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Agent from "./Agent";

/** The Agent page used to render a Tauri error object with
 *  `String(error)`, which shows "[object Object]" — every other
 *  page formats the backend's { code, message } value. A rejected
 *  health check (e.g. the agent binary missing) must show the real
 *  message. */

vi.mock("@reponyx/api-client", () => ({
  agentHealth: vi.fn(),
  ensureAgentRunning: vi.fn(),
  stopAgent: vi.fn(),
  agentPause: vi.fn(),
  agentResume: vi.fn(),
  registerAgentAutostart: vi.fn(),
  unregisterAgentAutostart: vi.fn(),
  autostartStatus: vi.fn(),
  isTauriAvailable: () => true,
}));

import {
  agentHealth,
  stopAgent,
  registerAgentAutostart,
  autostartStatus,
} from "@reponyx/api-client";

const healthy = {
  reachable: true,
  status: {
    generation: 1795528861981,
    startedAt: "2026-11-24T14:01:02.011906900+00:00",
    protocolVersion: 1,
    paused: false,
    pendingJobs: 0,
    runningJobs: 0,
    workers: 5,
    lastMaterializeAt: "2026-11-24T14:19:02.028317500+00:00",
    lastMaterializeCreated: 0,
  },
  message: "ok",
};

describe("Agent page health badge (audit F9)", () => {
  it("shows Checking… before the first poll settles", async () => {
    // A promise that never settles: the first poll is still in
    // flight, so the badge must not claim "Not running" yet.
    vi.mocked(agentHealth).mockReturnValue(new Promise(() => {}));
    vi.mocked(autostartStatus).mockResolvedValue(false);
    render(<Agent />);
    expect(screen.getByText("Checking…")).toBeInTheDocument();
  });

  it("keeps the last known health when a later poll fails", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(false);
    render(<Agent />);
    await screen.findByText("Running");
    // The next poll fails (e.g. a transient probe miss): the error
    // banner appears, but the badge keeps the last known state
    // instead of flipping to "Not running".
    vi.mocked(agentHealth).mockRejectedValue({
      code: "Timeout",
      message: "agent did not answer",
    });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(
      await screen.findByText("Timeout: agent did not answer"),
    ).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
  });
});

describe("Agent page error rendering", () => {
  it("shows the backend error message instead of [object Object]", async () => {
    vi.mocked(agentHealth).mockRejectedValue({
      code: "NotFound",
      message: "reponyx-agent executable not found",
    });
    render(<Agent />);
    expect(
      await screen.findByText("NotFound: reponyx-agent executable not found"),
    ).toBeInTheDocument();
    expect(screen.queryByText("[object Object]")).not.toBeInTheDocument();
  });
});

describe("Agent page start/stop controls", () => {
  it("disables Start and enables Stop while the agent is running", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(false);
    render(<Agent />);
    await screen.findByText("1795528861981");
    const start = screen.getByRole("button", { name: "Start agent" });
    const stop = screen.getByRole("button", { name: "Stop agent" });
    expect(start).toBeDisabled();
    expect(stop).not.toBeDisabled();
  });

  it("enables only Pause while the agent is running and not paused", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(false);
    render(<Agent />);
    await screen.findByText("1795528861981");
    expect(screen.getByRole("button", { name: "Pause" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "Resume" })).toBeDisabled();
  });

  it("enables only Resume while the agent is paused", async () => {
    vi.mocked(agentHealth).mockResolvedValue({
      ...healthy,
      status: { ...healthy.status, paused: true },
    });
    vi.mocked(autostartStatus).mockResolvedValue(false);
    render(<Agent />);
    await screen.findByText("1795528861981");
    expect(screen.getByRole("button", { name: "Pause" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Resume" })).not.toBeDisabled();
  });

  it("shows the stop result as a notice", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(false);
    vi.mocked(stopAgent).mockResolvedValue("Background agent stopped.");
    render(<Agent />);
    await screen.findByText("1795528861981");
    fireEvent.click(screen.getByRole("button", { name: "Stop agent" }));
    expect(
      await screen.findByText("Background agent stopped."),
    ).toBeInTheDocument();
  });

  it("enables only Disable start at login while start at login is enabled", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(true);
    render(<Agent />);
    await screen.findByText(/Start at login: enabled/);
    expect(
      screen.getByRole("button", { name: "Enable start at login" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Disable start at login" }),
    ).not.toBeDisabled();
  });

  it("enables only Enable start at login while start at login is disabled", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(false);
    render(<Agent />);
    await screen.findByText(/Start at login: disabled/);
    expect(
      screen.getByRole("button", { name: "Enable start at login" }),
    ).not.toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Disable start at login" }),
    ).toBeDisabled();
  });

  it("shows the autostart result and the updated autostart state", async () => {
    vi.mocked(agentHealth).mockResolvedValue(healthy);
    vi.mocked(autostartStatus).mockResolvedValue(false);
    vi.mocked(registerAgentAutostart).mockResolvedValue(
      "Registered RepoNyxBackgroundAgent (ONLOGON, Task Scheduler)",
    );
    render(<Agent />);
    expect(
      await screen.findByText("Start at login:", { exact: false }),
    ).toBeInTheDocument();
    vi.mocked(autostartStatus).mockResolvedValue(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Enable start at login" }),
    );
    expect(
      await screen.findByText(
        "Registered RepoNyxBackgroundAgent (ONLOGON, Task Scheduler)",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByText(/enabled/)).toBeInTheDocument();
  });
});
