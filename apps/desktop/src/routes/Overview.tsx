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

  useEffect(() => {
    let cancelled = false;
    Promise.all([getDashboardCounts(), getGitStatus()])
      .then(([c, g]) => {
        if (!cancelled) {
          setCounts(c);
          setGit(g);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
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
          Could not load dashboard data: {error}. If you are running the web
          preview outside Tauri, the backend commands are unavailable.
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
          Connect a GitHub account to start discovering repositories. Account
          connection (GitHub App / PAT) arrives in Phase 5; repository
          discovery in Phase 4.
        </p>
      </Card>
    </div>
  );
}
