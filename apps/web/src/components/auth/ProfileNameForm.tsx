"use client";

import { useState, type FormEvent } from "react";
import { errorMessage } from "./api";
import {
  alertError,
  alertSuccess,
  fieldLabel,
  fieldWrapper,
  primaryButton,
  textInput,
} from "@/components/ui";

/** Edit the account's full name (completes the profile). */
export function ProfileNameForm({
  initialName,
  strings,
  errors,
  locale,
}: {
  initialName: string | null;
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const [name, setName] = useState(initialName ?? "");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    setSaved(false);
    const res = await fetch("/api/me", {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    });
    setBusy(false);
    if (res.ok) {
      setSaved(true);
      return;
    }
    const data = (await res.json().catch(() => ({}))) as Record<
      string,
      unknown
    >;
    setMessage(errorMessage(errors, locale, data));
  }

  return (
    <form onSubmit={onSubmit} className="mt-2 space-y-3">
      <div className={fieldWrapper}>
        <label htmlFor="profile-name" className={fieldLabel}>
          {strings.fullName}
        </label>
        <input
          id="profile-name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          maxLength={128}
          autoComplete="name"
          className={textInput}
        />
      </div>
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
      {saved ? <p className={alertSuccess}>{strings.nameUpdated}</p> : null}
      <button type="submit" disabled={busy} className={primaryButton}>
        {strings.save}
      </button>
    </form>
  );
}
