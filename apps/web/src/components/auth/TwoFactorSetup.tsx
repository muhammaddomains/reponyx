"use client";

import { useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { errorMessage, postJson } from "./api";
import {
  alertError,
  alertSuccess,
  fieldLabel,
  fieldWrapper,
  outlineButton,
  primaryButton,
  textInput,
} from "@/components/ui";

/**
 * Two-factor authentication on the dashboard: enroll with an
 * authenticator app (QR + manual secret), confirm with a code, and
 * keep the one-time backup codes. Disabling needs the password
 * (password accounts) or a TOTP code (OAuth-only accounts). Admins
 * cannot disable it — 2FA is mandatory for admin accounts.
 */
export function TwoFactorSetup({
  initialEnabled,
  initialRemaining,
  hasPassword,
  isAdmin,
  strings,
  errors,
  locale,
}: {
  initialEnabled: boolean;
  initialRemaining: number;
  hasPassword: boolean;
  isAdmin: boolean;
  strings: Record<string, string>;
  errors: Record<string, string>;
  locale: string;
}) {
  const router = useRouter();
  const [enabled, setEnabled] = useState(initialEnabled);
  const [remaining, setRemaining] = useState(initialRemaining);
  const [setup, setSetup] = useState<{
    qrDataUrl: string;
    secret: string;
  } | null>(null);
  const [backupCodes, setBackupCodes] = useState<string[] | null>(null);
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  function fail(data: Record<string, unknown>) {
    setMessage(errorMessage(errors, locale, data));
  }

  async function startSetup() {
    setBusy(true);
    setMessage(null);
    setNotice(null);
    const { ok, data } = await postJson("/api/auth/2fa/setup", {});
    setBusy(false);
    if (!ok) return fail(data);
    setSetup({
      qrDataUrl: data.qrDataUrl as string,
      secret: data.secret as string,
    });
  }

  async function confirmSetup(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    setNotice(null);
    const { ok, data } = await postJson("/api/auth/2fa/confirm", { code });
    setBusy(false);
    if (!ok) return fail(data);
    setBackupCodes(data.backupCodes as string[]);
    setRemaining((data.backupCodes as string[]).length);
    setSetup(null);
    setCode("");
    setEnabled(true);
    setNotice(strings.enabledSuccess);
    router.refresh();
  }

  async function regenerate(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    setNotice(null);
    const { ok, data } = await postJson("/api/auth/2fa/backup-codes", {
      code,
    });
    setBusy(false);
    if (!ok) return fail(data);
    setBackupCodes(data.backupCodes as string[]);
    setRemaining((data.backupCodes as string[]).length);
    setCode("");
    setNotice(strings.regeneratedSuccess);
    router.refresh();
  }

  async function disable(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    setNotice(null);
    const { ok, data } = await postJson(
      "/api/auth/2fa/disable",
      hasPassword ? { password } : { code }
    );
    setBusy(false);
    if (!ok) return fail(data);
    setEnabled(false);
    setRemaining(0);
    setBackupCodes(null);
    setPassword("");
    setCode("");
    setNotice(strings.disabledSuccess);
    router.refresh();
  }

  return (
    <div>
      <p className="mt-1 text-sm text-muted-foreground">{strings.subtitle}</p>
      <p className="mt-2 text-sm">
        {enabled ? strings.enabled : strings.disabled}
        {enabled
          ? ` — ${strings.remaining.replace("{count}", String(remaining))}`
          : ""}
      </p>

      {message ? (
        <p role="alert" className={`${alertError} mt-3`}>
          {message}
        </p>
      ) : null}
      {notice ? <p className={`${alertSuccess} mt-3`}>{notice}</p> : null}

      {!enabled && !setup ? (
        <button
          type="button"
          onClick={startSetup}
          disabled={busy}
          className={`${primaryButton} mt-3`}
        >
          {strings.enable}
        </button>
      ) : null}

      {!enabled && setup ? (
        <form onSubmit={confirmSetup} className="mt-3 space-y-3">
          <p className="text-sm font-medium">{strings.setupTitle}</p>
          <p className="text-sm text-muted-foreground">
            {strings.setupSubtitle}
          </p>
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={setup.qrDataUrl}
            alt={strings.setupTitle}
            width={192}
            height={192}
            className="rounded-md border border-border bg-white p-2"
          />
          <p className="text-sm">
            {strings.secretLabel}:{" "}
            <code className="rounded bg-muted px-1 py-0.5 text-xs">
              {setup.secret}
            </code>
          </p>
          <div className={fieldWrapper}>
            <label htmlFor="totp-confirm-code" className={fieldLabel}>
              {strings.confirmLabel}
            </label>
            <input
              id="totp-confirm-code"
              type="text"
              inputMode="numeric"
              autoComplete="one-time-code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              required
              maxLength={16}
              className={textInput}
            />
          </div>
          <button type="submit" disabled={busy} className={primaryButton}>
            {strings.confirm}
          </button>
        </form>
      ) : null}

      {enabled ? (
        <div className="mt-3 space-y-3">
          <form onSubmit={regenerate} className="space-y-3">
            <div className={fieldWrapper}>
              <label htmlFor="totp-regen-code" className={fieldLabel}>
                {strings.codeLabel}
              </label>
              <input
                id="totp-regen-code"
                type="text"
                inputMode="numeric"
                autoComplete="one-time-code"
                value={code}
                onChange={(e) => setCode(e.target.value)}
                required
                maxLength={16}
                className={textInput}
              />
            </div>
            <button type="submit" disabled={busy} className={outlineButton}>
              {strings.regenerate}
            </button>
          </form>
          {isAdmin ? (
            <p className="text-sm text-muted-foreground">
              {strings.adminRequired}
            </p>
          ) : (
            <form onSubmit={disable} className="space-y-3">
              <p className="text-sm text-muted-foreground">
                {strings.disableConfirm}
              </p>
              <div className={fieldWrapper}>
                <label htmlFor="totp-disable-proof" className={fieldLabel}>
                  {hasPassword ? strings.passwordLabel : strings.codeLabel}
                </label>
                <input
                  id="totp-disable-proof"
                  type={hasPassword ? "password" : "text"}
                  inputMode={hasPassword ? undefined : "numeric"}
                  autoComplete={
                    hasPassword ? "current-password" : "one-time-code"
                  }
                  value={hasPassword ? password : code}
                  onChange={(e) =>
                    hasPassword
                      ? setPassword(e.target.value)
                      : setCode(e.target.value)
                  }
                  required
                  maxLength={128}
                  className={textInput}
                />
              </div>
              <button type="submit" disabled={busy} className={outlineButton}>
                {strings.disable}
              </button>
            </form>
          )}
        </div>
      ) : null}

      {backupCodes ? (
        <div className="mt-3">
          <p className="text-sm font-medium">{strings.backupCodesTitle}</p>
          <p className="text-sm text-muted-foreground">
            {strings.backupCodesSubtitle}
          </p>
          <ul className="mt-2 space-y-1">
            {backupCodes.map((backupCode) => (
              <li key={backupCode}>
                <code className="rounded bg-muted px-1 py-0.5 text-xs">
                  {backupCode}
                </code>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
