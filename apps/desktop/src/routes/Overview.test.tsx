import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Overview from "./Overview";

/** Audit F13: the Overview used to show zeros while the counts
 *  were still loading, and a failed load left a permanent error
 *  banner with no way to retry. Loading shows "…", a failure shows
 *  a Retry button, and a successful retry clears the error. */

vi.mock("@reponyx/api-client", () => ({
  getDashboardCounts: vi.fn(),
  getGitStatus: vi.fn(),
}));

import { getDashboardCounts, getGitStatus } from "@reponyx/api-client";

const counts = {
  accounts: 3,
  repositories: 9,
  synced: 1,
  syncing: 0,
  failed: 0,
  pending_jobs: 2,
};

beforeEach(() => {
  vi.mocked(getGitStatus).mockResolvedValue({
    available: true,
    supported: true,
    version: "git version 2.40.0",
    path: "git",
    error: null,
  });
});

describe("Overview loading state", () => {
  it("shows … while the counts are loading, not zeros", async () => {
    vi.mocked(getDashboardCounts).mockReturnValue(new Promise(() => {}));
    render(<Overview />);
    // All six stats are still loading.
    expect(screen.getAllByText("…")).toHaveLength(6);
    expect(screen.queryByText("0")).not.toBeInTheDocument();
  });

  it("shows the real counts once they load", async () => {
    vi.mocked(getDashboardCounts).mockResolvedValue(counts);
    render(<Overview />);
    await screen.findByText("3");
    expect(screen.getByText("9")).toBeInTheDocument();
  });
});

describe("Overview error retry", () => {
  it("retries a failed load and clears the error on success", async () => {
    vi.mocked(getDashboardCounts).mockRejectedValue(
      new Error("database is unavailable"),
    );
    render(<Overview />);
    expect(
      await screen.findByText(/Could not load dashboard data/),
    ).toBeInTheDocument();
    vi.mocked(getDashboardCounts).mockResolvedValue(counts);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByText("3");
    expect(
      screen.queryByText(/Could not load dashboard data/),
    ).not.toBeInTheDocument();
  });

  it("does not mention Phase 8 in the next steps", async () => {
    vi.mocked(getDashboardCounts).mockResolvedValue(counts);
    render(<Overview />);
    await screen.findByText("3");
    expect(screen.queryByText(/Phase 8/)).not.toBeInTheDocument();
  });
});
