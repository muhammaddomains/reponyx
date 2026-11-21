import { useCallback, useEffect, useState } from "react";
import {
  listHistoryForRepository,
  listRecentHistory,
  listRepositories,
} from "@reponyx/api-client";
import type { SyncHistory, SyncResult } from "@reponyx/shared";
import { Badge, Button, Card } from "@reponyx/ui";

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

function resultBadge(result: SyncResult) {
  switch (result) {
    case "success":
      return <Badge variant="success">success</Badge>;
    case "failed":
      return <Badge variant="danger">failed</Badge>;
    case "cancelled":
      return <Badge>cancelled</Badge>;
  }
}

function operationLabel(operation: string): string {
  switch (operation) {
    case "mirror_clone":
      return "Mirror clone";
    case "working_clone":
      return "Working clone";
    case "fetch":
      return "Fetch";
    case "lfs_fetch":
      return "LFS fetch";
    case "submodule_sync":
      return "Submodule sync";
    default:
      return operation;
  }
}

function formatDuration(ms: number | null | undefined): string {
  if (ms == null) return "—";
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

type ResultFilter = "all" | SyncResult;

/** Activity: the sync-history record — what ran, when, how long it
 *  took, and whether it worked. Newest first, refreshed every 10
 *  seconds. History is append-only; this page never edits it. */
export default function Activity() {
  const [entries, setEntries] = useState<SyncHistory[]>([]);
  const [repoNames, setRepoNames] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [repositoryFilter, setRepositoryFilter] = useState("all");
  const [resultFilter, setResultFilter] = useState<ResultFilter>("all");

  const refresh = useCallback(async () => {
    const entries =
      repositoryFilter === "all"
        ? await listRecentHistory(100)
        : await listHistoryForRepository(repositoryFilter, 100);
    setEntries(entries);
  }, [repositoryFilter]);

  useEffect(() => {
    let cancelled = false;
    listRepositories()
      .then((repos) => {
        if (cancelled) return;
        const names: Record<string, string> = {};
        for (const repo of repos) names[repo.id] = repo.fullName;
        setRepoNames(names);
      })
      .catch(() => {
        // Names are a convenience; ids still identify the entry.
      });
    refresh()
      .catch((e: unknown) => {
        if (!cancelled) setError(commandMessage(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    const id = window.setInterval(() => {
      refresh().catch((e: unknown) => {
        if (!cancelled) setError(commandMessage(e));
      });
    }, 10_000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [refresh]);

  async function handleRefresh() {
    setError(null);
    setBusy(true);
    try {
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const visible =
    resultFilter === "all"
      ? entries
      : entries.filter((entry) => entry.result === resultFilter);

  return (
    <div className="space-y-6 p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Activity</h1>
          <p className="text-sm text-zinc-500">
            The sync-history record — newest first. History is
            append-only; this page shows it, never edits it.
          </p>
        </div>
        <Button
          variant="secondary"
          disabled={busy}
          onClick={() => void handleRefresh()}
        >
          Refresh
        </Button>
      </header>

      {error && (
        <Card className="border-red-200 bg-red-50 p-4 text-sm whitespace-pre-line text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200">
          {error}
        </Card>
      )}

      <section className="space-y-4">
        <div className="flex flex-wrap items-center gap-3">
          <select
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            value={repositoryFilter}
            onChange={(e) => setRepositoryFilter(e.target.value)}
          >
            <option value="all">All repositories</option>
            {Object.entries(repoNames).map(([id, name]) => (
              <option key={id} value={id}>
                {name}
              </option>
            ))}
          </select>
          <select
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            value={resultFilter}
            onChange={(e) => setResultFilter(e.target.value as ResultFilter)}
          >
            <option value="all">Any result</option>
            <option value="success">Success</option>
            <option value="failed">Failed</option>
            <option value="cancelled">Cancelled</option>
          </select>
          {!loading && (
            <span className="text-sm text-zinc-500">
              {visible.length} {visible.length === 1 ? "entry" : "entries"}
              {entries.length > 0 &&
                ` (latest ${entries.length} shown)`}
            </span>
          )}
        </div>

        {loading && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Loading activity…
            </p>
          </Card>
        )}
        {!loading && visible.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              No history yet. Entries appear here after the first sync
              finishes — run one from the Repositories page or wait for
              a schedule to fire.
            </p>
          </Card>
        )}
        {visible.map((entry) => (
          <Card key={entry.id} className="space-y-2 p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <span className="font-medium">
                  {repoNames[entry.repositoryId] ?? entry.repositoryId}
                </span>
                <span className="ml-2 text-sm text-zinc-500">
                  {operationLabel(entry.operation)}
                </span>
              </div>
              {resultBadge(entry.result)}
            </div>
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Started {formatDate(entry.startedAt)}
              {entry.completedAt
                ? ` · finished ${formatDate(entry.completedAt)}`
                : ""}
              {" · "}
              {formatDuration(entry.durationMs)}
              {entry.bytesReceived != null
                ? ` · ${formatBytes(entry.bytesReceived)} received`
                : ""}
            </p>
          </Card>
        ))}
      </section>
    </div>
  );
}
