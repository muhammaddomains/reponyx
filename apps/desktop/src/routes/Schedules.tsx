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
import { Badge, Button, Card } from "@reponyx/ui";

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
      return "Hours between runs, e.g. 6 (1–168).";
    case "daily":
      return "Time of day, 24-hour, e.g. 02:30.";
    case "weekly":
      return "Weekdays (1=Mon … 7=Sun) and time, e.g. 1,3,5 02:30.";
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
                      onChange={(e) =>
                        setEditing({
                          ...editing,
                          intervalType: e.target.value as IntervalType,
                        })
                      }
                    >
                      <option value="interval_hours">Every N hours</option>
                      <option value="daily">Daily</option>
                      <option value="weekly">Weekly</option>
                      <option value="custom">Every N minutes</option>
                    </select>
                    <input
                      className={inputClass}
                      type="text"
                      placeholder={cadenceHint(editing.intervalType)}
                      value={editing.intervalValue}
                      onChange={(e) =>
                        setEditing({
                          ...editing,
                          intervalValue: e.target.value,
                        })
                      }
                    />
                  </div>
                  <p className="text-xs text-zinc-500">
                    {cadenceHint(editing.intervalType)}
                  </p>
                  <div className="flex gap-3">
                    <input
                      className={inputClass}
                      type="text"
                      placeholder="Timezone, e.g. America/Phoenix"
                      value={editing.timezone}
                      onChange={(e) =>
                        setEditing({ ...editing, timezone: e.target.value })
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
                    onClick={() => {
                      if (
                        window.confirm(
                          `Delete this ${schedule.scope} schedule (${cadenceDescription(schedule)})? Repositories it governed fall back to the next broader schedule.`,
                        )
                      ) {
                        void run(
                          () => deleteSchedule(schedule.id),
                          () => setNotice("Schedule deleted."),
                        );
                      }
                    }}
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
              onChange={(e) =>
                setIntervalType(e.target.value as IntervalType)
              }
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
          <input
            className={inputClass}
            type="text"
            placeholder={cadenceHint(intervalType)}
            value={intervalValue}
            onChange={(e) => setIntervalValue(e.target.value)}
          />
          <p className="text-xs text-zinc-500">
            {cadenceHint(intervalType)} The backend validates the cadence
            and timezone before saving — an invalid value is rejected with
            a real error, never silently stored.
          </p>
          <div className="flex gap-3">
            <input
              className={inputClass}
              type="text"
              placeholder="Timezone, e.g. America/Phoenix"
              value={timezone}
              onChange={(e) => setTimezone(e.target.value)}
            />
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
    </div>
  );
}
