"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";

export function LoginForm({ strings, errors, locale }: AuthFormProps) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    const result = await postJson("/api/auth/login", { email, password });
    setBusy(false);
    if (result.ok) {
      // The session cookie is set; the dashboard is the next page.
      window.location.href = "/dashboard";
      return;
    }
    setMessage(errorMessage(errors, locale, result.data));
  }

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
        />
      </label>
      {message ? <p role="alert">{message}</p> : null}
      <button type="submit" disabled={busy}>
        {strings.loginButton}
      </button>
    </form>
  );
}
