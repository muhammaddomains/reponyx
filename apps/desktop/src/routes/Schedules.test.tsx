import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import Schedules from "./Schedules";

vi.mock("@reponyx/api-client", () => ({
  createSchedule: vi.fn(),
  deleteSchedule: vi.fn(),
  effectiveSchedule: vi.fn(),
  listAccounts: vi.fn(),
  listSchedules: vi.fn(),
  materializeDue: vi.fn(),
  previewNextRuns: vi.fn(),
  updateSchedule: vi.fn(),
}));

import {
  createSchedule,
  listAccounts,
  listSchedules,
} from "@reponyx/api-client";
import type { ScheduleWithNextRun } from "@reponyx/shared";

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listAccounts).mockResolvedValue([]);
  vi.mocked(listSchedules).mockResolvedValue([]);
});

describe("Schedules page — timezone picker", () => {
  it("offers every IANA timezone in a dropdown, defaulting to the browser zone", async () => {
    render(<Schedules />);
    const select = await screen.findByLabelText("Timezone");
    expect(select.tagName).toBe("SELECT");
    const options = Array.from(
      (select as HTMLSelectElement).options,
    ).map((option) => option.value);
    expect(options).toContain("America/Phoenix");
    expect(options).toContain("UTC");
    expect(options.length).toBeGreaterThan(100);
    expect((select as HTMLSelectElement).value).toBe(
      Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
    );
  });

  it("creates the schedule with the timezone picked from the dropdown", async () => {
    const created: ScheduleWithNextRun = {
      id: "sched-1",
      scope: "global",
      accountId: null,
      repositoryId: null,
      intervalType: "interval_hours",
      intervalValue: "6",
      timezone: "America/Phoenix",
      enabled: true,
      nextRunAt: "2026-11-25T06:00:00Z",
      computedNextRun: "2026-11-25T06:00:00Z",
      createdAt: "2026-11-24T00:00:00Z",
      updatedAt: "2026-11-24T00:00:00Z",
    };
    vi.mocked(createSchedule).mockResolvedValue(created);
    render(<Schedules />);
    const select = await screen.findByLabelText("Timezone");
    fireEvent.change(select, { target: { value: "America/Phoenix" } });
    fireEvent.click(screen.getByRole("button", { name: "Create schedule" }));
    expect(vi.mocked(createSchedule)).toHaveBeenCalledWith(
      expect.objectContaining({ timezone: "America/Phoenix" }),
    );
    expect(await screen.findByText(/Schedule created/)).toBeInTheDocument();
  });
});
