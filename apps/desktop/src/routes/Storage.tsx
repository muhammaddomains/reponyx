import { useCallback, useEffect, useState } from "react";
import {
  deleteLocalArchive,
  deleteOrphan,
  getStorageOverview,
  listAccounts,
  moveArchiveForRename,
  openRepositoryLocation,
  openStorageRoot,
} from "@reponyx/api-client";
import type {
  OrphanArchive,
  RenamedArchive,
  StorageOverview,
  StoredRepository,
} from "@reponyx/shared";
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

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatDate(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

/** Storage: where the archives live, how much space they take, and
 *  how full the volume is — plus what is wrong with the storage
 *  (orphans, missing archives, renamed archives) and the actions
 *  that fix it. Sizes are computed on demand by the backend, so
 *  this page loads on request (Refresh) rather than polling — a
 *  large archive takes a moment to walk. Every destructive action
 *  asks first and is re-verified server-side before anything is
 *  removed. */
export default function Storage() {
  const [overview, setOverview] = useState<StorageOverview | null>(null);
  const [accountNames, setAccountNames] = useState<Record<string, string>>(
    {},
  );
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setOverview(await getStorageOverview());
  }, []);

  useEffect(() => {
    let cancelled = false;
    listAccounts()
      .then((accounts) => {
        if (cancelled) return;
        const names: Record<string, string> = {};
        for (const account of accounts) names[account.id] = account.username;
        setAccountNames(names);
      })
      .catch(() => {
        // Names are a convenience; ids still identify the repo.
      });
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

  async function handleRefresh() {
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleOpenRoot() {
    setError(null);
    try {
      await openStorageRoot();
    } catch (e: unknown) {
      setError(commandMessage(e));
    }
  }

  async function handleOpenRepo(repositoryId: string) {
    setError(null);
    try {
      await openRepositoryLocation(repositoryId);
    } catch (e: unknown) {
      setError(commandMessage(e));
    }
  }

  async function handleDeleteOrphan(orphan: OrphanArchive) {
    if (
      !window.confirm(
        `Delete this orphan archive?\n\n${orphan.path}\n\n${formatBytes(orphan.bytes)} will be freed. No repository points at it, so nothing can re-create it. This cannot be undone.`,
      )
    ) {
      return;
    }
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      const freed = await deleteOrphan(orphan.path);
      setNotice(`Deleted the orphan archive — freed ${formatBytes(freed)}.`);
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteArchive(repo: StoredRepository) {
    if (
      !window.confirm(
        `Delete the local archive for ${repo.fullName}?\n\nThe repository row stays — the next sync will re-clone it. ${formatBytes(repo.bytes)} will be freed. This cannot be undone.`,
      )
    ) {
      return;
    }
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      const freed = await deleteLocalArchive(repo.repositoryId);
      setNotice(
        `Deleted the local archive for ${repo.fullName} — freed ${formatBytes(freed)}. It will re-clone on the next sync.`,
      );
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleMoveArchive(renamed: RenamedArchive) {
    if (
      !window.confirm(
        `Move the archive to match the new name?\n\n${renamed.currentPath}\n→ ${renamed.expectedPath}`,
      )
    ) {
      return;
    }
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      const result = await moveArchiveForRename(renamed.repositoryId);
      setNotice(
        `Moved ${result.moved.length} archive${result.moved.length === 1 ? "" : "s"} to match ${renamed.fullName}.`,
      );
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const usedBytes =
    overview?.totalBytes != null && overview?.availableBytes != null
      ? overview.totalBytes - overview.availableBytes
      : null;
  const usedPct =
    overview?.totalBytes != null && overview.totalBytes > 0 && usedBytes != null
      ? Math.min(100, (usedBytes / overview.totalBytes) * 100)
      : null;
  const lowSpace =
    overview?.totalBytes != null &&
    overview?.availableBytes != null &&
    overview.totalBytes > 0 &&
    overview.availableBytes < overview.totalBytes * 0.1;

  return (
    <div className="space-y-6 p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Storage</h1>
          <p className="text-sm text-zinc-500">
            Where the archives live and how much space they take. Sizes
            are measured on demand — press Refresh to re-measure.
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

      {notice && (
        <Card className="border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-200">
          {notice}
        </Card>
      )}

      {loading && (
        <Card className="p-4">
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            Measuring storage…
          </p>
        </Card>
      )}

      {!loading && overview && (
        <>
          <Card className="space-y-3 p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="text-sm text-zinc-500">Storage root</p>
                <p className="truncate font-mono text-sm">{overview.root}</p>
                {!overview.rootConfigured && (
                  <p className="text-sm text-zinc-500">
                    Not set explicitly — using the default data directory.
                  </p>
                )}
              </div>
              <Button
                variant="secondary"
                onClick={() => void handleOpenRoot()}
              >
                Open folder
              </Button>
            </div>
            {overview.totalBytes != null && usedBytes != null ? (
              <div className="space-y-1">
                <div
                  className="h-2 w-full overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800"
                  role="progressbar"
                  aria-valuenow={Math.round(usedPct ?? 0)}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div
                    className="h-full rounded-full bg-zinc-600 dark:bg-zinc-400"
                    style={{ width: `${usedPct ?? 0}%` }}
                  />
                </div>
                <p className="text-sm text-zinc-500">
                  {formatBytes(usedBytes)} used of{" "}
                  {formatBytes(overview.totalBytes)} ·{" "}
                  {formatBytes(overview.availableBytes)} free
                </p>
                {lowSpace && (
                  <p className="text-sm font-medium text-amber-700 dark:text-amber-300">
                    Low space: less than 10% of this volume is free.
                    Syncs will start failing once it fills up — delete
                    an orphan or an archive you no longer need.
                  </p>
                )}
              </div>
            ) : (
              <p className="text-sm text-zinc-500">
                Volume size is unknown for this path.
              </p>
            )}
          </Card>

          <section className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <Card className="p-4">
              <p className="text-sm text-zinc-500">Mirrors</p>
              <p className="text-xl font-semibold">
                {formatBytes(overview.mirrorsBytes)}
              </p>
              <p className="text-sm text-zinc-500">
                {overview.storedRepositoryCount}{" "}
                {overview.storedRepositoryCount === 1
                  ? "repository stored"
                  : "repositories stored"}
              </p>
            </Card>
            <Card className="p-4">
              <p className="text-sm text-zinc-500">Database</p>
              <p className="text-xl font-semibold">
                {formatBytes(overview.databaseBytes)}
              </p>
              <p className="text-sm text-zinc-500">reponyx.sqlite</p>
            </Card>
            <Card className="p-4">
              <p className="text-sm text-zinc-500">Logs</p>
              <p className="text-xl font-semibold">
                {formatBytes(overview.logsBytes)}
              </p>
              <p className="text-sm text-zinc-500">log directory</p>
            </Card>
            <Card className="p-4">
              <p className="text-sm text-zinc-500">Stored total</p>
              <p className="text-xl font-semibold">
                {formatBytes(
                  overview.mirrorsBytes +
                    overview.databaseBytes +
                    overview.logsBytes,
                )}
              </p>
              <p className="text-sm text-zinc-500">
                mirrors + database + logs
              </p>
            </Card>
          </section>

          {overview.orphans.length > 0 && (
            <section className="space-y-4">
              <div className="flex flex-wrap items-baseline gap-2">
                <h2 className="text-lg font-semibold">Orphan archives</h2>
                <span className="text-sm text-zinc-500">
                  {formatBytes(overview.orphansBytes)} in directories no
                  repository points at — safe to delete.
                </span>
              </div>
              {overview.orphans.map((orphan) => (
                <Card key={orphan.path} className="space-y-2 p-4">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="warning">
                        {orphan.kind === "mirror" ? "Mirror" : "Working clone"}
                      </Badge>
                      <span className="font-medium">
                        {orphan.owner}/{orphan.name}
                      </span>
                    </div>
                    <span className="text-lg font-semibold">
                      {formatBytes(orphan.bytes)}
                    </span>
                  </div>
                  <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                    {orphan.path}
                  </p>
                  <div>
                    <Button
                      variant="danger"
                      disabled={busy}
                      onClick={() => void handleDeleteOrphan(orphan)}
                    >
                      Delete
                    </Button>
                  </div>
                </Card>
              ))}
            </section>
          )}

          {overview.renamed.length > 0 && (
            <section className="space-y-4">
              <h2 className="text-lg font-semibold">Renamed on GitHub</h2>
              <p className="text-sm text-zinc-500">
                These repositories were renamed, but their archive
                directories still carry the old name. Moving the
                directory keeps the archive — no re-clone needed.
              </p>
              {overview.renamed.map((renamed) => (
                <Card key={renamed.repositoryId} className="space-y-2 p-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge>
                      {renamed.kind === "mirror"
                        ? "Mirror"
                        : "Working clone"}
                    </Badge>
                    <span className="font-medium">{renamed.fullName}</span>
                  </div>
                  <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                    {renamed.currentPath}
                  </p>
                  <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                    → {renamed.expectedPath}
                  </p>
                  <div>
                    <Button
                      variant="secondary"
                      disabled={busy}
                      onClick={() => void handleMoveArchive(renamed)}
                    >
                      Move archive
                    </Button>
                  </div>
                </Card>
              ))}
            </section>
          )}

          {overview.missing.length > 0 && (
            <section className="space-y-4">
              <h2 className="text-lg font-semibold">Missing archives</h2>
              <p className="text-sm text-zinc-500">
                These repositories have a recorded local path, but the
                directory is gone from disk. Nothing to delete — the
                next sync will re-clone them.
              </p>
              {overview.missing.map((missing) => (
                <Card key={missing.repositoryId} className="space-y-2 p-4">
                  <p className="font-medium">{missing.fullName}</p>
                  <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                    {missing.localPath}
                  </p>
                </Card>
              ))}
            </section>
          )}

          <section className="space-y-4">
            <h2 className="text-lg font-semibold">Repositories on disk</h2>
            {overview.repositories.length === 0 && (
              <Card className="p-4">
                <p className="text-sm text-zinc-600 dark:text-zinc-400">
                  Nothing stored yet. Repositories appear here after
                  their first sync — run one from the Repositories
                  page.
                </p>
              </Card>
            )}
            {overview.repositories.map((repo) => (
              <Card key={repo.repositoryId} className="space-y-2 p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <span className="font-medium">{repo.fullName}</span>
                    <span className="ml-2 text-sm text-zinc-500">
                      {accountNames[repo.accountId] ?? repo.accountId}
                    </span>
                  </div>
                  <span className="text-lg font-semibold">
                    {formatBytes(repo.bytes)}
                  </span>
                </div>
                <p className="truncate font-mono text-sm text-zinc-600 dark:text-zinc-400">
                  {repo.localPath}
                </p>
                <div className="flex flex-wrap items-center gap-3">
                  <span className="text-sm text-zinc-500">
                    Last synced {formatDate(repo.lastSyncedAt)}
                  </span>
                  <Button
                    variant="secondary"
                    onClick={() => void handleOpenRepo(repo.repositoryId)}
                  >
                    Open folder
                  </Button>
                  <Button
                    variant="danger"
                    disabled={busy}
                    onClick={() => void handleDeleteArchive(repo)}
                  >
                    Delete archive
                  </Button>
                </div>
              </Card>
            ))}
          </section>
        </>
      )}
    </div>
  );
}
