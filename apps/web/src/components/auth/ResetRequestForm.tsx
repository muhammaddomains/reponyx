"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";

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

  if (done) return <p>{strings.resetSent}</p>;

  return (
    <form onSubmit={onSubmit}>
      <label>
        {strings.email}
        <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required />
      </label>
      {message ? <p role="alert">{message}</p> : null}
      <button type="submit" disabled={busy}>
        {strings.resetButton}
      </button>
    </form>
  );
}
