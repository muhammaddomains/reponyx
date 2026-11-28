"use client";

import { useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { errorMessage, postJson } from "@/components/auth/api";
import {
  alertError,
  alertSuccess,
  fieldLabel,
  fieldWrapper,
  outlineButton,
  primaryButton,
  textInput,
} from "@/components/ui";

export interface PackageInfo {
  id: string;
  name: string;
  maxLinkedAccounts: number;
  maxDevices: number;
  priceCents: number;
  isActive: boolean;
}

function PackageForm({
  initial,
  onDone,
  strings,
  errors,
  locale,
}: {
  initial: PackageInfo | null;
  onDone: (pkg: PackageInfo) => void;
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [maxLinkedAccounts, setMaxLinkedAccounts] = useState(
    String(initial?.maxLinkedAccounts ?? 6)
  );
  const [maxDevices, setMaxDevices] = useState(
    String(initial?.maxDevices ?? 2)
  );
  const [priceCents, setPriceCents] = useState(
    String(initial?.priceCents ?? 0)
  );
  const [isActive, setIsActive] = useState(initial?.isActive ?? true);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    const body = {
      name,
      maxLinkedAccounts: Number(maxLinkedAccounts),
      maxDevices: Number(maxDevices),
      priceCents: Number(priceCents),
      ...(initial ? { isActive } : {}),
    };
    if (!initial) {
      const created = await postJson("/api/admin/packages", body);
      setBusy(false);
      if (!created.ok)
        return setMessage(errorMessage(errors, locale, created.data));
      onDone(created.data.package as unknown as PackageInfo);
      return;
    }
    const res = await fetch(`/api/admin/packages/${initial.id}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    setBusy(false);
    if (!res.ok) {
      const data = (await res.json().catch(() => ({}))) as Record<
        string,
        unknown
      >;
      return setMessage(errorMessage(errors, locale, data));
    }
    const data = (await res.json()) as {
      package: Record<string, unknown>;
    };
    onDone({
      id: String(data.package.id),
      name: String(data.package.name),
      maxLinkedAccounts: Number(data.package.max_linked_accounts),
      maxDevices: Number(data.package.max_devices),
      priceCents: Number(data.package.price_cents),
      isActive: Boolean(data.package.is_active),
    });
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <div className={fieldWrapper}>
        <label
          htmlFor={`pkg-name-${initial?.id ?? "new"}`}
          className={fieldLabel}
        >
          {strings.packageName}
        </label>
        <input
          id={`pkg-name-${initial?.id ?? "new"}`}
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          maxLength={64}
          className={textInput}
        />
      </div>
      <div className="grid gap-3 sm:grid-cols-3">
        <div className={fieldWrapper}>
          <label
            htmlFor={`pkg-linked-${initial?.id ?? "new"}`}
            className={fieldLabel}
          >
            {strings.maxLinkedAccounts}
          </label>
          <input
            id={`pkg-linked-${initial?.id ?? "new"}`}
            type="number"
            min={0}
            value={maxLinkedAccounts}
            onChange={(e) => setMaxLinkedAccounts(e.target.value)}
            required
            className={textInput}
          />
        </div>
        <div className={fieldWrapper}>
          <label
            htmlFor={`pkg-devices-${initial?.id ?? "new"}`}
            className={fieldLabel}
          >
            {strings.maxDevices}
          </label>
          <input
            id={`pkg-devices-${initial?.id ?? "new"}`}
            type="number"
            min={0}
            value={maxDevices}
            onChange={(e) => setMaxDevices(e.target.value)}
            required
            className={textInput}
          />
        </div>
        <div className={fieldWrapper}>
          <label
            htmlFor={`pkg-price-${initial?.id ?? "new"}`}
            className={fieldLabel}
          >
            {strings.priceCents}
          </label>
          <input
            id={`pkg-price-${initial?.id ?? "new"}`}
            type="number"
            min={0}
            value={priceCents}
            onChange={(e) => setPriceCents(e.target.value)}
            required
            className={textInput}
          />
        </div>
      </div>
      {initial ? (
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={isActive}
            onChange={(e) => setIsActive(e.target.checked)}
          />
          {strings.active}
        </label>
      ) : null}
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
      <button type="submit" disabled={busy} className={primaryButton}>
        {initial ? strings.save : strings.createPackage}
      </button>
    </form>
  );
}

/** The admin's package list: create a package, edit limits, retire/reactivate. */
export function PackageManager({
  initial,
  strings,
  errors,
  locale,
}: {
  initial: PackageInfo[];
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const router = useRouter();
  const [packages, setPackages] = useState<PackageInfo[]>(initial);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  function upsert(pkg: PackageInfo, created: boolean) {
    setPackages((rows) => {
      const exists = rows.some((row) => row.id === pkg.id);
      return exists
        ? rows.map((row) => (row.id === pkg.id ? pkg : row))
        : [...rows, pkg];
    });
    setEditingId(null);
    setCreating(false);
    setNotice(created ? strings.packageCreated : strings.packageUpdated);
    router.refresh();
  }

  return (
    <div className="mt-6 space-y-6">
      {notice ? <p className={alertSuccess}>{notice}</p> : null}
      {creating ? (
        <section className="rounded-lg border border-border bg-card p-4">
          <PackageForm
            initial={null}
            onDone={(pkg) => upsert(pkg, true)}
            strings={strings}
            errors={errors}
            locale={locale}
          />
        </section>
      ) : (
        <button
          type="button"
          onClick={() => setCreating(true)}
          className={outlineButton}
        >
          {strings.createPackage}
        </button>
      )}
      <ul className="space-y-4">
        {packages.map((pkg) => (
          <li
            key={pkg.id}
            className="rounded-lg border border-border bg-card p-4"
          >
            <p className="font-medium">
              {pkg.name} — {pkg.maxLinkedAccounts} / {pkg.maxDevices}
              {pkg.priceCents > 0 ? ` — ${pkg.priceCents}¢` : ""}
              {!pkg.isActive ? ` (${strings.retired})` : ""}
            </p>
            {editingId === pkg.id ? (
              <div className="mt-3">
                <PackageForm
                  initial={pkg}
                  onDone={(updated) => upsert(updated, false)}
                  strings={strings}
                  errors={errors}
                  locale={locale}
                />
              </div>
            ) : (
              <button
                type="button"
                onClick={() => setEditingId(pkg.id)}
                className={`${outlineButton} mt-3`}
              >
                {strings.edit}
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
