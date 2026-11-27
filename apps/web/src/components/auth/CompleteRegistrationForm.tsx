"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import {
  alertError,
  fieldLabel,
  fieldWrapper,
  primaryButton,
  textInput,
} from "@/components/ui";

export function CompleteRegistrationForm({
  token,
  email,
  strings,
  errors,
  locale,
}: {
  token: string;
  email: string;
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    const result = await postJson("/api/auth/register/complete", {
      token,
      name,
    });
    setBusy(false);
    if (result.ok) {
      window.location.href = "/dashboard";
      return;
    }
    setMessage(errorMessage(errors, locale, result.data));
  }

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      <div className={fieldWrapper}>
        <label htmlFor="complete-email" className={fieldLabel}>
          {strings.email}
        </label>
        <input
          id="complete-email"
          type="email"
          value={email}
          readOnly
          disabled
          className={textInput}
        />
      </div>
      <div className={fieldWrapper}>
        <label htmlFor="complete-name" className={fieldLabel}>
          {strings.fullName}
        </label>
        <input
          id="complete-name"
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
      <button type="submit" disabled={busy} className={primaryButton}>
        {strings.registerButton}
      </button>
    </form>
  );
}
