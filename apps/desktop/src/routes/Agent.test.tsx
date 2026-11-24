import { render, screen } from "@testing-library/react";
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
  agentPause: vi.fn(),
  agentResume: vi.fn(),
  registerAgentAutostart: vi.fn(),
  unregisterAgentAutostart: vi.fn(),
  isTauriAvailable: () => true,
}));

import { agentHealth } from "@reponyx/api-client";

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
