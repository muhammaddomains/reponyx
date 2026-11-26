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

describe("Schedules page — cadence controls", () => {
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

  function typeSelect() {
    return screen.getByDisplayValue("Every N hours") as HTMLSelectElement;
  }

  it("offers 1–24 hours in a dropdown, defaulting to 6", async () => {
    render(<Schedules />);
    const select = await screen.findByLabelText("Hours between runs");
    expect(select.tagName).toBe("SELECT");
    const values = Array.from(
      (select as HTMLSelectElement).options,
    ).map((option) => option.value);
    expect(values).toEqual(
      Array.from({ length: 24 }, (_, i) => String(i + 1)),
    );
    expect((select as HTMLSelectElement).value).toBe("6");
  });

  it("creates the schedule with the hours picked from the dropdown", async () => {
    vi.mocked(createSchedule).mockResolvedValue(created);
    render(<Schedules />);
    const select = await screen.findByLabelText("Hours between runs");
    fireEvent.change(select, { target: { value: "12" } });
    fireEvent.click(screen.getByRole("button", { name: "Create schedule" }));
    expect(vi.mocked(createSchedule)).toHaveBeenCalledWith(
      expect.objectContaining({
        intervalType: "interval_hours",
        intervalValue: "12",
      }),
    );
  });

  it("switching to Daily resets the value to a time picker at 02:30", async () => {
    render(<Schedules />);
    fireEvent.change(typeSelect(), { target: { value: "daily" } });
    const time = await screen.findByLabelText("Time of day");
    expect(time.tagName).toBe("INPUT");
    expect((time as HTMLInputElement).type).toBe("time");
    expect((time as HTMLInputElement).value).toBe("02:30");
  });

  it("creates a daily schedule with the time picked", async () => {
    vi.mocked(createSchedule).mockResolvedValue(created);
    render(<Schedules />);
    fireEvent.change(typeSelect(), { target: { value: "daily" } });
    const time = await screen.findByLabelText("Time of day");
    fireEvent.change(time, { target: { value: "14:45" } });
    fireEvent.click(screen.getByRole("button", { name: "Create schedule" }));
    expect(vi.mocked(createSchedule)).toHaveBeenCalledWith(
      expect.objectContaining({
        intervalType: "daily",
        intervalValue: "14:45",
      }),
    );
  });

  it("weekly offers weekdays by name, and creates with the picked days and time", async () => {
    vi.mocked(createSchedule).mockResolvedValue(created);
    render(<Schedules />);
    fireEvent.change(typeSelect(), { target: { value: "weekly" } });
    const monday = await screen.findByRole("button", { name: "Monday" });
    const thursday = screen.getByRole("button", { name: "Thursday" });
    // Monday is selected by default ("1 02:30"); add Thursday.
    expect(monday.getAttribute("aria-pressed")).toBe("true");
    expect(thursday.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(thursday);
    const time = screen.getByLabelText("Time of day");
    fireEvent.change(time, { target: { value: "09:00" } });
    fireEvent.click(screen.getByRole("button", { name: "Create schedule" }));
    expect(vi.mocked(createSchedule)).toHaveBeenCalledWith(
      expect.objectContaining({
        intervalType: "weekly",
        intervalValue: "1,4 09:00",
      }),
    );
  });

  it("refuses a weekly schedule with no weekday picked", async () => {
    render(<Schedules />);
    fireEvent.change(typeSelect(), { target: { value: "weekly" } });
    const monday = await screen.findByRole("button", { name: "Monday" });
    fireEvent.click(monday);
    fireEvent.click(screen.getByRole("button", { name: "Create schedule" }));
    expect(vi.mocked(createSchedule)).not.toHaveBeenCalled();
    expect(
      await screen.findByText(/Pick at least one weekday/),
    ).toBeInTheDocument();
  });
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
