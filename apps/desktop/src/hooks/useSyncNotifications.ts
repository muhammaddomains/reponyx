import { useEffect, useRef } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import {
  getRepositoryNames,
  getSettings,
  listSyncJobs,
} from "@reponyx/api-client";
import type { SyncJob } from "@reponyx/shared";

/** Poll interval for finished jobs. The Synchronization page polls
 *  every 5 s for its own list; this hook only needs to notice a
 *  job finishing, so 30 s is plenty and cheap (one bounded query
 *  plus a settings read). */
const POLL_MS = 30_000;

function jobLabel(job: SyncJob): string {
  switch (job.jobType) {
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

/** System notifications for finished sync jobs (Phase 8).
 *  Mounted once in the app layout, so it runs on every page.
 *  The first poll only seeds the seen set — jobs that finished
 *  before the app started are history, not news. Later polls
 *  notify once per newly finished job, naming the repository.
 *  The `notifications.enabled` setting is re-read on every poll,
 *  so toggling it on the Settings page takes effect without a
 *  restart. Nothing here may break the app: every failure is
 *  swallowed and the next poll retries. */
export function useSyncNotifications(): void {
  const seenRef = useRef<Set<string> | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setInterval> | undefined;

    async function poll(): Promise<void> {
      try {
        const settings = await getSettings();
        if (cancelled || settings.notificationsEnabled === false) return;
        const jobs = await listSyncJobs({ limit: 100 });
        if (cancelled) return;
        const finished = jobs.filter(
          (job) => job.status === "completed" || job.status === "failed",
        );
        const seen = seenRef.current;
        if (seen === null) {
          seenRef.current = new Set(finished.map((job) => job.id));
          return;
        }
        const fresh = finished.filter((job) => !seen.has(job.id));
        if (fresh.length === 0) return;
        for (const job of fresh) seen.add(job.id);
        const names = await getRepositoryNames(
          fresh.map((job) => job.repositoryId),
        );
        if (cancelled) return;
        for (const job of fresh) {
          const name = names[job.repositoryId] ?? "a repository";
          if (job.status === "completed") {
            await sendNotification({
              title: "Sync finished",
              body: `${jobLabel(job)} finished for ${name}.`,
            });
          } else {
            await sendNotification({
              title: "Sync failed",
              body: `${jobLabel(job)} failed for ${name}${
                job.errorMessage ? ` — ${job.errorMessage}` : "."
              }`,
            });
          }
        }
      } catch {
        // A failed poll just means this round is skipped.
      }
    }

    async function start(): Promise<void> {
      try {
        const settings = await getSettings();
        if (cancelled || settings.notificationsEnabled === false) return;
        if (!(await isPermissionGranted())) {
          if ((await requestPermission()) !== "granted") return;
        }
        if (cancelled) return;
        await poll();
        if (cancelled) return;
        timer = setInterval(() => void poll(), POLL_MS);
      } catch {
        // Notifications must never break the app.
      }
    }

    void start();
    return () => {
      cancelled = true;
      if (timer !== undefined) clearInterval(timer);
    };
  }, []);
}
