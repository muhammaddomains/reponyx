import { useCallback, useEffect, useState } from "react";
import {
  createDatabaseBackup,
  deleteDatabaseBackup,
  exportConfig,
  exportDiagnostics,
  getSettings,
  importConfig,
  listDatabaseBackups,
  openSupportFolder,
  restoreDatabaseBackup,
  setSetting,
} from "@reponyx/api-client";
import type {
  ConfigExport,
  DatabaseBackup,
  DiagnosticsExport,
  ImportReport,
  Settings,
} from "@reponyx/shared";
import { Button, Card, ConfirmDialog } from "@reponyx/ui";
import { applyTheme, parseTheme } from "../lib/theme";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatDate(value: string): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

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

const selectClass =
  "rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900";
const inputClass =
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-2 font-mono text-sm dark:border-zinc-700 dark:bg-zinc-900";

/** Settings: the app's real configuration — every control here
 *  reads and writes a setting the code actually uses. General:
 *  theme (`ui.theme`, applied on startup). Notifications: a
 *  system notification when a sync job finishes or fails
 *  (`notifications.enabled`, on unless set to `false`). Sync: the default mode
 *  for newly discovered repositories (`sync.default_mode`) and how
 *  many syncs run at once (`sync.concurrency`, takes effect when
 *  the agent next starts). Storage: the archive root
 *  (`storage.root`). Advanced is read-only information.
 *  Diagnostics & backup (Phase 9) exports a redacted support file
 *  and manages consistent database snapshots. Import & export
 *  (Phase 9) moves settings, repository sync choices, and
 *  schedules to another machine in a versioned JSON file. */
export default function Settings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState<string | null>(null);
  const [backups, setBackups] = useState<DatabaseBackup[]>([]);
  const [lastExport, setLastExport] = useState<DiagnosticsExport | null>(
    null,
  );
  const [lastConfigExport, setLastConfigExport] =
    useState<ConfigExport | null>(null);
  const [lastImport, setLastImport] = useState<ImportReport | null>(null);
  const [diagBusy, setDiagBusy] = useState(false);

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

  const [theme, setTheme] = useState("system");
  const [notifications, setNotifications] = useState(true);
  const [mode, setMode] = useState("mirror");
  const [concurrency, setConcurrency] = useState("5");
  const [storageRoot, setStorageRoot] = useState("");

  const refresh = useCallback(async () => {
    const loaded = await getSettings();
    setSettings(loaded);
    setTheme(parseTheme(loaded.theme));
    setNotifications(loaded.notificationsEnabled ?? true);
    setMode(loaded.defaultSyncMode ?? "mirror");
    setConcurrency(String(loaded.concurrency ?? 5));
    setStorageRoot(loaded.storageRoot ?? "");
    return loaded;
  }, []);

  const refreshBackups = useCallback(async () => {
    setBackups(await listDatabaseBackups());
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
    refreshBackups().catch(() => {
      // Backups are a convenience; the list can stay empty.
    });
    return () => {
      cancelled = true;
    };
  }, [refresh, refreshBackups]);

  async function save(key: string, value: string, label: string) {
    setError(null);
    setNotice(null);
    setSaving(key);
    try {
      const result = await setSetting(key, value);
      // `storage.root` returns the migration summary ("Storage root
      // saved. Moved 3 archive(s)..."); other keys return the saved
      // value, so the generic notice is used for those.
      setNotice(key === "storage.root" ? result : `${label} saved.`);
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setSaving(null);
    }
  }

  async function handleThemeChange(value: string) {
    const next = parseTheme(value);
    setTheme(next);
    applyTheme(next);
    await save("ui.theme", next, "Theme");
  }

  async function handleNotificationsChange(value: boolean) {
    setNotifications(value);
    await save("notifications.enabled", String(value), "Notifications");
  }

  function handleConcurrencySave() {
    const parsed = Number(concurrency);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 10) {
      setError("Concurrent syncs must be a whole number between 1 and 10.");
      return;
    }
    void save("sync.concurrency", String(parsed), "Concurrent syncs");
  }

  async function handleExportDiagnostics() {
    setError(null);
    setNotice(null);
    setDiagBusy(true);
    try {
      const exported = await exportDiagnostics();
      setLastExport(exported);
      setNotice(
        `Diagnostics exported (${formatBytes(exported.bytes)}). It holds versions, counts, storage sizes and a redacted log tail — no credentials and no repository contents.`,
      );
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiagBusy(false);
    }
  }

  async function handleCreateBackup() {
    setError(null);
    setNotice(null);
    setDiagBusy(true);
    try {
      const backup = await createDatabaseBackup();
      setNotice(
        `Database backed up (${formatBytes(backup.bytes)}). A restore replaces the live database at the next startup.`,
      );
      await refreshBackups();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiagBusy(false);
    }
  }

  async function handleDeleteBackup(backup: DatabaseBackup) {
    setError(null);
    setNotice(null);
    setDiagBusy(true);
    try {
      await deleteDatabaseBackup(backup.path);
      setNotice("Backup deleted.");
      await refreshBackups();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiagBusy(false);
    }
  }

  async function handleRestoreBackup(backup: DatabaseBackup) {
    setError(null);
    setNotice(null);
    setDiagBusy(true);
    try {
      const staged = await restoreDatabaseBackup(backup.path);
      setNotice(
        `Restore staged. Quit RepoNyx completely and relaunch it to apply the backup. The current database is kept as a fallback.${staged.agentWasRunning ? " The background agent was stopped; it restarts when needed." : ""}`,
      );
      await refreshBackups();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiagBusy(false);
    }
  }

  async function handleExportConfig() {
    setError(null);
    setNotice(null);
    setDiagBusy(true);
    try {
      const exported = await exportConfig();
      if (exported) {
        setLastConfigExport(exported);
        setNotice(
          `Config exported (${formatBytes(exported.bytes)}). It holds settings, repository sync choices, and schedules — no credentials and no archives.`,
        );
      }
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiagBusy(false);
    }
  }

  async function handleImportConfig() {
    setError(null);
    setNotice(null);
    setDiagBusy(true);
    try {
      const report = await importConfig();
      if (report) {
        setLastImport(report);
        // Settings may have changed (including the theme), so
        // reload the form and apply the theme right away.
        const loaded = await refresh();
        applyTheme(parseTheme(loaded.theme));
        setNotice(
          `Config imported: ${report.settingsApplied} settings, ${report.repositoriesApplied} repositories, ${report.schedulesCreated} schedules created, ${report.schedulesUpdated} updated.${report.skipped.length > 0 ? ` ${report.skipped.length} entries were skipped — see below.` : ""}`,
        );
      }
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiagBusy(false);
    }
  }

  async function handleOpenFolder(folder: "diagnostics" | "backups") {
    setError(null);
    try {
      await openSupportFolder(folder);
    } catch (e: unknown) {
      setError(commandMessage(e));
    }
  }

  return (
    <div className="space-y-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Settings</h1>
        <p className="text-sm text-zinc-500">
          Configuration the app actually uses. Changes take effect as
          noted on each section.
        </p>
      </header>

      {error && (
        <Card className="border-red-200 bg-red-50 p-4 text-sm whitespace-pre-line text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200">
          {error}
        </Card>
      )}
      {notice && (
        <Card className="border-green-200 bg-green-50 p-4 text-sm text-green-800 dark:border-green-900 dark:bg-green-950 dark:text-green-200">
          {notice}
        </Card>
      )}

      {loading && (
        <Card className="p-4">
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            Loading settings…
          </p>
        </Card>
      )}

      {!loading && settings && (
        <>
          <Card className="space-y-3 p-4">
            <h2 className="text-lg font-semibold">General</h2>
            <div className="flex flex-wrap items-center gap-3">
              <label htmlFor="theme" className="text-sm font-medium">
                Theme
              </label>
              <select
                id="theme"
                className={selectClass}
                value={theme}
                disabled={saving === "ui.theme"}
                onChange={(e) => void handleThemeChange(e.target.value)}
              >
                <option value="system">System</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
              <span className="text-sm text-zinc-500">
                System follows your OS theme at startup.
              </span>
            </div>
          </Card>

          <Card className="space-y-3 p-4">
            <h2 className="text-lg font-semibold">Notifications</h2>
            <label className="flex flex-wrap items-center gap-3 text-sm">
              <input
                type="checkbox"
                checked={notifications}
                disabled={saving === "notifications.enabled"}
                onChange={(e) =>
                  void handleNotificationsChange(e.target.checked)
                }
              />
              <span className="font-medium">
                Notify me when a sync finishes or fails
              </span>
            </label>
            <p className="text-sm text-zinc-500">
              A system notification names the repository and the job.
              Turn it off to stay quiet — the Synchronization page
              still shows every job.
            </p>
          </Card>

          <Card className="space-y-4 p-4">
            <h2 className="text-lg font-semibold">Sync</h2>
            <div className="flex flex-wrap items-center gap-3">
              <label htmlFor="mode" className="text-sm font-medium">
                Default sync mode
              </label>
              <select
                id="mode"
                className={selectClass}
                value={mode}
                disabled={saving === "sync.default_mode"}
                onChange={(e) => setMode(e.target.value)}
              >
                <option value="mirror">Mirror</option>
                <option value="working">Working clone</option>
                <option value="both">Both</option>
              </select>
              <Button
                variant="secondary"
                disabled={saving === "sync.default_mode"}
                onClick={() =>
                  void save(
                    "sync.default_mode",
                    mode,
                    "Default sync mode",
                  )
                }
              >
                Save
              </Button>
            </div>
            <p className="text-sm text-zinc-500">
              Newly discovered repositories start with this mode.
              Repositories already in the list keep theirs — change
              one on the Repositories page.
            </p>
            <div className="flex flex-wrap items-center gap-3">
              <label
                htmlFor="concurrency"
                className="text-sm font-medium"
              >
                Concurrent syncs
              </label>
              <input
                id="concurrency"
                className="w-20 rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                type="number"
                min={1}
                max={10}
                step={1}
                value={concurrency}
                disabled={saving === "sync.concurrency"}
                onChange={(e) => setConcurrency(e.target.value)}
              />
              <Button
                variant="secondary"
                disabled={saving === "sync.concurrency"}
                onClick={handleConcurrencySave}
              >
                Save
              </Button>
              <span className="text-sm text-zinc-500">
                1–10. Takes effect when the agent next starts.
              </span>
            </div>
          </Card>

          <Card className="space-y-4 p-4">
            <h2 className="text-lg font-semibold">Storage</h2>
            <div className="space-y-2">
              <label htmlFor="root" className="text-sm font-medium">
                Storage root
              </label>
              <input
                id="root"
                className={inputClass}
                type="text"
                value={storageRoot}
                placeholder={settings.dataDir}
                disabled={saving === "storage.root"}
                onChange={(e) => setStorageRoot(e.target.value)}
              />
              <p className="text-sm text-zinc-500">
                {settings.storageRoot
                  ? "Where new archives are stored."
                  : "Not set explicitly — using the default data directory."}{" "}
                Changing it moves existing archives to the new root; new
                syncs go to the new root after the agent restarts. See the
                Storage page for what is stored where.
              </p>
              <Button
                variant="secondary"
                disabled={saving === "storage.root"}
                onClick={() =>
                  void save(
                    "storage.root",
                    storageRoot.trim() || settings.dataDir,
                    "Storage root",
                  )
                }
              >
                Save
              </Button>
            </div>
          </Card>

          <Card className="space-y-3 p-4">
            <h2 className="text-lg font-semibold">Advanced</h2>
            <dl className="space-y-2 text-sm">
              <div className="flex flex-wrap gap-2">
                <dt className="text-zinc-500">App version</dt>
                <dd className="font-mono">{settings.appVersion}</dd>
              </div>
              <div className="flex flex-wrap gap-2">
                <dt className="text-zinc-500">Data directory</dt>
                <dd className="font-mono break-all">{settings.dataDir}</dd>
              </div>
              <div className="flex flex-wrap gap-2">
                <dt className="text-zinc-500">Paired device</dt>
                <dd className="font-mono break-all">
                  {settings.deviceIdentifier ??
                    "Not paired — pair a device from the Accounts page to sync GitHub App accounts."}
                </dd>
              </div>
            </dl>
          </Card>

          <Card className="space-y-4 p-4">
            <h2 className="text-lg font-semibold">Diagnostics & backup</h2>
            <div className="space-y-2">
              <p className="text-sm text-zinc-500">
                The diagnostics file holds versions, counts, storage
                sizes and a redacted log tail — no credentials and no
                repository contents. Send it when reporting a problem.
              </p>
              <div className="flex flex-wrap items-center gap-3">
                <Button
                  variant="secondary"
                  disabled={diagBusy}
                  onClick={() => void handleExportDiagnostics()}
                >
                  Export diagnostics
                </Button>
                <Button
                  variant="secondary"
                  disabled={diagBusy}
                  onClick={() => void handleOpenFolder("diagnostics")}
                >
                  Open folder
                </Button>
              </div>
              {lastExport && (
                <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                  {lastExport.path}
                </p>
              )}
            </div>
            <div className="space-y-2">
              <p className="text-sm text-zinc-500">
                Backups are consistent snapshots of the database. A
                restore is checked, staged, and applied when RepoNyx
                next starts — the current database is kept as a
                fallback.
              </p>
              <div className="flex flex-wrap items-center gap-3">
                <Button
                  variant="secondary"
                  disabled={diagBusy}
                  onClick={() => void handleCreateBackup()}
                >
                  Back up database now
                </Button>
                <Button
                  variant="secondary"
                  disabled={diagBusy}
                  onClick={() => void handleOpenFolder("backups")}
                >
                  Open folder
                </Button>
              </div>
              {backups.length === 0 ? (
                <p className="text-sm text-zinc-500">
                  No backups yet.
                </p>
              ) : (
                <ul className="space-y-2">
                  {backups.map((backup) => (
                    <li
                      key={backup.path}
                      className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-zinc-200 px-3 py-2 dark:border-zinc-800"
                    >
                      <div className="min-w-0">
                        <p className="truncate font-mono text-sm">
                          {fileName(backup.path)}
                        </p>
                        <p className="text-sm text-zinc-500">
                          {formatBytes(backup.bytes)} ·{" "}
                          {formatDate(backup.createdAt)}
                        </p>
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <Button
                          variant="secondary"
                          disabled={diagBusy}
                          onClick={() =>
                            setPendingConfirm({
                              title: "Restore database backup",
                              message: `Restore the database from this backup?\n\n${fileName(backup.path)}\n\nThe backup is checked first, and the background agent is stopped while the restore is staged. Nothing changes until you quit RepoNyx completely and relaunch it. The current database is kept as a fallback.`,
                              confirmLabel: "Restore",
                              action: () => void handleRestoreBackup(backup),
                            })
                          }
                        >
                          Restore
                        </Button>
                        <Button
                          variant="danger"
                          disabled={diagBusy}
                          onClick={() =>
                            setPendingConfirm({
                              title: "Delete database backup",
                              message: `Delete this database backup?\n\n${fileName(backup.path)}\n\nThis cannot be undone.`,
                              confirmLabel: "Delete",
                              action: () => void handleDeleteBackup(backup),
                            })
                          }
                        >
                          Delete
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </Card>

          <Card className="space-y-4 p-4">
            <h2 className="text-lg font-semibold">Import & export</h2>
            <div className="space-y-2">
              <p className="text-sm text-zinc-500">
                A config file holds settings, each repository&apos;s
                sync choice, and schedules — so a setup can move to
                another machine. It never holds credentials, tokens,
                or archives. Import checks the whole file first; a
                bad file changes nothing. Entries for a repository or
                account this machine does not have yet are skipped —
                discover it or connect it, then import again.
              </p>
              <div className="flex flex-wrap items-center gap-3">
                <Button
                  variant="secondary"
                  disabled={diagBusy}
                  onClick={() => void handleExportConfig()}
                >
                  Export config
                </Button>
                <Button
                  variant="secondary"
                  disabled={diagBusy}
                  onClick={() => void handleImportConfig()}
                >
                  Import config
                </Button>
              </div>
              {lastConfigExport && (
                <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                  {lastConfigExport.path}
                </p>
              )}
            </div>
            {lastImport && (
              <div className="space-y-2">
                <p className="text-sm">
                  Imported: {lastImport.settingsApplied} settings,{" "}
                  {lastImport.repositoriesApplied} repositories,{" "}
                  {lastImport.schedulesCreated} schedules created,{" "}
                  {lastImport.schedulesUpdated} updated.
                </p>
                {lastImport.skipped.length > 0 && (
                  <ul className="space-y-1">
                    {lastImport.skipped.map((entry) => (
                      <li
                        key={`${entry.kind}:${entry.label}`}
                        className="text-sm text-zinc-500"
                      >
                        Skipped {entry.label} — {entry.reason}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
          </Card>
        </>
      )}
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
