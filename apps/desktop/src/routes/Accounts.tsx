import { useCallback, useEffect, useState } from "react";
import {
  addPatAccount,
  completePairing,
  getBrokerStatus,
  listAccounts,
  listBrokerInstallations,
  reconnectPatAccount,
  removePatAccount,
  replacePatAccount,
  revokeDevicePairing,
  setBrokerSetupToken,
  startPairing,
  validateGithubAppAccount,
  validatePatAccount,
} from "@reponyx/api-client";
import type {
  Account,
  BrokerInstallation,
  BrokerStatus,
  PairingInfo,
} from "@reponyx/shared";
import { Badge, Button, Card } from "@reponyx/ui";

const inputClass =
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900";

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

/** Accounts: PAT add/replace/remove/reconnect and GitHub App
 *  install + pairing via the Auth Broker. Phase 5 flows, usable but
 *  unpolished — Phase 8 owns the settings UI. */
export default function Accounts() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [broker, setBroker] = useState<BrokerStatus | null>(null);
  const [installations, setInstallations] = useState<BrokerInstallation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [pat, setPat] = useState("");
  const [patLabel, setPatLabel] = useState("");
  const [patAccountType, setPatAccountType] = useState<"personal" | "organization">("personal");
  const [patExpires, setPatExpires] = useState("");
  const [replacePat, setReplacePat] = useState<Record<string, string>>({});
  const [setupToken, setSetupToken] = useState("");
  const [pairing, setPairing] = useState<PairingInfo | null>(null);

  const refresh = useCallback(async () => {
    const [loadedAccounts, brokerStatus] = await Promise.all([
      listAccounts(),
      getBrokerStatus(),
    ]);
    setAccounts(loadedAccounts);
    setBroker(brokerStatus);
    if (brokerStatus.configured) {
      setInstallations(await listBrokerInstallations());
    } else {
      setInstallations([]);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    refresh().catch((e: unknown) => {
      if (!cancelled) setError(commandMessage(e));
    });
    return () => {
      cancelled = true;
    };
  }, [refresh]);

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

  function expiryDateToRfc3339(date: string): string | null {
    if (!date) return null;
    return `${date}T00:00:00Z`;
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
        {accounts.length === 0 && (
          <Card className="p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              No accounts yet. Add a PAT below, or pair the GitHub App.
            </p>
          </Card>
        )}
        {accounts.map((account) => (
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
                            account_id: account.id,
                            pat: replacePat[account.id] ?? "",
                            expires_at: null,
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
                    onClick={() => {
                      if (
                        window.confirm(
                          `Remove the PAT account ${account.username}? The stored token is deleted and the account is marked revoked. Local repositories are kept.`,
                        )
                      ) {
                        void run(
                          () => removePatAccount(account.id),
                          () =>
                            setNotice(
                              `Removed the PAT for ${account.username}.`,
                            ),
                        );
                      }
                    }}
                  >
                    Remove
                  </Button>
                </>
              )}
            </div>
          </Card>
        ))}
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-medium">Add a fine-grained PAT</h2>
        <Card className="space-y-3 p-4">
          <input
            className={inputClass}
            type="password"
            placeholder="github_pat_… or ghp_…"
            autoComplete="off"
            value={pat}
            onChange={(e) => setPat(e.target.value)}
          />
          <div className="flex gap-3">
            <input
              className={inputClass}
              type="text"
              placeholder="Label (optional)"
              value={patLabel}
              onChange={(e) => setPatLabel(e.target.value)}
            />
            <select
              className={inputClass}
              value={patAccountType}
              onChange={(e) =>
                setPatAccountType(
                  e.target.value as "personal" | "organization",
                )
              }
            >
              <option value="personal">Personal</option>
              <option value="organization">Organization</option>
            </select>
            <input
              className={inputClass}
              type="date"
              title="PAT expiry (optional — used for warnings)"
              value={patExpires}
              onChange={(e) => setPatExpires(e.target.value)}
            />
          </div>
          <p className="text-xs text-zinc-500">
            The PAT is validated against GitHub, then stored only in the OS
            credential store — never in the database or logs.
          </p>
          <Button
            disabled={busy || !pat.trim()}
            onClick={() =>
              run(
                () =>
                  addPatAccount({
                    pat,
                    label: patLabel || null,
                    account_type: patAccountType,
                    expires_at: expiryDateToRfc3339(patExpires),
                  }),
                (account) => {
                  setPat("");
                  setPatLabel("");
                  setPatExpires("");
                  setNotice(`Added PAT account ${account.username}.`);
                },
              )
            }
          >
            Add PAT account
          </Button>
        </Card>
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-medium">GitHub App (Auth Broker)</h2>
        <Card className="space-y-3 p-4">
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            {broker == null
              ? "Checking the Auth Broker…"
              : broker.configured
                ? `Broker at ${broker.baseUrl} is ${broker.reachable ? "reachable" : "not reachable"}; setup token is configured.`
                : `Broker at ${broker.baseUrl} — enter the setup token supplied when the broker was started.`}
          </p>
          <div className="flex gap-3">
            <input
              className={inputClass}
              type="password"
              placeholder="Broker setup token"
              autoComplete="off"
              value={setupToken}
              onChange={(e) => setSetupToken(e.target.value)}
            />
            <Button
              variant="secondary"
              disabled={busy || setupToken.trim().length < 16}
              onClick={() =>
                run(
                  () => setBrokerSetupToken(setupToken),
                  () => {
                    setSetupToken("");
                    setNotice("Broker setup token saved in the OS credential store.");
                  },
                )
              }
            >
              Save setup token
            </Button>
          </div>

          {installations.length > 0 && (
            <div className="space-y-2">
              {installations.map((installation) => (
                <div
                  key={installation.installationId}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-zinc-200 p-3 dark:border-zinc-800"
                >
                  <span className="text-sm">
                    Installation {installation.installationId}
                    {installation.accountLogin
                      ? ` (${installation.accountLogin})`
                      : ""}
                    {installation.pairedDevice
                      ? ` — paired to ${installation.pairedDevice}`
                      : ""}
                  </span>
                  <Button
                    variant="secondary"
                    disabled={busy}
                    onClick={() =>
                      run(
                        () => startPairing(installation.installationId),
                        (info) => setPairing(info),
                      )
                    }
                  >
                    Pair this device
                  </Button>
                </div>
              ))}
            </div>
          )}

          {pairing && (
            <Card className="space-y-2 border-amber-200 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950">
              <p className="text-sm">
                Pairing code for installation {pairing.installationId}:
              </p>
              <p className="text-2xl font-mono font-semibold tracking-widest">
                {pairing.pairingCode}
              </p>
              <p className="text-xs text-zinc-500">
                Expires {new Date(pairing.expiresAt).toLocaleString()}. The
                code is single-use and bound to this device.
              </p>
              <div className="flex gap-2">
                <Button
                  disabled={busy}
                  onClick={() =>
                    run(
                      () => completePairing(pairing.pairingCode),
                      (account) => {
                        setPairing(null);
                        setNotice(
                          `Paired and added GitHub App account ${account.username}.`,
                        );
                      },
                    )
                  }
                >
                  Complete pairing
                </Button>
                <Button
                  variant="ghost"
                  disabled={busy}
                  onClick={() =>
                    run(
                      () => revokeDevicePairing(pairing.deviceIdentifier),
                      () => {
                        setPairing(null);
                        setNotice("Pairing revoked for this device.");
                      },
                    )
                  }
                >
                  Revoke
                </Button>
              </div>
            </Card>
          )}
        </Card>
      </section>
    </div>
  );
}
