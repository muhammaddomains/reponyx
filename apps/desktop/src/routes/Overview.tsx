import { useEffect, useState } from "react";
import { getDashboardCounts, getGitStatus } from "@reponyx/api-client";
import type { DashboardCounts, GitStatus } from "@reponyx/shared";
import { Badge, Card } from "@reponyx/ui";

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <Card className="p-4">
      <div className="text-2xl font-semibold tabular-nums">{value}</div>
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
  useEffect(() => {
    let cancelled = false;
    getDashboardCounts()
      .then((c) => {
        if (!cancelled) setCounts(c);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
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
  }, []);

  return (
    <div className="space-y-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Overview</h1>
        <p className="text-sm text-zinc-500">
          Local repository archive status — Phase 1 foundation.
        </p>
      </header>

      {error && (
        <Card className="border-red-200 bg-red-50 p-4 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200">
          Could not load dashboard data: {error}
        </Card>
      )}

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-3">
        <Stat label="Connected accounts" value={counts?.accounts ?? 0} />
        <Stat label="Discovered repositories" value={counts?.repositories ?? 0} />
        <Stat label="Synchronized" value={counts?.synced ?? 0} />
        <Stat label="Currently syncing" value={counts?.syncing ?? 0} />
        <Stat label="Failed" value={counts?.failed ?? 0} />
        <Stat label="Pending jobs" value={counts?.pending_jobs ?? 0} />
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
          discovering repositories. Account connection (GitHub App / PAT)
          and repository discovery are implemented; the Repositories page
          itself arrives in Phase 8.
        </p>
      </Card>
    </div>
  );
}
