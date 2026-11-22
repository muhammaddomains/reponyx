import { useCallback, useEffect, useState } from "react";
import {
  agentEnqueue,
  agentHealth,
  agentPause,
  agentResume,
  cancelSyncJob,
  countSyncJobs,
  ensureAgentRunning,
  getRepositoryNames,
  listSyncJobs,
} from "@reponyx/api-client";
import type {
  AgentHealth,
  JobStatus,
  JobStatusCount,
  SyncJob,
} from "@reponyx/shared";
import { Badge, Button, Card } from "@reponyx/ui";

const ROW_CAP = 100;

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

function jobStatusBadge(status: JobStatus) {
  switch (status) {
    case "pending":
      return <Badge variant="warning">pending</Badge>;
    case "running":
      return <Badge>running</Badge>;
    case "completed":
      return <Badge variant="success">completed</Badge>;
    case "failed":
      return <Badge variant="danger">failed</Badge>;
    case "cancelled":
      return <Badge>cancelled</Badge>;
  }
}

function jobTypeLabel(jobType: SyncJob["jobType"]): string {
  switch (jobType) {
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
  }
}

function formatDate(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function countFor(counts: JobStatusCount[], status: JobStatus): number {
  return counts.find((c) => c.status === status)?.count ?? 0;
}

type StatusFilter = "active" | "all" | JobStatus;

/** Synchronization: the live sync queue. Job rows are read from
 *  SQLite and refreshed every 5 seconds — there are no live
 *  progress bars yet, because the queue's progress events are not
 *  forwarded over IPC to the desktop. */
export default function Synchronization() {
  const [jobs, setJobs] = useState<SyncJob[]>([]);
  const [counts, setCounts] = useState<JobStatusCount[]>([]);
  const [health, setHealth] = useState<AgentHealth | null>(null);
  const [repoNames, setRepoNames] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<StatusFilter>("active");
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const [retryingId, setRetryingId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [jobsResult, countsResult, healthResult] =
      await Promise.allSettled([
        filter === "active"
          ? Promise.all([
              listSyncJobs({ status: "pending", limit: ROW_CAP }),
              listSyncJobs({ status: "running", limit: ROW_CAP }),
            ]).then(([pending, running]) =>
              [...pending, ...running].sort(
                (a, b) =>
                  b.priority - a.priority ||
                  a.createdAt.localeCompare(b.createdAt),
              ),
            )
          : listSyncJobs({
              status: filter === "all" ? null : filter,
              limit: ROW_CAP,
            }),
        countSyncJobs(),
        agentHealth(),
      ]);
    if (healthResult.status === "fulfilled") {
      setHealth(healthResult.value);
    }
    if (countsResult.status === "fulfilled") {
      setCounts(countsResult.value);
    }
    if (jobsResult.status === "rejected") {
      throw jobsResult.reason;
    }
    setJobs(jobsResult.value);
  }, [filter]);

  // Repository names only for the jobs actually on screen — the
  // page used to load the whole repository table for this lookup.
  useEffect(() => {
    const unknown = [...new Set(jobs.map((job) => job.repositoryId))].filter(
      (id) => repoNames[id] === undefined,
    );
    if (unknown.length === 0) return;
    let cancelled = false;
    getRepositoryNames(unknown)
      .then((names) => {
        if (cancelled) return;
        setRepoNames((current) => ({ ...current, ...names }));
      })
      .catch(() => {
        // Names are a convenience; ids still identify the job.
      });
    return () => {
      cancelled = true;
    };
  }, [jobs, repoNames]);

  useEffect(() => {
    let cancelled = false;
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
    }, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
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

  async function handleCancel(job: SyncJob) {
    setError(null);
    setNotice(null);
    setCancellingId(job.id);
    try {
      const updated = await cancelSyncJob(job.id);
      setNotice(
        updated.status === "cancelled"
          ? `Cancelled ${repoNames[job.repositoryId] ?? job.repositoryId}.`
          : `Cancel requested for ${repoNames[job.repositoryId] ?? job.repositoryId} — the worker stops it on its next heartbeat.`,
      );
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setCancellingId(null);
    }
  }

  async function handleRetry(job: SyncJob) {
    setError(null);
    setNotice(null);
    setRetryingId(job.id);
    try {
      const agent = await ensureAgentRunning();
      if (!agent.reachable) {
        throw new Error(agent.message || "Background agent is not running.");
      }
      const jobId = await agentEnqueue(job.repositoryId);
      setNotice(
        jobId
          ? `Re-queued ${repoNames[job.repositoryId] ?? job.repositoryId} (job ${jobId.slice(0, 8)}…).`
          : `${repoNames[job.repositoryId] ?? job.repositoryId} already has an active job.`,
      );
      await refresh();
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setRetryingId(null);
    }
  }

  const visible = jobs.slice(0, ROW_CAP);
  const paused = health?.status?.paused ?? false;

  return (
    <div className="space-y-6 p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Synchronization</h1>
          <p className="text-sm text-zinc-500">
            The live sync queue. This page polls the job rows every 5
            seconds — per-job progress bars are not available yet,
            because the queue&apos;s progress events are not forwarded
            over IPC to the desktop.
          </p>
        </div>
        <div className="flex gap-2">
          {paused ? (
            <Button
              disabled={busy || !health?.reachable}
              onClick={() =>
                run(() => agentResume(), () => setNotice("Queue resumed."))
              }
            >
              Resume queue
            </Button>
          ) : (
            <Button
              disabled={busy || !health?.reachable}
              onClick={() =>
                run(() => agentPause(), () => setNotice("Queue paused."))
              }
            >
              Pause queue
            </Button>
          )}
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => void run(() => refresh())}
          >
            Refresh
          </Button>
        </div>
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

      <Card className="space-y-2 p-4">
        <p className="text-sm text-zinc-600 dark:text-zinc-400">
          {health == null
            ? "Checking the background agent…"
            : health.reachable
              ? `Agent ${paused ? "paused" : "running"} · ${health.status?.pendingJobs ?? 0} pending · ${health.status?.runningJobs ?? 0} running · ${health.status?.workers ?? 0} workers`
              : `Agent not running — ${health.message}`}
        </p>
        <p className="text-sm">
          Pending {countFor(counts, "pending")} · Running{" "}
          {countFor(counts, "running")} · Completed{" "}
          {countFor(counts, "completed")} · Failed{" "}
          {countFor(counts, "failed")} · Cancelled{" "}
          {countFor(counts, "cancelled")}
        </p>
      </Card>

      <section className="space-y-4">
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="text-lg font-medium">Jobs</h2>
          <select
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
            value={filter}
            onChange={(e) => setFilter(e.target.value as StatusFilter)}
          >
            <option value="active">Active (pending + running)</option>
            <option value="all">All</option>
            <option value="pending">Pending</option>
            <option value="running">Running</option>
            <option value="completed">Completed</option>
            <option value="failed">Failed</option>
            <option value="cancelled">Cancelled</option>
          </select>
          {!loading && (
            <span className="text-sm text-zinc-500">
              {jobs.length} job{jobs.length === 1 ? "" : "s"}
              {jobs.length > ROW_CAP
                ? ` — showing the first ${ROW_CAP}`
                : ""}
            </span>
          )}
        </div>

        {loading && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Loading jobs…
            </p>
          </Card>
        )}
        {!loading && jobs.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              No jobs match this filter. Jobs appear here when a schedule
              fires, when you press Sync now on a repository, or when the
              agent retries a failed sync.
            </p>
          </Card>
        )}
        {visible.map((job) => {
          const availableAt = job.availableAt
            ? new Date(job.availableAt)
            : null;
          const waiting =
            job.status === "pending" &&
            availableAt !== null &&
            !Number.isNaN(availableAt.getTime()) &&
            availableAt.getTime() > Date.now();
          return (
            <Card key={job.id} className="space-y-3 p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <span className="font-medium">
                    {repoNames[job.repositoryId] ?? job.repositoryId}
                  </span>
                  <span className="ml-2 text-sm text-zinc-500">
                    {jobTypeLabel(job.jobType)} · priority {job.priority} ·
                    attempt {job.attemptCount}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  {jobStatusBadge(job.status)}
                  {job.cancelRequestedAt && job.status === "running" && (
                    <Badge variant="warning">cancel requested</Badge>
                  )}
                </div>
              </div>
              <p className="text-sm text-zinc-600 dark:text-zinc-400">
                Created {formatDate(job.createdAt)}
                {job.startedAt ? ` · started ${formatDate(job.startedAt)}` : ""}
                {job.completedAt
                  ? ` · finished ${formatDate(job.completedAt)}`
                  : ""}
                {job.workerId ? ` · ${job.workerId}` : ""}
                {waiting
                  ? ` · next attempt at ${formatDate(job.availableAt)}`
                  : ""}
              </p>
              {job.status === "failed" && (
                <p className="text-sm text-red-800 dark:text-red-300">
                  {job.errorCode ? `${job.errorCode}: ` : ""}
                  {job.errorMessage ?? "Unknown error."}
                </p>
              )}
              <div className="flex flex-wrap gap-2">
                {(job.status === "pending" || job.status === "running") && (
                  <Button
                    variant="danger"
                    disabled={busy || cancellingId === job.id}
                    onClick={() => void handleCancel(job)}
                  >
                    {cancellingId === job.id ? "Cancelling…" : "Cancel"}
                  </Button>
                )}
                {(job.status === "failed" || job.status === "cancelled") && (
                  <Button
                    variant="secondary"
                    disabled={busy || retryingId === job.id}
                    onClick={() => void handleRetry(job)}
                  >
                    {retryingId === job.id ? "Queuing…" : "Retry now"}
                  </Button>
                )}
              </div>
            </Card>
          );
        })}
      </section>
    </div>
  );
}
