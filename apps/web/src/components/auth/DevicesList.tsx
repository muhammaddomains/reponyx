"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { errorMessage } from "./api";
import { alertError, alertSuccess, outlineButton } from "@/components/ui";

export interface DeviceInfo {
  id: string;
  machineName: string | null;
  deviceKey: string;
  lastSeenAt: string;
  revokedAt: string | null;
}

/**
 * The dashboard's device list. Revoking a device also stops the
 * broker minting tokens for it (W4 revocation mirroring): the mint
 * endpoint refuses revoked devices.
 */
export function DevicesList({
  initial,
  strings,
  errors,
  locale,
}: {
  initial: DeviceInfo[];
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const router = useRouter();
  const [devices, setDevices] = useState<DeviceInfo[]>(initial);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  async function revoke(id: string) {
    setBusy(id);
    setMessage(null);
    setNotice(null);
    const res = await fetch(`/api/devices/${id}`, { method: "DELETE" });
    setBusy(null);
    if (res.ok) {
      setDevices((rows) =>
        rows.map((row) =>
          row.id === id ? { ...row, revokedAt: new Date().toISOString() } : row
        )
      );
      setNotice(strings.revokedSuccess);
      router.refresh();
      return;
    }
    const data = (await res.json().catch(() => ({}))) as Record<
      string,
      unknown
    >;
    setMessage(errorMessage(errors, locale, data));
  }

  return (
    <div className="mt-2">
      <ul className="divide-y divide-border">
        {devices.map((device) => (
          <li
            key={device.id}
            className="flex items-center justify-between gap-3 py-2 text-sm"
          >
            <div>
              {device.machineName ?? device.deviceKey} — {strings.lastSeen}:{" "}
              {new Intl.DateTimeFormat(locale, {
                dateStyle: "medium",
                timeStyle: "short",
              }).format(new Date(device.lastSeenAt))}
              {device.revokedAt ? ` (${strings.revoked})` : ""}
            </div>
            {!device.revokedAt ? (
              <button
                type="button"
                disabled={busy === device.id}
                onClick={() => revoke(device.id)}
                className={outlineButton}
              >
                {strings.revoke}
              </button>
            ) : null}
          </li>
        ))}
      </ul>
      {notice ? <p className={alertSuccess}>{notice}</p> : null}
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
    </div>
  );
}
