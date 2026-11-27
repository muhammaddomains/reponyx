"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { errorMessage } from "./api";
import { alertError, alertSuccess, link, outlineButton } from "@/components/ui";

export interface LinkedProviderInfo {
  provider: string;
  email: string;
  createdAt: string;
}

/**
 * The profile's sign-in methods: Google and GitHub linked to this one
 * account. Link goes through the OAuth flow with intent=link; unlink
 * is a DELETE, refused when it would remove the last way in.
 */
export function LinkedAccounts({
  initial,
  hasPassword,
  strings,
  errors,
  locale,
}: {
  initial: LinkedProviderInfo[];
  hasPassword: boolean;
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const router = useRouter();
  const [linked, setLinked] = useState<LinkedProviderInfo[]>(initial);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  async function refresh() {
    const res = await fetch("/api/me/linked");
    if (res.ok) {
      const data = (await res.json()) as { linked: LinkedProviderInfo[] };
      setLinked(data.linked);
    }
  }

  async function unlink(provider: string) {
    setBusy(provider);
    setMessage(null);
    setNotice(null);
    const res = await fetch(`/api/me/linked/${provider}`, {
      method: "DELETE",
    });
    setBusy(null);
    if (res.ok) {
      setNotice(strings.unlinkedSuccess);
      await refresh();
      router.refresh();
      return;
    }
    const data = (await res.json().catch(() => ({}))) as Record<
      string,
      unknown
    >;
    setMessage(errorMessage(errors, locale, data));
  }

  const isLinked = (provider: string) =>
    linked.some((row) => row.provider === provider);

  return (
    <div className="mt-2 space-y-3">
      {(["google", "github"] as const).map((provider) => {
        const row = linked.find((r) => r.provider === provider);
        return (
          <div
            key={provider}
            className="flex items-center justify-between gap-3"
          >
            <div>
              <p className="text-sm font-medium">
                {provider === "google" ? "Google" : "GitHub"}
              </p>
              <p className="text-sm text-muted-foreground">
                {row ? `${strings.linked} — ${row.email}` : strings.notLinked}
              </p>
            </div>
            {isLinked(provider) ? (
              <button
                type="button"
                disabled={busy === provider}
                onClick={() => unlink(provider)}
                className={outlineButton}
              >
                {strings.unlink}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => {
                  window.location.href = `/api/auth/oauth/${provider}?intent=link`;
                }}
                className={outlineButton}
              >
                {strings.link}
              </button>
            )}
          </div>
        );
      })}
      {notice ? <p className={alertSuccess}>{notice}</p> : null}
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
      {!hasPassword ? (
        <p className="text-sm text-muted-foreground">
          {strings.noPasswordHint}{" "}
          <a href="/reset" className={link}>
            {strings.setPassword}
          </a>
        </p>
      ) : null}
    </div>
  );
}
