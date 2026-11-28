"use client";

import { useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { errorMessage, postJson } from "@/components/auth/api";
import {
  alertError,
  alertSuccess,
  fieldLabel,
  fieldWrapper,
  primaryButton,
  textInput,
} from "@/components/ui";

export interface PackageOption {
  id: string;
  name: string;
  maxLinkedAccounts: number;
  maxDevices: number;
}

/** Assign a package to a user (admin). Expiry is optional. */
export function AssignPackageForm({
  userId,
  packages,
  currentPackageId,
  strings,
  errors,
  locale,
}: {
  userId: string;
  packages: PackageOption[];
  currentPackageId: string | null;
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const router = useRouter();
  const [packageId, setPackageId] = useState(currentPackageId ?? "");
  const [expiresAt, setExpiresAt] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    setSaved(false);
    const { ok, data } = await postJson(`/api/admin/users/${userId}/package`, {
      packageId,
      expiresAt: expiresAt ? new Date(expiresAt).toISOString() : null,
    });
    setBusy(false);
    if (!ok) return setMessage(errorMessage(errors, locale, data));
    setSaved(true);
    router.refresh();
  }

  return (
    <form onSubmit={onSubmit} className="mt-2 space-y-3">
      <div className={fieldWrapper}>
        <label htmlFor="assign-package" className={fieldLabel}>
          {strings.package}
        </label>
        <select
          id="assign-package"
          value={packageId}
          onChange={(e) => setPackageId(e.target.value)}
          required
          className={textInput}
        >
          <option value="" disabled>
            —
          </option>
          {packages.map((pkg) => (
            <option key={pkg.id} value={pkg.id}>
              {pkg.name} ({pkg.maxLinkedAccounts} / {pkg.maxDevices})
            </option>
          ))}
        </select>
      </div>
      <div className={fieldWrapper}>
        <label htmlFor="assign-expires" className={fieldLabel}>
          {strings.expiresAt}
        </label>
        <input
          id="assign-expires"
          type="datetime-local"
          value={expiresAt}
          onChange={(e) => setExpiresAt(e.target.value)}
          className={textInput}
        />
      </div>
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
      {saved ? <p className={alertSuccess}>{strings.packageAssigned}</p> : null}
      <button type="submit" disabled={busy} className={primaryButton}>
        {strings.assign}
      </button>
    </form>
  );
}
