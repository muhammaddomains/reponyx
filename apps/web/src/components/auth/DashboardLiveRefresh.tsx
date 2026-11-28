"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";

/**
 * Live refresh (W4-full): the dashboard re-reads the database every 30s
 * and whenever the window regains focus, so linked accounts, devices and
 * counts move without a reload — changes made in the desktop app show
 * up here in real time.
 */
export function DashboardLiveRefresh({
  strings,
  locale,
}: {
  strings: { live: string; updatedAt: string };
  locale: string;
}) {
  const router = useRouter();
  const [updated, setUpdated] = useState(() => new Date());
  useEffect(() => {
    const refresh = () => {
      router.refresh();
      setUpdated(new Date());
    };
    const id = window.setInterval(refresh, 30_000);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("focus", refresh);
    };
  }, [router]);
  return (
    <p className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
      <span
        className="inline-block h-2 w-2 rounded-full bg-emerald-500"
        aria-hidden="true"
      />
      {strings.live} ·{" "}
      {strings.updatedAt.replace(
        "{time}",
        updated.toLocaleTimeString(locale),
      )}
    </p>
  );
}
