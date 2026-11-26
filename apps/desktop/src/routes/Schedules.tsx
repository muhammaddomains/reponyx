import { useCallback, useEffect, useState } from "react";
import {
  createSchedule,
  deleteSchedule,
  effectiveSchedule,
  listAccounts,
  listSchedules,
  materializeDue,
  previewNextRuns,
  updateSchedule,
} from "@reponyx/api-client";
import type {
  Account,
  EffectiveSchedule,
  IntervalType,
  MaterializeReport,
  NewScheduleInput,
  NextRunPreview,
  ScheduleScope,
  ScheduleWithNextRun,
} from "@reponyx/shared";
import { Badge, Button, Card, ConfirmDialog } from "@reponyx/ui";

const inputClass =
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function commandMessage(error: unknown): string {
  if (typeof error === "object" && error !== null) {
    const record = error as { code?: unknown; message?: unknown };
    if (typeof record.message === "string") {
      return typeof record.code === "string"
        ? `${record.code}: ${record.message}`
        : record.message;
    }
  }
  return error instanceof Error ? error.message : String(error);
}

function browserTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

/** Every IANA timezone the runtime knows, for the timezone picker.
 *  Falls back to the browser zone + UTC where `supportedValuesOf`
 *  is unavailable, so the select never renders empty. */
function allTimezones(): string[] {
  try {
    const zones = Intl.supportedValuesOf("timeZone");
    if (zones.length > 0) {
      // `supportedValuesOf` lists `Etc/UTC` but not the `UTC` alias
      // the app already uses as a fallback — include it explicitly.
      return [...new Set([...zones, "UTC"])].sort();
    }
  } catch {
    // Fall through to the fallback below.
  }
  const fallback = browserTimezone();
  return fallback === "UTC" ? ["UTC"] : [fallback, "UTC"];
}

const TIMEZONES = allTimezones();

/** Timezone picker: every IANA zone, so the user picks instead of
 *  typing a name by hand. A stored zone outside the list (e.g. an
 *  older value) is kept as the first option so the select never
 *  renders blank. */
function TimezoneSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const options = TIMEZONES.includes(value)
    ? TIMEZONES
    : [value, ...TIMEZONES];
  return (
    <select
      className={inputClass}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      aria-label="Timezone"
    >
      {options.map((zone) => (
        <option key={zone} value={zone}>
          {zone}
        </option>
      ))}
    </select>
  );
}

const WEEKDAY_NAMES = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];

/** Sensible value when the user switches to a cadence type, so the
 *  previous type's value (e.g. "6") never leaks into the new one —
 *  that was the old bug: the one text box kept "6" for Daily and
 *  Weekly, where "6" means nothing. */
function defaultIntervalValue(intervalType: IntervalType): string {
  switch (intervalType) {
    case "interval_hours":
      return "6";
    case "daily":
      return "02:30";
    case "weekly":
      return "1 02:30";
    case "custom":
      return "90";
  }
}

/** Parse a stored weekly value ("1,3,5 02:30") into its days
 *  (1=Mon … 7=Sun) and time. Malformed parts fall back to Monday
 *  at 02:30 — the backend rejects anything still invalid on save,
 *  so this never invents a schedule. */
function parseWeeklyValue(value: string): { days: number[]; time: string } {
  const [daysPart, timePart] = value.split(" ");
  const days = (daysPart ?? "")
    .split(",")
    .map((day) => Number(day))
    .filter((day) => Number.isInteger(day) && day >= 1 && day <= 7);
  const time = /^\d{2}:\d{2}$/.test(timePart ?? "") ? timePart! : "02:30";
  return { days, time };
}

/** The cadence value, as a proper control for the chosen type
 *  instead of one free-text box for all four: a dropdown of 1–24
 *  hours, a time picker for Daily, weekday toggles by name plus a
 *  time picker for Weekly. "Every N minutes" keeps its text box —
 *  its range (1–10080) is too wide for a dropdown. The value the
 *  component reports is always the backend string ("6", "02:30",
 *  "1,4 02:30", "90"), so create/update need no change. */
function CadenceInput({
  intervalType,
  value,
  onChange,
}: {
  intervalType: IntervalType;
  value: string;
  onChange: (value: string) => void;
}) {
  if (intervalType === "interval_hours") {
    // A stored value outside 1–24 (made by the old text box) is
    // kept as an option, like TimezoneSelect does — the select must
    // never render blank or silently drop the current value.
    const hours = Array.from({ length: 24 }, (_, i) => i + 1);
    const current = Number(value);
    const options =
      Number.isInteger(current) && current >= 1 && !hours.includes(current)
        ? [...hours, current].sort((a, b) => a - b)
        : hours;
    return (
      <select
        className={inputClass}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Hours between runs"
      >
        {options.map((hour) => (
          <option key={hour} value={String(hour)}>
            {hour} hour{hour === 1 ? "" : "s"}
          </option>
        ))}
      </select>
    );
  }
  if (intervalType === "daily") {
    return (
      <input
        className={inputClass}
        type="time"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Time of day"
      />
    );
  }
  if (intervalType === "weekly") {
    const { days, time } = parseWeeklyValue(value);
    const toggleDay = (day: number) => {
      const next = days.includes(day)
        ? days.filter((d) => d !== day)
        : [...days, day].sort((a, b) => a - b);
      onChange(`${next.join(",")} ${time}`);
    };
    return (
      <div className="space-y-3">
        <div className="flex flex-wrap gap-2" role="group" aria-label="Weekdays">
          {WEEKDAY_NAMES.map((name, index) => {
            const day = index + 1;
            const selected = days.includes(day);
            return (
              <button
                key={name}
                type="button"
                aria-pressed={selected}
                onClick={() => toggleDay(day)}
                className={
                  selected
                    ? "rounded-md bg-zinc-900 px-3 py-2 text-sm text-white dark:bg-white dark:text-zinc-900"
                    : "rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                }
              >
                {name}
              </button>
            );
          })}
        </div>
        <input
          className={inputClass}
          type="time"
          value={time}
          onChange={(e) => onChange(`${days.join(",")} ${e.target.value}`)}
          aria-label="Time of day"
        />
      </div>
    );
  }
  return (
    <input
      className={inputClass}
      type="text"
      placeholder={cadenceHint(intervalType)}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/** The one invalid value the new controls can still produce: a
 *  weekly schedule with no weekday picked. The backend would
 *  reject it too, but naming it here saves a round trip. */
function cadenceError(intervalType: IntervalType, value: string): string | null {
  if (intervalType === "weekly" && parseWeeklyValue(value).days.length === 0) {
    return "Pick at least one weekday for a weekly schedule.";
  }
  return null;
}

/** Human-readable cadence. Falls back to the raw value when the
 *  stored cadence does not parse — the backend is the source of
 *  truth for validity, never this display helper. */
function cadenceDescription(schedule: ScheduleWithNextRun): string {
  const value = schedule.intervalValue ?? "";
  switch (schedule.intervalType) {
    case "interval_hours":
      return `Every ${value || "?"} hour${value === "1" ? "" : "s"}`;
    case "daily":
      return `Daily at ${value || "?"}`;
    case "weekly": {
      const [days, time] = value.split(" ");
      const names = (days ?? "")
        .split(",")
        .map((d) => WEEKDAYS[Number(d) - 1] ?? d)
        .join(", ");
      return `${names || "?"} at ${time ?? "?"}`;
    }
    case "custom":
      return `Every ${value || "?"} minutes`;
  }
}

function cadenceHint(intervalType: IntervalType): string {
  switch (intervalType) {
    case "interval_hours":
      return "Hours between runs, e.g. 6 (1–24).";
    case "daily":
      return "Time of day, 24-hour, e.g. 02:30.";
    case "weekly":
      return "Pick weekdays by name and a time — e.g. Monday and Thursday at 02:30.";
    case "custom":
      return "Minutes between runs, e.g. 90 (1–10080).";
  }
}

function scopeLabel(
  schedule: ScheduleWithNextRun,
  accounts: Account[],
): string {
  if (schedule.scope === "global") return "Global";
  if (schedule.scope === "account") {
    const account = accounts.find((a) => a.id === schedule.accountId);
    return `Account: ${account?.username ?? schedule.accountId ?? "?"}`;
  }
  return `Repository: ${schedule.repositoryId ?? "?"}`;
}

function formatRunAt(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

interface EditDraft {
  id: string;
  intervalType: IntervalType;
  intervalValue: string;
  timezone: string;
  enabled: boolean;
}

/** Schedules: CRUD over the Phase 7 scheduler backend, next-run
 *  previews, running due schedules now, and the effective-schedule
 *  lookup (repository > account > global precedence). */
export default function Schedules() {
  const [schedules, setSchedules] = useState<ScheduleWithNextRun[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [accountsError, setAccountsError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);

  const [previews, setPreviews] = useState<Record<string, NextRunPreview[]>>(
    {},
  );
  const [previewBusyId, setPreviewBusyId] = useState<string | null>(null);
  const [report, setReport] = useState<MaterializeReport | null>(null);

  const [scope, setScope] = useState<ScheduleScope>("global");
  const [accountId, setAccountId] = useState("");
  const [repositoryId, setRepositoryId] = useState("");
  const [intervalType, setIntervalType] =
    useState<IntervalType>("interval_hours");
  const [intervalValue, setIntervalValue] = useState("6");
  const [timezone, setTimezone] = useState(browserTimezone());
  const [enabled, setEnabled] = useState(true);

  const [editing, setEditing] = useState<EditDraft | null>(null);

  // Pending destructive confirmation. The action only runs from the
  // dialog's OK button — never from `window.confirm`, which the
  // Tauri webview answers immediately (truthy) while its native
  // dialog is still open, so Cancel could not stop the delete.
  const [pendingConfirm, setPendingConfirm] = useState<null | {
    title: string;
    message: string;
    confirmLabel: string;
    action: () => void;
  }>(null);

  const [lookupRepoId, setLookupRepoId] = useState("");
  const [effective, setEffective] = useState<EffectiveSchedule | null>(null);
  const [effectiveChecked, setEffectiveChecked] = useState(false);

  const refresh = useCallback(async () => {
    const [schedulesResult, accountsResult] = await Promise.allSettled([
      listSchedules(),
      listAccounts(),
    ]);
    if (accountsResult.status === "fulfilled") {
      setAccounts(accountsResult.value);
      setAccountsError(null);
    } else {
      setAccounts([]);
      setAccountsError(commandMessage(accountsResult.reason));
    }
    if (schedulesResult.status === "rejected") {
      throw schedulesResult.reason;
    }
    setSchedules(schedulesResult.value);
  }, []);

  useEffect(() => {
    let cancelled = false;
    refresh()
      .catch((e: unknown) => {
        if (!cancelled) setError(commandMessage(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [refresh]);

  async function run<T>(action: () => Promise<T>, done?: (value: T) => void) {
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      const value = await action();
      await refresh();
      done?.(value);
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleCreate() {
    if (scope === "account" && !accountId) {
      setError("Pick an account for an account-scoped schedule.");
      return;
    }
    if (scope === "repository" && !repositoryId.trim()) {
      setError("Enter a repository id for a repository-scoped schedule.");
      return;
    }
    if (!intervalValue.trim()) {
      setError("Enter the cadence value.");
      return;
    }
    const valueError = cadenceError(intervalType, intervalValue);
    if (valueError) {
      setError(valueError);
      return;
    }
    if (!timezone.trim()) {
      setError("Enter a timezone (IANA name, e.g. America/Phoenix).");
      return;
    }
    const input: NewScheduleInput = {
      scope,
      accountId: scope === "account" ? accountId : null,
      repositoryId: scope === "repository" ? repositoryId.trim() : null,
      intervalType,
      intervalValue: intervalValue.trim(),
      timezone: timezone.trim(),
      enabled,
    };
    await run(
      () => createSchedule(input),
      (created) => {
        setNotice(
          `Schedule created — next run ${formatRunAt(created.computedNextRun ?? created.nextRunAt)}.`,
        );
        setRepositoryId("");
        setIntervalValue("6");
      },
    );
  }

  function startEditing(schedule: ScheduleWithNextRun) {
    setEditing({
      id: schedule.id,
      intervalType: schedule.intervalType,
      intervalValue: schedule.intervalValue ?? "",
      timezone: schedule.timezone,
      enabled: schedule.enabled,
    });
  }

  async function handleSaveEdit() {
    if (!editing) return;
    const valueError = cadenceError(editing.intervalType, editing.intervalValue);
    if (valueError) {
      setError(valueError);
      return;
    }
    await run(
      () =>
        updateSchedule({
          id: editing.id,
          intervalType: editing.intervalType,
          intervalValue: editing.intervalValue.trim(),
          timezone: editing.timezone.trim(),
          enabled: editing.enabled,
        }),
      () => {
        setEditing(null);
        setNotice("Schedule updated.");
      },
    );
  }

  async function handlePreview(schedule: ScheduleWithNextRun) {
    if (previews[schedule.id]) {
      setPreviews((current) => {
        const next = { ...current };
        delete next[schedule.id];
        return next;
      });
      return;
    }
    setError(null);
    setPreviewBusyId(schedule.id);
    try {
      const runs = await previewNextRuns({
        scheduleId: schedule.id,
        count: 5,
        displayTimezone: schedule.timezone,
      });
      setPreviews((current) => ({ ...current, [schedule.id]: runs }));
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setPreviewBusyId(null);
    }
  }

  async function handleMaterialize() {
    await run(
      () => materializeDue(),
      (value) => {
        setReport(value);
        setNotice(
          `Checked ${value.checked} schedule${value.checked === 1 ? "" : "s"} — ${value.createdJobIds.length} job${value.createdJobIds.length === 1 ? "" : "s"} created.`,
        );
      },
    );
  }

  async function handleLookup() {
    if (!lookupRepoId.trim()) {
      setError("Enter a repository id to look up its effective schedule.");
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const value = await effectiveSchedule(lookupRepoId.trim());
      setEffective(value);
      setEffectiveChecked(true);
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-6 p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Schedules</h1>
          <p className="text-sm text-zinc-500">
            When repositories sync. Precedence is repository &gt; account &gt;
            global; the agent materializes due schedules into sync jobs.
          </p>
        </div>
        <Button disabled={busy} onClick={() => void handleMaterialize()}>
          Run due schedules now
        </Button>
      </header>

      {error && (
        <Card className="border-red-200 bg-red-50 p-4 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200">
          {error}
        </Card>
      )}
      {notice && (
        <Card className="border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-200">
          {notice}
        </Card>
      )}

      {report && (
        <Card className="space-y-2 p-4">
          <h2 className="text-base font-medium">Last run now</h2>
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            Checked {report.checked} · primed {report.primed} · created{" "}
            {report.createdJobIds.length} job
            {report.createdJobIds.length === 1 ? "" : "s"} · skipped
            duplicate {report.skippedDuplicate} · sync-disabled{" "}
            {report.skippedSyncDisabled} · missing repository{" "}
            {report.skippedMissingRepository} · out of window{" "}
            {report.skippedOutOfWindow}
          </p>
          {report.errors.length > 0 && (
            <ul className="list-disc space-y-1 pl-5 text-sm text-red-800 dark:text-red-300">
              {report.errors.map((e) => (
                <li key={e.scheduleId}>
                  {e.scheduleId}: {e.message}
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}

      <section className="space-y-4">
        <h2 className="text-lg font-medium">All schedules</h2>
        {loading && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Loading schedules…
            </p>
          </Card>
        )}
        {!loading && schedules.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              No schedules yet. Create one below — a global schedule governs
              every repository unless a narrower one overrides it.
            </p>
          </Card>
        )}
        {schedules.map((schedule) => {
          const preview = previews[schedule.id];
          const isEditing = editing?.id === schedule.id;
          return (
            <Card key={schedule.id} className="space-y-3 p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <span className="font-medium">
                    {cadenceDescription(schedule)}
                  </span>
                  <span className="ml-2 text-sm text-zinc-500">
                    {scopeLabel(schedule, accounts)} · {schedule.timezone}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  {schedule.enabled ? (
                    <Badge variant="success">enabled</Badge>
                  ) : (
                    <Badge>disabled</Badge>
                  )}
                </div>
              </div>
              <p className="text-sm text-zinc-600 dark:text-zinc-400">
                Next run:{" "}
                {formatRunAt(
                  schedule.computedNextRun ?? schedule.nextRunAt,
                )}
              </p>
              {schedule.nextRunError && (
                <p className="text-sm text-red-800 dark:text-red-300">
                  Next run cannot be computed: {schedule.nextRunError}
                </p>
              )}

              {isEditing && editing ? (
                <div className="space-y-3 rounded-md border border-zinc-200 p-3 dark:border-zinc-800">
                  <div className="flex gap-3">
                    <select
                      className={inputClass}
                      value={editing.intervalType}
                      onChange={(e) => {
                        const next = e.target.value as IntervalType;
                        setEditing({
                          ...editing,
                          intervalType: next,
                          intervalValue: defaultIntervalValue(next),
                        });
                      }}
                    >
                      <option value="interval_hours">Every N hours</option>
                      <option value="daily">Daily</option>
                      <option value="weekly">Weekly</option>
                      <option value="custom">Every N minutes</option>
                    </select>
                    <CadenceInput
                      intervalType={editing.intervalType}
                      value={editing.intervalValue}
                      onChange={(value) =>
                        setEditing({ ...editing, intervalValue: value })
                      }
                    />
                  </div>
                  <p className="text-xs text-zinc-500">
                    {cadenceHint(editing.intervalType)}
                  </p>
                  <div className="flex gap-3">
                    <TimezoneSelect
                      value={editing.timezone}
                      onChange={(value) =>
                        setEditing({ ...editing, timezone: value })
                      }
                    />
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={editing.enabled}
                        onChange={(e) =>
                          setEditing({
                            ...editing,
                            enabled: e.target.checked,
                          })
                        }
                      />
                      Enabled
                    </label>
                  </div>
                  <div className="flex gap-2">
                    <Button disabled={busy} onClick={() => void handleSaveEdit()}>
                      Save
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={busy}
                      onClick={() => setEditing(null)}
                    >
                      Cancel
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="secondary"
                    disabled={busy}
                    onClick={() =>
                      run(
                        () =>
                          updateSchedule({
                            id: schedule.id,
                            enabled: !schedule.enabled,
                          }),
                        () =>
                          setNotice(
                            schedule.enabled
                              ? "Schedule disabled."
                              : "Schedule enabled.",
                          ),
                      )
                    }
                  >
                    {schedule.enabled ? "Disable" : "Enable"}
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={busy}
                    onClick={() => startEditing(schedule)}
                  >
                    Edit
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={previewBusyId === schedule.id}
                    onClick={() => void handlePreview(schedule)}
                  >
                    {preview
                      ? "Hide preview"
                      : previewBusyId === schedule.id
                        ? "Loading…"
                        : "Preview next runs"}
                  </Button>
                  <Button
                    variant="danger"
                    disabled={busy}
                    onClick={() =>
                      setPendingConfirm({
                        title: "Delete schedule",
                        message: `Delete this ${schedule.scope} schedule (${cadenceDescription(schedule)})? Repositories it governed fall back to the next broader schedule.`,
                        confirmLabel: "Delete",
                        action: () =>
                          void run(
                            () => deleteSchedule(schedule.id),
                            () => setNotice("Schedule deleted."),
                          ),
                      })
                    }
                  >
                    Delete
                  </Button>
                </div>
              )}

              {preview && (
                <ul className="list-disc space-y-1 pl-5 text-sm text-zinc-600 dark:text-zinc-400">
                  {preview.map((run) => (
                    <li key={run.utc}>
                      {run.local} ({run.timezone})
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          );
        })}
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-medium">New schedule</h2>
        <Card className="space-y-3 p-4">
          {accountsError && (
            <p className="text-sm text-amber-800 dark:text-amber-300">
              Accounts could not be loaded ({accountsError}) — account-scoped
              schedules need an account id typed below instead.
            </p>
          )}
          <div className="flex gap-3">
            <select
              className={inputClass}
              value={scope}
              onChange={(e) => setScope(e.target.value as ScheduleScope)}
            >
              <option value="global">Global — all repositories</option>
              <option value="account">Account</option>
              <option value="repository">Repository</option>
            </select>
            <select
              className={inputClass}
              value={intervalType}
              onChange={(e) => {
                const next = e.target.value as IntervalType;
                setIntervalType(next);
                setIntervalValue(defaultIntervalValue(next));
              }}
            >
              <option value="interval_hours">Every N hours</option>
              <option value="daily">Daily</option>
              <option value="weekly">Weekly</option>
              <option value="custom">Every N minutes</option>
            </select>
          </div>
          {scope === "account" &&
            (accounts.length > 0 ? (
              <select
                className={inputClass}
                value={accountId}
                onChange={(e) => setAccountId(e.target.value)}
              >
                <option value="">Pick an account…</option>
                {accounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.username}
                  </option>
                ))}
              </select>
            ) : (
              <input
                className={inputClass}
                type="text"
                placeholder="Account id (UUID)"
                value={accountId}
                onChange={(e) => setAccountId(e.target.value)}
              />
            ))}
          {scope === "repository" && (
            <input
              className={inputClass}
              type="text"
              placeholder="Repository id (UUID — from the repository row; the Repositories page arrives later in Phase 8)"
              value={repositoryId}
              onChange={(e) => setRepositoryId(e.target.value)}
            />
          )}
          <CadenceInput
            intervalType={intervalType}
            value={intervalValue}
            onChange={setIntervalValue}
          />
          <p className="text-xs text-zinc-500">
            {cadenceHint(intervalType)} The backend validates the cadence
            and timezone before saving — an invalid value is rejected with
            a real error, never silently stored.
          </p>
          <div className="flex gap-3">
            <TimezoneSelect value={timezone} onChange={setTimezone} />
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(e) => setEnabled(e.target.checked)}
              />
              Enabled
            </label>
          </div>
          <Button disabled={busy} onClick={() => void handleCreate()}>
            Create schedule
          </Button>
        </Card>
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-medium">Effective schedule</h2>
        <Card className="space-y-3 p-4">
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            Which schedule governs one repository — repository &gt; account
            &gt; global.
          </p>
          <div className="flex gap-3">
            <input
              className={inputClass}
              type="text"
              placeholder="Repository id (UUID)"
              value={lookupRepoId}
              onChange={(e) => setLookupRepoId(e.target.value)}
            />
            <Button disabled={busy} onClick={() => void handleLookup()}>
              Look up
            </Button>
          </div>
          {effectiveChecked &&
            (effective ? (
              <div className="space-y-1 text-sm">
                <p>
                  Governed by a {effective.schedule.scope} schedule:{" "}
                  {cadenceDescription(effective.schedule)} (
                  {effective.schedule.timezone})
                </p>
                <ul className="list-disc space-y-1 pl-5 text-zinc-600 dark:text-zinc-400">
                  {effective.nextRuns.map((run) => (
                    <li key={run}>{formatRunAt(run)}</li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="text-sm text-zinc-600 dark:text-zinc-400">
                No schedule governs this repository.
              </p>
            ))}
        </Card>
      </section>
      <ConfirmDialog
        open={pendingConfirm !== null}
        title={pendingConfirm?.title ?? ""}
        message={pendingConfirm?.message ?? ""}
        confirmLabel={pendingConfirm?.confirmLabel ?? "OK"}
        onConfirm={() => {
          const pending = pendingConfirm;
          setPendingConfirm(null);
          pending?.action();
        }}
        onCancel={() => setPendingConfirm(null)}
      />
    </div>
  );
}
