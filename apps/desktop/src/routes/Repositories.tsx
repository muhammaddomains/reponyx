import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  agentEnqueue,
  deleteRepository,
  discoverRepositories,
  ensureAgentRunning,
  listAccounts,
  listRepositories,
  openRepositoryInBrowser,
  openRepositoryLocation,
  updateRepository,
} from "@reponyx/api-client";
import type {
  Account,
  ArchiveStatus,
  DiscoveryOutcome,
  Repository,
  SyncMode,
} from "@reponyx/shared";
import { Badge, Button, Card } from "@reponyx/ui";

const inputClass =
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900";

const PAGE_SIZE = 100;

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

function statusBadge(status: ArchiveStatus) {
  switch (status) {
    case "synced":
      return <Badge variant="success">synced</Badge>;
    case "pending":
      return <Badge variant="warning">pending</Badge>;
    case "syncing":
      return <Badge>syncing</Badge>;
    case "failed":
      return <Badge variant="danger">failed</Badge>;
    case "paused":
      return <Badge>paused</Badge>;
    case "disconnected":
      return <Badge variant="danger">disconnected</Badge>;
    case "remote_deleted":
      return <Badge variant="danger">remote deleted</Badge>;
  }
}

function formatDate(value: string | null | undefined): string {
  if (!value) return "never";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

type SyncFilter = "all" | "enabled" | "disabled";

/** Repositories: list, search/filter, discover from GitHub, sync
 *  now (via the background agent), pause/resume, mode change, open
 *  folder / GitHub, remove (row only — local files are kept). */
export default function Repositories() {
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);

  const [accountFilter, setAccountFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState<"all" | ArchiveStatus>(
    "all",
  );
  const [syncFilter, setSyncFilter] = useState<SyncFilter>("all");
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  // The total the filter matches, and the rows loaded so far.
  // Pages of 100 are fetched from the server and appended; the
  // whole table is never loaded, however many repositories exist.
  const [total, setTotal] = useState(0);
  const [loadingMore, setLoadingMore] = useState(false);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());

  const [discoverAccountId, setDiscoverAccountId] = useState("");
  const [discovering, setDiscovering] = useState(false);
  const [outcome, setOutcome] = useState<DiscoveryOutcome | null>(null);

  const [syncingId, setSyncingId] = useState<string | null>(null);

  // True virtualization (Phase 8): only the rows on screen are in
  // the DOM, however many repositories are loaded. The list still
  // pages 100 rows at a time from the server ("Show more" below);
  // the virtualizer renders the loaded rows. Row heights vary
  // (long paths wrap), so each row is measured after mount and the
  // estimate only seeds the scrollbar.
  const listRef = useRef<HTMLDivElement | null>(null);
  const rowVirtualizer = useVirtualizer({
    count: repositories.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => 190,
    overscan: 5,
    // Seeds the first paint (and jsdom, where the container has no
    // layout yet); the ResizeObserver corrects it on mount.
    initialRect: { width: 800, height: 640 },
  });

  // Search applies 400 ms after the last keystroke, so every key
  // does not refetch the list.
  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput), 400);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  const refresh = useCallback(async () => {
    const [reposResult, accountsResult] = await Promise.allSettled([
      listRepositories({
        accountId: accountFilter === "all" ? null : accountFilter,
        archiveStatus: statusFilter === "all" ? null : statusFilter,
        syncEnabled:
          syncFilter === "all" ? null : syncFilter === "enabled",
        search: search.trim() ? search.trim() : null,
        limit: PAGE_SIZE,
        offset: 0,
      }),
      listAccounts(),
    ]);
    if (accountsResult.status === "fulfilled") {
      setAccounts(accountsResult.value);
    }
    if (reposResult.status === "rejected") {
      throw reposResult.reason;
    }
    setRepositories(reposResult.value.items);
    setTotal(reposResult.value.total);
  }, [accountFilter, statusFilter, syncFilter, search]);

  async function handleShowMore() {
    if (loadingMore || repositories.length >= total) return;
    setError(null);
    setLoadingMore(true);
    try {
      const page = await listRepositories({
        accountId: accountFilter === "all" ? null : accountFilter,
        archiveStatus: statusFilter === "all" ? null : statusFilter,
        syncEnabled:
          syncFilter === "all" ? null : syncFilter === "enabled",
        search: search.trim() ? search.trim() : null,
        limit: PAGE_SIZE,
        offset: repositories.length,
      });
      setRepositories((current) => [...current, ...page.items]);
      setTotal(page.total);
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setLoadingMore(false);
    }
  }

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

  async function ensureAgent() {
    const health = await ensureAgentRunning();
    if (!health.reachable) {
      throw new Error(
        health.message || "Background agent is not running.",
      );
    }
  }

  async function handleSyncNow(repository: Repository) {
    setError(null);
    setNotice(null);
    setSyncingId(repository.id);
    try {
      await ensureAgent();
      const jobId = await agentEnqueue(repository.id);
      setNotice(
        jobId
          ? `Sync queued for ${repository.fullName} (job ${jobId.slice(0, 8)}…).`
          : `Sync queued for ${repository.fullName}.`,
      );
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setSyncingId(null);
    }
  }

  async function handleBulkSync() {
    const ids = [...selected];
    if (ids.length === 0) return;
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      await ensureAgent();
      let queued = 0;
      const failed: string[] = [];
      for (const id of ids) {
        try {
          await agentEnqueue(id);
          queued += 1;
        } catch (e: unknown) {
          failed.push(`${id.slice(0, 8)}: ${commandMessage(e)}`);
        }
      }
      setSelected(new Set());
      setNotice(
        `Queued ${queued} of ${ids.length} selected repositories.` +
          (failed.length > 0 ? ` ${failed.length} failed.` : ""),
      );
      if (failed.length > 0) setError(failed.join("\n"));
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleBulkSyncEnabled(enabled: boolean) {
    const ids = [...selected];
    if (ids.length === 0) return;
    await run(
      async () => {
        for (const id of ids) {
          await updateRepository({ id, syncEnabled: enabled });
        }
      },
      () => {
        setSelected(new Set());
        setNotice(
          `${ids.length} repositories ${enabled ? "resumed" : "paused"}.`,
        );
      },
    );
  }

  async function handleBulkRemove() {
    const ids = [...selected];
    if (ids.length === 0) return;
    // One confirm and one refresh for the whole selection: removing
    // repos one row at a time meant a confirm + a busy refresh per
    // repo, which felt slow even though each delete is a single
    // SQL statement.
    if (
      !window.confirm(
        `Remove ${ids.length} selected repositories from RepoNyx? Their local files on disk are kept — only the RepoNyx rows are deleted.`,
      )
    ) {
      return;
    }
    await run(
      async () => {
        for (const id of ids) {
          await deleteRepository(id);
        }
      },
      () => {
        setSelected(new Set());
        setNotice(
          `Removed ${ids.length} repositories. Local files were kept.`,
        );
      },
    );
  }

  async function handleDiscover() {
    if (!discoverAccountId) {
      setError("Pick an account to discover its repositories.");
      return;
    }
    setError(null);
    setNotice(null);
    setDiscovering(true);
    try {
      const result = await discoverRepositories(discoverAccountId);
      setOutcome(result);
      const account = accounts.find((a) => a.id === discoverAccountId);
      setNotice(
        `Discovered ${account?.username ?? "account"}: ${result.created} new, ${result.updated} updated` +
          (result.notSeen.length > 0
            ? `, ${result.notSeen.length} local rows not seen on GitHub (kept, not deleted).`
            : "."),
      );
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setDiscovering(false);
    }
  }

  function toggleSelected(id: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  const visible = repositories;
  const allVisibleSelected =
    visible.length > 0 && visible.every((r) => selected.has(r.id));

  return (
    <div className="space-y-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Repositories</h1>
        <p className="text-sm text-zinc-500">
          Repositories discovered from your GitHub accounts. Discovery
          never deletes local rows; removing one here keeps its local
          files on disk.
        </p>
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

      <section className="space-y-4">
        <h2 className="text-lg font-medium">Discover from GitHub</h2>
        <Card className="space-y-3 p-4">
          <div className="flex gap-3">
            <select
              className={inputClass}
              value={discoverAccountId}
              onChange={(e) => setDiscoverAccountId(e.target.value)}
            >
              <option value="">Pick an account…</option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.username}
                  {account.status !== "connected"
                    ? ` (${account.status})`
                    : ""}
                </option>
              ))}
            </select>
            <Button
              disabled={discovering || !discoverAccountId}
              onClick={() => void handleDiscover()}
            >
              {discovering ? "Discovering…" : "Discover repositories"}
            </Button>
          </div>
          <p className="text-xs text-zinc-500">
            Lists the account&apos;s repositories on GitHub and saves
            them here. New ones start sync-enabled in mirror mode;
            existing ones keep their settings. This can take a while
            for large accounts — it pages through every repository.
          </p>
          {outcome && (
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Last discovery: {outcome.created} new · {outcome.updated}{" "}
              updated · {outcome.notSeen.length} not seen on GitHub
              (kept).
            </p>
          )}
        </Card>
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-medium">
          All repositories
          {!loading && ` (${total})`}
        </h2>
        <div className="flex flex-wrap gap-3">
          <select
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            value={accountFilter}
            onChange={(e) => {
              setAccountFilter(e.target.value);
            }}
          >
            <option value="all">All accounts</option>
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>
                {account.username}
              </option>
            ))}
          </select>
          <select
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            value={statusFilter}
            onChange={(e) => {
              setStatusFilter(e.target.value as "all" | ArchiveStatus);
            }}
          >
            <option value="all">Any status</option>
            <option value="synced">Synced</option>
            <option value="pending">Pending</option>
            <option value="syncing">Syncing</option>
            <option value="failed">Failed</option>
            <option value="paused">Paused</option>
            <option value="disconnected">Disconnected</option>
            <option value="remote_deleted">Remote deleted</option>
          </select>
          <select
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            value={syncFilter}
            onChange={(e) => {
              setSyncFilter(e.target.value as SyncFilter);
            }}
          >
            <option value="all">Sync on or off</option>
            <option value="enabled">Sync enabled</option>
            <option value="disabled">Sync disabled</option>
          </select>
          <input
            className="min-w-56 flex-1 rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            type="search"
            placeholder="Search by name, e.g. reponyx"
            value={searchInput}
            onChange={(e) => {
              setSearchInput(e.target.value);
            }}
          />
        </div>

        {selected.size > 0 && (
          <Card className="flex flex-wrap items-center gap-2 p-3">
            <span className="text-sm font-medium">
              {selected.size} selected
            </span>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => void handleBulkSync()}
            >
              Sync selected now
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => void handleBulkSyncEnabled(false)}
            >
              Pause selected
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => void handleBulkSyncEnabled(true)}
            >
              Resume selected
            </Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={() => void handleBulkRemove()}
            >
              Remove selected
            </Button>
            <Button variant="ghost" onClick={() => setSelected(new Set())}>
              Clear
            </Button>
          </Card>
        )}

        {loading && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Loading repositories…
            </p>
          </Card>
        )}
        {!loading && repositories.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              No repositories match. Discover one above, or clear the
              filters.
            </p>
          </Card>
        )}
        {visible.length > 0 && (
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={allVisibleSelected}
              onChange={() => {
                setSelected((current) => {
                  const next = new Set(current);
                  if (allVisibleSelected) {
                    for (const r of visible) next.delete(r.id);
                  } else {
                    for (const r of visible) next.add(r.id);
                  }
                  return next;
                });
              }}
            />
            Select all shown ({visible.length})
          </label>
        )}
        {visible.length > 0 && (
          <div
            ref={listRef}
            className="h-[640px] overflow-auto rounded-lg"
          >
            <div
              style={{
                height: `${rowVirtualizer.getTotalSize()}px`,
                width: "100%",
                position: "relative",
              }}
            >
              {rowVirtualizer.getVirtualItems().map((virtualRow) => {
                const repository = repositories[virtualRow.index];
                return (
                  <div
                    key={repository.id}
                    data-index={virtualRow.index}
                    ref={rowVirtualizer.measureElement}
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      transform: `translateY(${virtualRow.start}px)`,
                    }}
                    className="pb-4"
                  >
                    <Card className="space-y-3 p-4">
                      <div className="flex flex-wrap items-start justify-between gap-2">
                        <div className="flex items-start gap-3">
                          <input
                            type="checkbox"
                            className="mt-1"
                            checked={selected.has(repository.id)}
                            onChange={() => toggleSelected(repository.id)}
                            aria-label={`Select ${repository.fullName}`}
                          />
                          <div>
                            <span className="font-medium">{repository.fullName}</span>
                            <span className="ml-2 text-sm text-zinc-500">
                              {accounts.find((a) => a.id === repository.accountId)
                                ?.username ?? ""}
                            </span>
                            <p className="text-sm text-zinc-600 dark:text-zinc-400">
                              Last synced: {formatDate(repository.lastSyncedAt)}
                              {repository.localPath
                                ? ` · ${repository.localPath}`
                                : " · not synced yet"}
                            </p>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          {statusBadge(repository.archiveStatus)}
                          <Badge>{repository.visibility}</Badge>
                          <Badge>{repository.syncMode}</Badge>
                          {!repository.syncEnabled && <Badge>sync off</Badge>}
                        </div>
                      </div>
                      <div className="flex flex-wrap items-center gap-2">
                        <Button
                          variant="secondary"
                          disabled={busy || syncingId === repository.id}
                          onClick={() => void handleSyncNow(repository)}
                        >
                          {syncingId === repository.id ? "Queuing…" : "Sync now"}
                        </Button>
                        <Button
                          variant="secondary"
                          disabled={busy}
                          onClick={() =>
                            run(
                              () =>
                                updateRepository({
                                  id: repository.id,
                                  syncEnabled: !repository.syncEnabled,
                                }),
                              () =>
                                setNotice(
                                  repository.syncEnabled
                                    ? `Paused ${repository.fullName}.`
                                    : `Resumed ${repository.fullName}.`,
                                ),
                            )
                          }
                        >
                          {repository.syncEnabled ? "Pause" : "Resume"}
                        </Button>
                        <select
                          className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                          value={repository.syncMode}
                          disabled={busy}
                          aria-label={`Sync mode for ${repository.fullName}`}
                          onChange={(e) => {
                            // Capture the picked mode now: reading
                            // `e.target.value` in the `done` callback below
                            // would see the previous mode, because React
                            // re-renders this controlled select with the
                            // still-stale `repository.syncMode` while the
                            // update is in flight.
                            const mode = e.target.value as SyncMode;
                            run(
                              () =>
                                updateRepository({
                                  id: repository.id,
                                  syncMode: mode,
                                }),
                              () =>
                                setNotice(
                                  `${repository.fullName} mode set to ${mode}.`,
                                ),
                            );
                          }}
                        >
                          <option value="mirror">Mirror</option>
                          <option value="working">Working clone</option>
                          <option value="both">Both</option>
                        </select>
                        <Button
                          variant="secondary"
                          disabled={busy || !repository.localPath}
                          onClick={() =>
                            run(() => openRepositoryLocation(repository.id))
                          }
                        >
                          Open folder
                        </Button>
                        <Button
                          variant="secondary"
                          disabled={busy}
                          onClick={() =>
                            run(() => openRepositoryInBrowser(repository.id))
                          }
                        >
                          Open on GitHub
                        </Button>
                        <Button
                          variant="danger"
                          disabled={busy}
                          onClick={() => {
                            if (
                              window.confirm(
                                `Remove ${repository.fullName} from RepoNyx? Its local files on disk are kept — only the RepoNyx row is deleted.`,
                              )
                            ) {
                              void run(
                                () => deleteRepository(repository.id),
                                () =>
                                  setNotice(
                                    `Removed ${repository.fullName}. Local files were kept.`,
                                  ),
                              );
                            }
                          }}
                        >
                          Remove
                        </Button>
                      </div>
                    </Card>
                  </div>
                );
              })}
            </div>
          </div>
        )}
        {repositories.length < total && (
          <Button
            variant="secondary"
            disabled={loadingMore}
            onClick={() => void handleShowMore()}
          >
            {loadingMore
              ? "Loading…"
              : `Show more (${total - repositories.length} remaining)`}
          </Button>
        )}
      </section>
    </div>
  );
}
