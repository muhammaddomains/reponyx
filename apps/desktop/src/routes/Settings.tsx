import { useCallback, useEffect, useState } from "react";
import { getSettings, setSetting } from "@reponyx/api-client";
import type { Settings } from "@reponyx/shared";
import { Button, Card } from "@reponyx/ui";
import { applyTheme, parseTheme } from "../lib/theme";

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
 *  theme (`ui.theme`, applied on startup). Sync: the default mode
 *  for newly discovered repositories (`sync.default_mode`) and how
 *  many syncs run at once (`sync.concurrency`, takes effect when
 *  the agent next starts). Storage: the archive root
 *  (`storage.root`). Advanced is read-only information. */
export default function Settings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState<string | null>(null);

  const [theme, setTheme] = useState("system");
  const [mode, setMode] = useState("mirror");
  const [concurrency, setConcurrency] = useState("5");
  const [storageRoot, setStorageRoot] = useState("");

  const refresh = useCallback(async () => {
    const loaded = await getSettings();
    setSettings(loaded);
    setTheme(parseTheme(loaded.theme));
    setMode(loaded.defaultSyncMode ?? "mirror");
    setConcurrency(String(loaded.concurrency ?? 5));
    setStorageRoot(loaded.storageRoot ?? "");
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

  async function save(key: string, value: string, label: string) {
    setError(null);
    setNotice(null);
    setSaving(key);
    try {
      await setSetting(key, value);
      setNotice(`${label} saved.`);
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

  function handleConcurrencySave() {
    const parsed = Number(concurrency);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 10) {
      setError("Concurrent syncs must be a whole number between 1 and 10.");
      return;
    }
    void save("sync.concurrency", String(parsed), "Concurrent syncs");
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
                Changing it does not move existing archives; they stay
                where they are, and new syncs go to the new root after
                the agent restarts. See the Storage page for what is
                stored where.
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
        </>
      )}
    </div>
  );
}
