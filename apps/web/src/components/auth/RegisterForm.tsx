"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import {
  alertError,
  alertSuccess,
  fieldLabel,
  fieldWrapper,
  primaryButton,
  textInput,
} from "@/components/ui";

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

  if (done) return <p className={alertSuccess}>{strings.registerSuccess}</p>;

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      <div className={fieldWrapper}>
        <label htmlFor="register-email" className={fieldLabel}>
          {strings.email}
        </label>
        <input
          id="register-email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          required
          className={textInput}
        />
      </div>
      <div className={fieldWrapper}>
        <label htmlFor="register-password" className={fieldLabel}>
          {strings.password}
        </label>
        <input
          id="register-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
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
        {strings.registerButton}
      </button>
    </form>
  );
}
