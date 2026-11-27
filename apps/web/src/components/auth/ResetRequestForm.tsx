"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";
import {
  alertError,
  alertSuccess,
  fieldLabel,
  fieldWrapper,
  primaryButton,
  textInput,
} from "@/components/ui";

export function ResetRequestForm({ strings, errors, locale }: AuthFormProps) {
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    const result = await postJson("/api/auth/reset", { email });
    setBusy(false);
    if (result.ok) {
      setDone(true);
      return;
    }
    setMessage(errorMessage(errors, locale, result.data));
  }

  if (done) return <p className={alertSuccess}>{strings.resetSent}</p>;

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      <div className={fieldWrapper}>
        <label htmlFor="reset-email" className={fieldLabel}>
          {strings.email}
        </label>
        <input
          id="reset-email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          required
          className={textInput}
        />
      </div>
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
      <button type="submit" disabled={busy} className={primaryButton}>
        {strings.resetButton}
      </button>
    </form>
  );
}
