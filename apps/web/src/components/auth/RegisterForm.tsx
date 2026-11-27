"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";

export interface AuthFormProps {
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}

export function RegisterForm({ strings, errors, locale }: AuthFormProps) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    const result = await postJson("/api/auth/register", { email, password });
    setBusy(false);
    if (result.ok) {
      setDone(true);
      return;
    }
    setMessage(errorMessage(errors, locale, result.data));
  }

  if (done) return <p>{strings.registerSuccess}</p>;

  return (
    <form onSubmit={onSubmit}>
      <label>
        {strings.email}
        <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required />
      </label>
      <label>
        {strings.password}
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={8}
        />
      </label>
      {message ? <p role="alert">{message}</p> : null}
      <button type="submit" disabled={busy}>
        {strings.registerButton}
      </button>
    </form>
  );
}
