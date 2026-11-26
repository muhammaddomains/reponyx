import { useCallback, useEffect, useState } from "react";
import { getDashboardCounts, getGitStatus } from "@reponyx/api-client";
import type { DashboardCounts, GitStatus } from "@reponyx/shared";
import { Badge, Card } from "@reponyx/ui";

function Stat({ label, value }: { label: string; value: number | null }) {
  return (
    <Card className="p-4">
      <div className="text-2xl font-semibold tabular-nums">
        {value === null ? "…" : value}
      </div>
      <div className="text-sm text-zinc-500">{label}</div>
    </Card>
  );
}

/** Overview: real counts from the local SQLite database and real Git status.
 *  A fresh install shows zeros — never fabricated data. */
export default function Overview() {
  const [counts, setCounts] = useState<DashboardCounts | null>(null);
  const [git, setGit] = useState<GitStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Counts and Git status load independently: in the web preview
  // (plain browser, no Tauri backend) both fail, but a Git failure
  // must not blank the counts, and a failed Git check must resolve
  // to "unavailable" instead of spinning on "checking…" forever.
  // A failed counts load can be retried from the error banner; a
  // successful load clears the error (audit F12/F13). The loader
  // is a useCallback with no dependencies (it only uses stable
  // setters and API calls), so the mount effect can list it as a
  // dependency without re-running.
  const loadCounts = useCallback((guard?: () => boolean) => {
    getDashboardCounts()
      .then((c) => {
        if (guard?.()) return;
        setCounts(c);
        setError(null);
      })
      .catch((e: unknown) => {
        if (guard?.()) return;
        setError(e instanceof Error ? e.message : String(e));
      });
  }, []);

  useEffect(() => {
    let cancelled = false;
    loadCounts(() => cancelled);
    getGitStatus()
      .then((g) => {
        if (!cancelled) setGit(g);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setGit({
            available: false,
            supported: false,
            error: e instanceof Error ? e.message : String(e),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [loadCounts]);

  return (
    <div className="space-y-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Overview</h1>
        <p className="text-sm text-zinc-500">
          Local repository archive status.
        </p>
      </header>

      {error && (
        <Card className="border-red-200 bg-red-50 p-4 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span>Could not load dashboard data: {error}</span>
            <button
              type="button"
              onClick={() => loadCounts()}
              className="font-medium underline"
            >
              Retry
            </button>
          </div>
        </Card>
      )}

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-3">
        <Stat label="Connected accounts" value={counts?.accounts ?? null} />
        <Stat
          label="Discovered repositories"
          value={counts?.repositories ?? null}
        />
        <Stat label="Synchronized" value={counts?.synced ?? null} />
        <Stat label="Currently syncing" value={counts?.syncing ?? null} />
        <Stat label="Failed" value={counts?.failed ?? null} />
        <Stat label="Pending jobs" value={counts?.pending_jobs ?? null} />
      </div>

      <Card className="p-4">
        <div className="flex items-center justify-between">
          <h2 className="font-medium">Git</h2>
          <Badge variant={git?.available && git.supported ? "success" : "warning"}>
            {git == null
              ? "checking…"
              : git.available && git.supported
                ? "ready"
                : "unavailable"}
          </Badge>
        </div>
        <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
          {git == null
            ? "Detecting the installed Git…"
            : git.available
              ? `${git.version ?? "Git"} (${git.path ?? "unknown path"})`
              : (git.error ?? "Git is not installed or not on PATH. RepoNyx requires Git ≥ 2.40.")}
        </p>
      </Card>

      <Card className="p-4">
        <h2 className="font-medium">Next steps</h2>
        <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
          Connect a GitHub account on the Accounts page to start
          discovering repositories. The Repositories page lists every
          discovered repository with its sync mode and status; pick a
          repository there and use Sync now to archive it.
        </p>
      </Card>
    </div>
  );
}
