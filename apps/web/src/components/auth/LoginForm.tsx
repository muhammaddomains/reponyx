"use client";

import { useState, type FormEvent } from "react";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";
import {
  alertError,
  fieldLabel,
  fieldWrapper,
  primaryButton,
  textInput,
} from "@/components/ui";

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
    <form onSubmit={onSubmit} className="space-y-4">
      <div className={fieldWrapper}>
        <label htmlFor="login-email" className={fieldLabel}>
          {strings.email}
        </label>
        <input
          id="login-email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          required
          className={textInput}
        />
      </div>
      <div className={fieldWrapper}>
        <label htmlFor="login-password" className={fieldLabel}>
          {strings.password}
        </label>
        <input
          id="login-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
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
        {strings.loginButton}
      </button>
    </form>
  );
}
