/** App shell: the login gate (plan §4, W3).
 *
 *  On mount the app asks the backend whether a web session exists.
 *  No session (or a dead one) → the Login page, outside the app
 *  layout. A session → the normal sidebar layout. A deep link
 *  (`reponyx://auth?...`) from the browser OAuth flow is handled
 *  here: an auth code is exchanged for a session, a 2FA outcome
 *  hands its pending token to Login, and a registration that still
 *  needs a name hands its token to Login. Deep links are ignored
 *  while already signed in — exchanging one then could sign a
 *  second account in over the first. */

import { useCallback, useEffect, useRef, useState } from "react";
import { HashRouter, Route, Routes } from "react-router";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import {
  isTauriAvailable,
  webExchangeDesktopCode,
  webLogout,
  webSessionStatus,
} from "@reponyx/api-client";
import type { WebAccount } from "@reponyx/shared";
import AppLayout from "./layouts/AppLayout";
import { LocaleProvider, useT } from "./lib/locale";
import Accounts from "./routes/Accounts";
import Activity from "./routes/Activity";
import AddAccount from "./routes/AddAccount";
import Agent from "./routes/Agent";
import Login, { type DeepLinkError } from "./routes/Login";
import Overview from "./routes/Overview";
import Repositories from "./routes/Repositories";
import Schedules from "./routes/Schedules";
import Settings from "./routes/Settings";
import Storage from "./routes/Storage";
import Synchronization from "./routes/Synchronization";

type AuthState =
  | { status: "loading" }
  | {
      status: "authenticated";
      account: WebAccount | null;
      offline: boolean;
    }
  | { status: "unauthenticated"; sessionExpired: boolean };

type DeepLinkOutcome =
  | { kind: "code"; code: string }
  | { kind: "2fa"; pending: string }
  | { kind: "register_complete"; token: string }
  | { kind: "error"; code: string; retryAt: string | null };

/** Parse `reponyx://auth?...` — the only deep-link URL the app
 *  understands. Anything else (or a malformed URL) is ignored. */
function parseAuthUrl(raw: string): DeepLinkOutcome | null {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== "reponyx:" || url.hostname !== "auth") return null;
  const params = url.searchParams;
  const code = params.get("code");
  if (code !== null) return { kind: "code", code };
  if (params.get("two_factor_required") === "1") {
    const pending = params.get("pending");
    if (pending !== null) return { kind: "2fa", pending };
  }
  const registerComplete = params.get("register_complete");
  if (registerComplete !== null) {
    return { kind: "register_complete", token: registerComplete };
  }
  const error = params.get("error");
  if (error !== null) {
    return { kind: "error", code: error, retryAt: params.get("retry_at") };
  }
  return null;
}

export default function App() {
  return (
    <LocaleProvider>
      <AppShell />
    </LocaleProvider>
  );
}

function AppShell() {
  const tCommon = useT("common");
  const [auth, setAuth] = useState<AuthState>({ status: "loading" });
  const [pendingToken, setPendingToken] = useState<string | null>(null);
  const [registerToken, setRegisterToken] = useState<string | null>(null);
  const [deepLinkError, setDeepLinkError] = useState<DeepLinkError | null>(
    null,
  );
  const authRef = useRef(auth);
  authRef.current = auth;

  const refreshStatus = useCallback(async () => {
    try {
      const status = await webSessionStatus();
      if (status.authenticated) {
        setAuth({
          status: "authenticated",
          account: status.account,
          offline: status.offline,
        });
      } else {
        setAuth({
          status: "unauthenticated",
          sessionExpired: status.sessionExpired,
        });
      }
    } catch {
      // No backend (plain-browser preview) — show the login page,
      // whose calls fail with the same clear message.
      setAuth({ status: "unauthenticated", sessionExpired: false });
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const handleDeepLink = useCallback(
    async (urls: string[]) => {
      // Only while signed out (see the module doc).
      if (authRef.current.status !== "unauthenticated") return;
      for (const raw of urls) {
        const outcome = parseAuthUrl(raw);
        if (outcome === null) continue;
        if (outcome.kind === "code") {
          try {
            await webExchangeDesktopCode(outcome.code);
            setDeepLinkError(null);
            await refreshStatus();
          } catch (e) {
            const record = e as { code?: unknown; retryAt?: unknown };
            setDeepLinkError({
              code:
                typeof record.code === "string"
                  ? record.code
                  : "oauth_failed",
              retryAt:
                typeof record.retryAt === "string" ? record.retryAt : null,
            });
          }
        } else if (outcome.kind === "2fa") {
          setDeepLinkError(null);
          setPendingToken(outcome.pending);
        } else if (outcome.kind === "register_complete") {
          setDeepLinkError(null);
          setRegisterToken(outcome.token);
        } else {
          setDeepLinkError({
            code: outcome.code,
            retryAt: outcome.retryAt,
          });
        }
      }
    },
    [refreshStatus],
  );

  useEffect(() => {
    if (!isTauriAvailable()) return;
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    onOpenUrl((urls) => void handleDeepLink(urls))
      .then((fn) => {
        if (!cancelled) unlisten = fn;
        else fn();
      })
      .catch(() => {
        // The deep-link plugin is unavailable — login still works
        // by email; OAuth just cannot hand its result back.
      });
    getCurrent()
      .then((urls) => {
        if (urls !== null) void handleDeepLink(urls);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      if (unlisten !== null) unlisten();
    };
  }, [handleDeepLink]);

  const handleLogout = useCallback(async () => {
    try {
      await webLogout();
    } catch {
      // Logging out locally must not fail on a dead network.
    }
    setPendingToken(null);
    setRegisterToken(null);
    setDeepLinkError(null);
    setAuth({ status: "unauthenticated", sessionExpired: false });
  }, []);

  if (auth.status === "loading") {
    return (
      <div className="flex min-h-screen items-center justify-center bg-zinc-50 text-sm text-zinc-500 dark:bg-zinc-950 dark:text-zinc-400">
        {tCommon("loading")}
      </div>
    );
  }

  if (auth.status === "unauthenticated") {
    return (
      <Login
        sessionExpired={auth.sessionExpired}
        initialPendingToken={pendingToken}
        registerCompleteToken={registerToken}
        deepLinkError={deepLinkError}
        onAuthenticated={() => void refreshStatus()}
      />
    );
  }

  return (
    <HashRouter>
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<Overview />} />
          <Route path="accounts" element={<Accounts />} />
          <Route path="accounts/add" element={<AddAccount />} />
          <Route path="agent" element={<Agent />} />
          <Route path="repositories" element={<Repositories />} />
          <Route path="synchronization" element={<Synchronization />} />
          <Route path="schedules" element={<Schedules />} />
          <Route path="activity" element={<Activity />} />
          <Route path="storage" element={<Storage />} />
          <Route
            path="settings"
            element={
              <Settings
                webAccount={auth.account}
                webOffline={auth.offline}
                onWebLogout={() => void handleLogout()}
              />
            }
          />
        </Route>
      </Routes>
    </HashRouter>
  );
}
