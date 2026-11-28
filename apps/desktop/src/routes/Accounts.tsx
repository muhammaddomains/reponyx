import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import {
  deletePatAccount,
  listAccounts,
  listBrokerInstallations,
  reconnectPatAccount,
  removePatAccount,
  replacePatAccount,
  validateGithubAppAccount,
  validatePatAccount,
  webLinkGithubAccount,
  webLinkPatAccount,
  webUnlinkPatAccount,
} from "@reponyx/api-client";
import type { Account, BrokerInstallation } from "@reponyx/shared";
import { Badge, Button, Card, ConfirmDialog } from "@reponyx/ui";
import AccountsTabs from "../components/AccountsTabs";
import { notifyWebAccountChanged } from "../webAccountSync";

function statusBadge(status: Account["status"]) {
  switch (status) {
    case "connected":
      return <Badge variant="success">connected</Badge>;
    case "paused":
      return <Badge>paused</Badge>;
    case "token_expired":
      return <Badge variant="warning">token expired</Badge>;
    case "revoked":
      return <Badge variant="danger">revoked</Badge>;
    case "error":
      return <Badge variant="danger">error</Badge>;
  }
}

function commandMessage(error: unknown): string {
  if (typeof error === "object" && error !== null) {
    const record = error as { code?: unknown; message?: unknown };
    if (typeof record.message === "string") {
      return typeof record.code === "string"
        ? `${record.code}: ${record.message}`
        : record.message;
    }
  }
  return error instanceof Error ? error.message : String(error);
}

const statusLabel: Record<Account["status"], string> = {
  connected: "connected",
  paused: "paused",
  token_expired: "token expired",
  revoked: "revoked",
  error: "error",
};

function summaryLine(accounts: Account[]): string {
  const counts = new Map<Account["status"], number>();
  for (const account of accounts) {
    counts.set(account.status, (counts.get(account.status) ?? 0) + 1);
  }
  return (Object.keys(statusLabel) as Account["status"][])
    .filter((status) => (counts.get(status) ?? 0) > 0)
    .map((status) => `${counts.get(status)} ${statusLabel[status]}`)
    .join(" · ");
}

/** Accounts — Connected accounts tab (default): the accounts
 *  list with search + filters. Adding an account lives on the
 *  Add account tab (`/accounts/add`). */
export default function Accounts() {
  const navigate = useNavigate();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [replacePat, setReplacePat] = useState<Record<string, string>>({});
  const [search, setSearch] = useState("");
  const [kindFilter, setKindFilter] = useState<
    "all" | "github_app" | "fine_grained_pat"
  >("all");
  const [statusFilter, setStatusFilter] = useState<
    "all" | "connected" | "revoked"
  >("all");

  // Pending destructive confirmation. The action only runs from the
  // dialog's OK button — never from `window.confirm`, which the
  // Tauri webview answers immediately (truthy) while its native
  // dialog is still open, so Cancel could not stop the delete.
  const [pendingConfirm, setPendingConfirm] = useState<null | {
    title: string;
    message: string;
    confirmLabel: string;
    action: () => void;
  }>(null);

  const refresh = useCallback(async () => {
    setAccounts(await listAccounts());
  }, []);

  // Reconcile: link every paired broker installation and every
  // PAT account to the web account, so the dashboard count moves
  // for accounts added before W4 (or while offline). The link is
  // idempotent — already-linked accounts return at once. A limit
  // failure is surfaced; an offline or expired session is silent
  // (the next page load retries).
  const reconcileLinks = useCallback(async () => {
    let installations: BrokerInstallation[];
    let accounts: Account[];
    try {
      [installations, accounts] = await Promise.all([
        listBrokerInstallations(),
        listAccounts(),
      ]);
    } catch {
      // Broker or database unreachable (offline) — the next load
      // retries.
      return;
    }
    const failures: string[] = [];
    let linked = 0;
    const link = async (label: string, attempt: () => Promise<unknown>) => {
      try {
        await attempt();
        linked += 1;
      } catch (e: unknown) {
        const record = e as { code?: unknown };
        const code = typeof record?.code === "string" ? record.code : "";
        if (code === "WebUnreachable" || code === "unauthorized") {
          throw e;
        }
        failures.push(`${label}: ${commandMessage(e)}`);
      }
    };
    try {
      for (const installation of installations) {
        if (!installation.pairedDevice) continue;
        await link(
          installation.accountLogin ?? installation.installationId,
          () => webLinkGithubAccount(installation.installationId),
        );
      }
      for (const account of accounts) {
        if (account.authenticationMethod !== "fine_grained_pat") continue;
        await link(account.username, () =>
          webLinkPatAccount(account.username),
        );
      }
    } catch {
      // Offline or logged out mid-reconcile — the next load
      // retries.
      return;
    }
    // A link moved the web counts — the app re-fetches package,
    // limits and usage at once instead of at the next 60s tick.
    if (linked > 0) notifyWebAccountChanged();
    if (failures.length > 0) {
      setNotice(
        `Some accounts could not be linked to the web — ${failures.join("; ")}. They still sync locally.`,
      );
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    refresh()
      .then(() => {
        if (!cancelled) void reconcileLinks();
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(commandMessage(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [refresh, reconcileLinks]);

  async function run<T>(action: () => Promise<T>, done?: (value: T) => void) {
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      const value = await action();
      await refresh();
      done?.(value);
    } catch (e: unknown) {
      setError(commandMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const filteredAccounts = useMemo(() => {
    const query = search.trim().toLowerCase();
    return accounts.filter((account) => {
      if (query && !account.username.toLowerCase().includes(query)) {
        return false;
      }
      if (
        kindFilter !== "all" &&
        account.authenticationMethod !== kindFilter
      ) {
        return false;
      }
      if (statusFilter !== "all" && account.status !== statusFilter) {
        return false;
      }
      return true;
    });
  }, [accounts, search, kindFilter, statusFilter]);

  function clearFilters() {
    setSearch("");
    setKindFilter("all");
    setStatusFilter("all");
  }

  return (
    <div className="space-y-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Accounts</h1>
        <p className="text-sm text-zinc-500">
          GitHub accounts connected by fine-grained PAT or by the GitHub
          App (via the Auth Broker).
        </p>
      </header>

      <AccountsTabs />

      {error && (
        <Card className="border-red-200 bg-red-50 p-4 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200">
          {error}
        </Card>
      )}
      {notice && (
        <Card className="border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-200">
          {notice}
        </Card>
      )}

      <section className="space-y-4">
        <h2 className="text-lg font-medium">Connected accounts</h2>
        {accounts.length > 0 && (
          <>
            <p className="text-sm text-zinc-500">{summaryLine(accounts)}</p>
            <div className="flex flex-wrap gap-3">
              <input
                className="w-64 rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                type="search"
                placeholder="Search by username"
                aria-label="Search accounts by username"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <select
                className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                aria-label="Filter by kind"
                value={kindFilter}
                onChange={(e) =>
                  setKindFilter(
                    e.target.value as "all" | "github_app" | "fine_grained_pat",
                  )
                }
              >
                <option value="all">All kinds</option>
                <option value="github_app">GitHub App</option>
                <option value="fine_grained_pat">PAT</option>
              </select>
              <select
                className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                aria-label="Filter by status"
                value={statusFilter}
                onChange={(e) =>
                  setStatusFilter(
                    e.target.value as "all" | "connected" | "revoked",
                  )
                }
              >
                <option value="all">All statuses</option>
                <option value="connected">Connected</option>
                <option value="revoked">Revoked</option>
              </select>
            </div>
          </>
        )}
        {accounts.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              {loading
                ? "Loading accounts…"
                : "No accounts yet — add your first account."}
            </p>
            {!loading && (
              <Button
                className="mt-3"
                onClick={() => navigate("/accounts/add")}
              >
                Add your first account
              </Button>
            )}
          </Card>
        )}
        {accounts.length > 0 && filteredAccounts.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              No accounts match this search.
            </p>
            <Button
              variant="secondary"
              className="mt-3"
              onClick={clearFilters}
            >
              Clear filters
            </Button>
          </Card>
        )}
        {filteredAccounts.map((account) => (
          <Card key={account.id} className="p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <span className="font-medium">{account.username}</span>
                {account.displayName && (
                  <span className="ml-2 text-sm text-zinc-500">
                    {account.displayName}
                  </span>
                )}
                <span className="ml-2 text-sm text-zinc-500">
                  {account.authenticationMethod === "github_app"
                    ? "GitHub App"
                    : "fine-grained PAT"}
                </span>
              </div>
              <div className="flex items-center gap-2">
                {statusBadge(account.status)}
                <Badge>{account.accountType}</Badge>
              </div>
            </div>

            {account.authenticationMethod === "fine_grained_pat" &&
              account.patExpiryStatus === "expiring_soon" && (
                <p className="mt-2 text-sm text-amber-800 dark:text-amber-300">
                  PAT expires in {account.patExpiryDaysLeft ?? "?"} day
                  {(account.patExpiryDaysLeft ?? 0) === 1 ? "" : "s"} — replace
                  it soon, or it will stop syncing.
                </p>
              )}
            {account.authenticationMethod === "fine_grained_pat" &&
              account.patExpiryStatus === "expired" && (
                <p className="mt-2 text-sm text-red-800 dark:text-red-300">
                  PAT expired — replace it or reconnect below.
                </p>
              )}
            {account.status === "token_expired" && (
              <p className="mt-2 text-sm text-amber-800 dark:text-amber-300">
                The stored credential was rejected by GitHub. Reconnect to
                re-validate it, or replace the PAT.
              </p>
            )}
            {account.status === "revoked" && (
              <p className="mt-2 text-sm text-red-800 dark:text-red-300">
                This account was revoked. Reconnect to re-validate the stored
                credential.
              </p>
            )}

            <div className="mt-3 flex flex-wrap gap-2">
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() =>
                  run(
                    () =>
                      account.authenticationMethod === "github_app"
                        ? validateGithubAppAccount(account.id)
                        : validatePatAccount(account.id),
                    (info) =>
                      setNotice(`Validated as ${info.username}.`),
                  )
                }
              >
                Validate
              </Button>
              {(account.status === "token_expired" ||
                account.status === "revoked" ||
                account.status === "error") && (
                <Button
                  variant="secondary"
                  disabled={busy}
                  onClick={() =>
                    run(
                      () => reconnectPatAccount(account.id),
                      () => setNotice(`Reconnected ${account.username}.`),
                    )
                  }
                >
                  Reconnect
                </Button>
              )}
              {account.authenticationMethod === "fine_grained_pat" && (
                <>
                  <input
                    className="w-64 rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
                    type="password"
                    placeholder="New PAT to replace the current one"
                    autoComplete="off"
                    value={replacePat[account.id] ?? ""}
                    onChange={(e) =>
                      setReplacePat((current) => ({
                        ...current,
                        [account.id]: e.target.value,
                      }))
                    }
                  />
                  <Button
                    variant="secondary"
                    disabled={busy || !(replacePat[account.id] ?? "").trim()}
                    onClick={() =>
                      run(
                        () =>
                          replacePatAccount({
                            accountId: account.id,
                            pat: replacePat[account.id] ?? "",
                            expiresAt: null,
                          }),
                        () => {
                          setReplacePat((current) => ({
                            ...current,
                            [account.id]: "",
                          }));
                          setNotice(`Replaced the PAT for ${account.username}.`);
                        },
                      )
                    }
                  >
                    Replace PAT
                  </Button>
                  <Button
                    variant="danger"
                    disabled={busy}
                    onClick={() =>
                      setPendingConfirm({
                        title: "Remove PAT account",
                        message: `Remove the PAT account ${account.username}? The stored token is deleted and the account is marked revoked. Local repositories are kept.`,
                        confirmLabel: "Remove",
                        action: () =>
                          void run(
                            async () => {
                              await removePatAccount(account.id);
                              // Unlink from the web (best-effort): a
                              // failure leaves the web link in
                              // place — it can be removed from the
                              // web dashboard — but must not undo
                              // the local remove.
                              try {
                                await webUnlinkPatAccount(account.username);
                                notifyWebAccountChanged();
                                return null;
                              } catch (e: unknown) {
                                return `Removed the PAT for ${account.username}, but the web link could not be removed (${commandMessage(e)}). Remove it from the web dashboard.`;
                              }
                            },
                            (note) =>
                              setNotice(
                                note ??
                                  `Removed the PAT for ${account.username}.`,
                              ),
                          ),
                      })
                    }
                  >
                    Remove
                  </Button>
                  <Button
                    variant="danger"
                    disabled={busy}
                    onClick={() =>
                      setPendingConfirm({
                        title: "Delete account",
                        message: `Delete the account ${account.username}? The stored token is deleted and the account is removed from RepoNyx, including its repositories, schedules and jobs. Local files on disk are kept.`,
                        confirmLabel: "Delete account",
                        action: () =>
                          void run(
                            async () => {
                              await deletePatAccount(account.id);
                              // Unlink from the web (best-effort):
                              // see the Remove handler above.
                              try {
                                await webUnlinkPatAccount(account.username);
                                notifyWebAccountChanged();
                                return null;
                              } catch (e: unknown) {
                                return `Deleted the account ${account.username}, but the web link could not be removed (${commandMessage(e)}). Remove it from the web dashboard.`;
                              }
                            },
                            (note) =>
                              setNotice(
                                note ??
                                  `Deleted the account ${account.username}.`,
                              ),
                          ),
                      })
                    }
                  >
                    Delete account
                  </Button>
                </>
              )}
            </div>
          </Card>
        ))}
      </section>
      <ConfirmDialog
        open={pendingConfirm !== null}
        title={pendingConfirm?.title ?? ""}
        message={pendingConfirm?.message ?? ""}
        confirmLabel={pendingConfirm?.confirmLabel ?? "OK"}
        onConfirm={() => {
          const pending = pendingConfirm;
          setPendingConfirm(null);
          pending?.action();
        }}
        onCancel={() => setPendingConfirm(null)}
      />
    </div>
  );
}
