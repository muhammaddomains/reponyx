"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";

export function ResetConfirmForm({
  token,
  strings,
  errors,
  locale,
}: AuthFormProps & { token: string }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (password !== confirm) {
      setMessage(strings.passwordsDoNotMatch);
      return;
    }
    setBusy(true);
    setMessage(null);
    const result = await postJson("/api/auth/reset/confirm", { token, password });
    setBusy(false);
    if (result.ok) {
      setDone(true);
      return;
    }
    setMessage(errorMessage(errors, locale, result.data));
  }

  if (done) return <p>{strings.resetSuccess}</p>;

  return (
    <form onSubmit={onSubmit}>
      <label>
        {strings.newPassword}
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={8}
        />
      </label>
      <label>
        {strings.confirmPassword}
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          required
          minLength={8}
        />
      </label>
      {message ? <p role="alert">{message}</p> : null}
      <button type="submit" disabled={busy}>
        {strings.resetButton}
      </button>
    </form>
  );
}
