import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card } from "@reponyx/ui";
import {
  agentHealth,
  ensureAgentRunning,
  stopAgent,
  agentPause,
  agentResume,
  registerAgentAutostart,
  unregisterAgentAutostart,
  autostartStatus,
  isTauriAvailable,
} from "@reponyx/api-client";
import type { AgentHealth } from "@reponyx/shared";

/** Same shape as the other pages: Tauri rejects with the backend's
 *  serialized error value ({ code, message }), not an Error — without
 *  this, `String(error)` renders "[object Object]". */
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

export default function Agent() {
  const [health, setHealth] = useState<AgentHealth | null>(null);
  // Whether the first health poll has settled (success or failure).
  // Before that the badge must say "Checking…" — showing "Not
  // running" before the first poll (audit F9) is a false alarm, and
  // a failed poll must not erase the last known health either.
  const [checked, setChecked] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const tauri = isTauriAvailable();

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const h = await agentHealth();
      setHealth(h);
    } catch (e) {
      // Keep the last known health: the poll failing does not mean
      // the agent stopped — only that this probe could not reach it.
      setError(commandMessage(e));
    } finally {
      setChecked(true);
    }
  }, []);

  const refreshAutostart = useCallback(async () => {
    try {
      setAutostart(await autostartStatus());
    } catch {
      setAutostart(null);
    }
  }, []);

  useEffect(() => {
    if (!tauri) return;
    void refresh();
    // Start-at-login changes only when the user clicks
    // Enable/Disable, so read it once — not on the 5 s health
    // poll, which would spawn a schtasks/reg check every tick.
    void refreshAutostart();
    const id = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(id);
  }, [tauri, refresh, refreshAutostart]);

  async function run<T>(action: () => Promise<T>, done?: (value: T) => void) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const value = await action();
      await refresh();
      done?.(value);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (!tauri) {
    return (
      <div className="space-y-6 p-6">
        <header>
          <h1 className="text-2xl font-semibold">Background agent</h1>
          <p className="text-sm text-zinc-500">
            Agent health is only available inside the RepoNyx desktop app.
          </p>
        </header>
      </div>
    );
  }

  const reachable = health?.reachable === true;
  const status = health?.status;
  const paused = status?.paused === true;

  return (
    <div className="space-y-6 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Background agent</h1>
          <p className="text-sm text-zinc-500">
            Scheduled sync runs in a separate process so it continues when
            this window is closed.
          </p>
        </div>
        <Badge
          variant={!checked ? "default" : reachable ? "success" : "danger"}
        >
          {!checked ? "Checking…" : reachable ? "Running" : "Not running"}
        </Badge>
      </header>

      {error ? (
        <Card className="border-red-300 p-4 text-sm text-red-700 dark:border-red-900 dark:text-red-300">
          {error}
        </Card>
      ) : null}

      {notice ? (
        <Card className="border-emerald-300 p-4 text-sm text-emerald-800 dark:border-emerald-900 dark:text-emerald-300">
          {notice}
        </Card>
      ) : null}

      <Card className="space-y-3 p-4">
        <h2 className="text-sm font-medium text-zinc-500">Status</h2>
        <p className="text-sm text-zinc-600 dark:text-zinc-400">
          Start at login:{" "}
          {autostart === null
            ? "unknown"
            : autostart
              ? "enabled"
              : "disabled"}
        </p>
        {status ? (
          <dl className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-3">
            <div>
              <dt className="text-zinc-500">Generation</dt>
              <dd className="font-mono">{status.generation}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Started</dt>
              <dd className="font-mono text-xs">{status.startedAt}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Paused</dt>
              <dd>{status.paused ? "yes" : "no"}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Pending jobs</dt>
              <dd>{status.pendingJobs}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Running jobs</dt>
              <dd>{status.runningJobs}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Workers</dt>
              <dd>{status.workers}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Last schedule tick</dt>
              <dd className="font-mono text-xs">
                {status.lastMaterializeAt ?? "—"}
              </dd>
            </div>
            <div>
              <dt className="text-zinc-500">Jobs created last tick</dt>
              <dd>{status.lastMaterializeCreated ?? "—"}</dd>
            </div>
            <div>
              <dt className="text-zinc-500">Protocol</dt>
              <dd>v{status.protocolVersion}</dd>
            </div>
          </dl>
        ) : (
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            {health?.message ?? "No status yet."}
          </p>
        )}
      </Card>

      <Card className="flex flex-wrap gap-2 p-4">
        <Button disabled={busy} onClick={() => run(refresh)}>
          Refresh
        </Button>
        <Button
          disabled={busy || reachable}
          variant="secondary"
          onClick={() =>
            run(ensureAgentRunning, (h) =>
              setNotice(
                h.reachable ? "Background agent started." : h.message,
              ),
            )
          }
        >
          Start agent
        </Button>
        <Button
          disabled={busy || !reachable}
          variant="secondary"
          onClick={() => run(stopAgent, (message) => setNotice(message))}
        >
          Stop agent
        </Button>
        <Button
          disabled={busy || !reachable || paused}
          variant="secondary"
          onClick={() => run(agentPause)}
        >
          Pause
        </Button>
        <Button
          disabled={busy || !reachable || !paused}
          variant="secondary"
          onClick={() => run(agentResume)}
        >
          Resume
        </Button>
        <Button
          disabled={busy || autostart === true}
          variant="secondary"
          onClick={() =>
            run(registerAgentAutostart, async (message) => {
              setNotice(message);
              await refreshAutostart();
            })
          }
        >
          Enable start at login
        </Button>
        <Button
          disabled={busy || autostart === false}
          variant="secondary"
          onClick={() =>
            run(unregisterAgentAutostart, async (message) => {
              setNotice(message);
              await refreshAutostart();
            })
          }
        >
          Disable start at login
        </Button>
      </Card>

      <Card className="p-4 text-sm text-zinc-600 dark:text-zinc-400">
        <p>
          Autostart uses Windows Task Scheduler (ONLOGON), falling back
          to the Registry Run key when Task Scheduler refuses the
          task; a systemd user unit on Linux; or a LaunchAgent on
          macOS. The agent is a user-session process and does not
          continue after logout (see ADR-006).
        </p>
      </Card>
    </div>
  );
}
