/** Login: the web-platform sign-in gate (plan §4, W3).
 *
 *  Rendered outside the app layout — no sidebar — while the app
 *  has no session. Email + password first; when the account has
 *  2FA on, the web stops the login at the 2FA step and this page
 *  shows the code field. Google/GitHub open the web in the system
 *  browser and hand the result back through `reponyx://auth`,
 *  which App parses and passes in as props. Registration happens
 *  on the web only — the button just opens the register page. */

import { useState, type FormEvent } from "react";
import {
  webLogin,
  webOpenOAuth,
  webOpenRegister,
  webOpenUrl,
  webVerify2fa,
} from "@reponyx/api-client";
import { Button, Card } from "@reponyx/ui";
import {
  commandErrorMessage,
  useLocale,
  useT,
} from "../lib/locale";

const inputClass =
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900";

export interface DeepLinkError {
  code: string;
  retryAt: string | null;
}

interface LoginProps {
  /** The previous session died (the web answered 401). */
  sessionExpired: boolean;
  /** From `reponyx://auth?two_factor_required=1&pending=…`. */
  initialPendingToken: string | null;
  /** From `reponyx://auth?register_complete=…`. */
  registerCompleteToken: string | null;
  /** From `reponyx://auth?error=…`. */
  deepLinkError: DeepLinkError | null;
  /** Clears a stale deep-link error when a new attempt starts, so
   *  an old banner never outlives the attempt that caused it. */
  onClearDeepLinkError: () => void;
  onAuthenticated: () => void;
}

export default function Login({
  sessionExpired,
  initialPendingToken,
  registerCompleteToken,
  deepLinkError,
  onClearDeepLinkError,
  onAuthenticated,
}: LoginProps) {
  const t = useT("auth");
  const tErrors = useT("errors");
  const { locale } = useLocale();
  const [step, setStep] = useState<"login" | "2fa">(
    initialPendingToken !== null ? "2fa" : "login",
  );
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [pendingToken, setPendingToken] = useState<string | null>(
    initialPendingToken,
  );
  const [registerToken, setRegisterToken] = useState<string | null>(
    registerCompleteToken,
  );
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [browserOpened, setBrowserOpened] = useState(false);

  async function handleLogin(event: FormEvent) {
    event.preventDefault();
    setError(null);
    onClearDeepLinkError();
    setBusy(true);
    try {
      const result = await webLogin(email, password);
      if (result.step === "2fa_required") {
        setPendingToken(result.pendingToken);
        setStep("2fa");
        return;
      }
      onAuthenticated();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }

  async function handleVerify(event: FormEvent) {
    event.preventDefault();
    if (pendingToken === null) return;
    setError(null);
    setBusy(true);
    try {
      await webVerify2fa(pendingToken, code);
      onAuthenticated();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }

  async function handleOAuth(provider: "google" | "github") {
    setError(null);
    onClearDeepLinkError();
    setBrowserOpened(true);
    try {
      await webOpenOAuth(provider);
    } catch (e) {
      setError(e);
      setBrowserOpened(false);
    }
  }

  async function handleRegister() {
    setError(null);
    try {
      await webOpenRegister();
    } catch (e) {
      setError(e);
    }
  }

  async function handleCompleteRegistration() {
    if (registerToken === null) return;
    setError(null);
    try {
      await webOpenUrl(
        `/register/complete?token=${encodeURIComponent(registerToken)}`,
      );
    } catch (e) {
      setError(e);
    }
  }

  const alert =
    error !== null
      ? commandErrorMessage(error, tErrors, locale)
      : deepLinkError !== null
        ? commandErrorMessage(deepLinkError, tErrors, locale)
        : null;

  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-50 p-6 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
      <Card className="w-full max-w-md space-y-5 p-6">
        <header className="space-y-1">
          <h1 className="text-2xl font-semibold">RepoNyx</h1>
          <p className="text-sm text-zinc-500">{t("subtitle")}</p>
        </header>

        {sessionExpired && (
          <p
            role="alert"
            className="rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-200"
          >
            {t("sessionExpired")}
          </p>
        )}
        {alert && (
          <p
            role="alert"
            className="rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200"
          >
            {alert}
          </p>
        )}

        {registerToken !== null ? (
          <div className="space-y-3">
            <h2 className="text-lg font-semibold">
              {t("registerCompleteTitle")}
            </h2>
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              {t("registerCompleteBody")}
            </p>
            <Button
              className="w-full"
              disabled={busy}
              onClick={() => void handleCompleteRegistration()}
            >
              {t("registerCompleteButton")}
            </Button>
            <Button
              variant="ghost"
              className="w-full"
              onClick={() => setRegisterToken(null)}
            >
              {t("backToLogin")}
            </Button>
          </div>
        ) : step === "2fa" ? (
          <form className="space-y-4" onSubmit={(e) => void handleVerify(e)}>
            <div className="space-y-1">
              <h2 className="text-lg font-semibold">{t("twoFactorTitle")}</h2>
              <p className="text-sm text-zinc-500">
                {t("twoFactorSubtitle")}
              </p>
            </div>
            <div className="space-y-1">
              <label htmlFor="code" className="text-sm font-medium">
                {t("twoFactorCode")}
              </label>
              <input
                id="code"
                className={inputClass}
                value={code}
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={6}
                disabled={busy}
                onChange={(e) =>
                  setCode(e.target.value.replace(/\D/g, "").slice(0, 6))
                }
              />
            </div>
            <Button type="submit" className="w-full" disabled={busy || code.length !== 6}>
              {t("verifyButton")}
            </Button>
            <Button
              type="button"
              variant="ghost"
              className="w-full"
              onClick={() => {
                setStep("login");
                setError(null);
              }}
            >
              {t("backToLogin")}
            </Button>
          </form>
        ) : (
          <>
            <div className="space-y-2">
              <Button
                variant="secondary"
                className="w-full"
                disabled={busy}
                onClick={() => void handleOAuth("google")}
              >
                {t("googleButton")}
              </Button>
              <Button
                variant="secondary"
                className="w-full"
                disabled={busy}
                onClick={() => void handleOAuth("github")}
              >
                {t("githubButton")}
              </Button>
              {browserOpened && (
                <p className="text-center text-sm text-zinc-500">
                  {t("browserContinueNote")}
                </p>
              )}
            </div>

            <div className="flex items-center gap-3 text-xs text-zinc-400">
              <span className="h-px flex-1 bg-zinc-200 dark:bg-zinc-800" />
              {t("divider")}
              <span className="h-px flex-1 bg-zinc-200 dark:bg-zinc-800" />
            </div>

            <form className="space-y-4" onSubmit={(e) => void handleLogin(e)}>
              <div className="space-y-1">
                <label htmlFor="email" className="text-sm font-medium">
                  {t("email")}
                </label>
                <input
                  id="email"
                  type="email"
                  className={inputClass}
                  value={email}
                  autoComplete="email"
                  disabled={busy}
                  onChange={(e) => setEmail(e.target.value)}
                />
              </div>
              <div className="space-y-1">
                <label htmlFor="password" className="text-sm font-medium">
                  {t("password")}
                </label>
                <input
                  id="password"
                  type="password"
                  className={inputClass}
                  value={password}
                  autoComplete="current-password"
                  disabled={busy}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </div>
              <Button
                type="submit"
                className="w-full"
                disabled={busy || email === "" || password === ""}
              >
                {t("loginButton")}
              </Button>
            </form>
          </>
        )}

        <p className="text-center text-sm text-zinc-500">
          {t("noAccount")}{" "}
          <button
            type="button"
            className="font-medium text-zinc-900 underline dark:text-zinc-100"
            onClick={() => void handleRegister()}
          >
            {t("registerButton")}
          </button>
        </p>
      </Card>
    </div>
  );
}
