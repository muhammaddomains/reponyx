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
    const result = await postJson("/api/auth/reset/confirm", {
      token,
      password,
    });
    setBusy(false);
    if (result.ok) {
      setDone(true);
      return;
    }
    setMessage(errorMessage(errors, locale, result.data));
  }

  if (done) return <p className={alertSuccess}>{strings.resetSuccess}</p>;

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      <div className={fieldWrapper}>
        <label htmlFor="reset-new-password" className={fieldLabel}>
          {strings.newPassword}
        </label>
        <input
          id="reset-new-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={8}
          className={textInput}
        />
      </div>
      <div className={fieldWrapper}>
        <label htmlFor="reset-confirm-password" className={fieldLabel}>
          {strings.confirmPassword}
        </label>
        <input
          id="reset-confirm-password"
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          required
          minLength={8}
          className={textInput}
        />
      </div>
      {message ? (
        <p role="alert" className={alertError}>
          {message}
        </p>
      ) : null}
      <button type="submit" disabled={busy} className={primaryButton}>
        {strings.changePasswordButton}
      </button>
    </form>
  );
}
