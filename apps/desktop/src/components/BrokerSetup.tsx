import { useCallback, useEffect, useRef, useState } from "react";
import {
  completePairing,
  createBrokerSetupState,
  getBrokerStatus,
  getSettings,
  listBrokerInstallations,
  pickBrokerKeyFile,
  revokeDevicePairing,
  setSetting,
  startPairing,
} from "@reponyx/api-client";
import type {
  BrokerInstallation,
  BrokerStatus,
  PairingInfo,
} from "@reponyx/shared";
import { Button, Card } from "@reponyx/ui";

const inputClass =
  "w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm dark:border-zinc-700 dark:bg-zinc-900";

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

/** The GitHub App (Auth Broker) card on the Add account tab:
 *  broker status, the one-time in-app setup form, Install +
 *  Refresh, the installations list, and the "Pair this device"
 *  flow. Self-contained: it loads the broker state itself and
 *  shows its own errors and notices. */
export default function BrokerSetup() {
  const [broker, setBroker] = useState<BrokerStatus | null>(null);
  const [brokerError, setBrokerError] = useState<string | null>(null);
  const [installations, setInstallations] = useState<BrokerInstallation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pairing, setPairing] = useState<PairingInfo | null>(null);

  // In-app broker setup: the GitHub App id and the private-key file
  // path, saved as settings so the desktop can start the broker it
  // spawns — no hand-made `.env` file. Loaded once at mount, not on
  // every refresh: a refresh after "Choose file…" must not wipe a
  // value the user typed but has not saved yet.
  const [brokerAppId, setBrokerAppId] = useState("");
  const [brokerKeyPath, setBrokerKeyPath] = useState("");

  // The setup form shows only when the broker has no configuration
  // at all: no `.env` file supplies it (dev, or a hand-made file),
  // and the App id or the key path is missing. The setup token is
  // never part of it — it resolves itself (.env file, else the OS
  // store, else generated per install), so there is no token
  // field anywhere on this page.
  const brokerNeedsSetup =
    broker !== null &&
    !broker.envFileConfigured &&
    (!broker.appIdConfigured || !broker.privateKeyConfigured);

  // Broker status and installations load independently of the
  // accounts list (which lives on the Connected tab): in the web
  // preview (plain browser, no Tauri backend) the status call
  // fails, but it must resolve the "Checking the Auth Broker…"
  // line to a real error instead of spinning forever. The
  // installations call is a real HTTP request, so it can reject
  // while the status already loaded — it is guarded below.
  const refresh = useCallback(async () => {
    const brokerResult = await getBrokerStatus();
    setBroker(brokerResult);
    setBrokerError(null);
    try {
      setInstallations(
        brokerResult.configured ? await listBrokerInstallations() : [],
      );
    } catch (e: unknown) {
      setBrokerError(commandMessage(e));
      setInstallations([]);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    refresh().catch((e: unknown) => {
      if (!cancelled) setBrokerError(commandMessage(e));
    });
    getSettings()
      .then((settings) => {
        if (cancelled) return;
        setBrokerAppId(settings.brokerAppId ?? "");
        setBrokerKeyPath(settings.brokerPrivateKeyPath ?? "");
      })
      .catch(() => {
        // Settings unreadable — leave the fields empty; the user
        // can still type the values and save them.
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

  // After the Install click, the installation only exists once the
  // user completes the GitHub flow in the browser and the broker
  // callback records it. Poll the list until a new installation
  // appears (or 90 s pass): the user should not have to press
  // Refresh to see the account they just installed. Only the
  // installation from this flow can appear — listing records
  // nothing on its own (see the broker's list_installations), so
  // no other account is pulled in.
  const installPollRef = useRef<number | null>(null);
  const stopInstallPoll = useCallback(() => {
    if (installPollRef.current !== null) {
      window.clearInterval(installPollRef.current);
      installPollRef.current = null;
    }
  }, []);
  useEffect(() => stopInstallPoll, [stopInstallPoll]);

  const startInstallPoll = useCallback(() => {
    stopInstallPoll();
    const known = new Set(installations.map((i) => i.installationId));
    let ticks = 0;
    installPollRef.current = window.setInterval(() => {
      ticks += 1;
      if (ticks > 45) {
        stopInstallPoll();
        setNotice(
          "Still waiting for the installation — if you completed it in the browser, press Refresh below.",
        );
        return;
      }
      listBrokerInstallations()
        .then((list) => {
          const found = list.find((item) => !known.has(item.installationId));
          if (!found) return;
          stopInstallPoll();
          setInstallations(list);
          setNotice(
            `Installation ${found.installationId}${found.accountLogin ? ` (${found.accountLogin})` : ""} is here — pair this device to add the account.`,
          );
        })
        .catch(() => {
          // Broker briefly unreachable — the next tick retries.
        });
    }, 2000);
  }, [installations, stopInstallPoll]);

  return (
    <Card className="space-y-3 p-4">
      {error && (
        <p className="text-sm text-red-800 dark:text-red-300">{error}</p>
      )}
      {notice && (
        <p className="text-sm text-emerald-800 dark:text-emerald-300">
          {notice}
        </p>
      )}
      <p className="text-sm text-zinc-600 dark:text-zinc-400">
        {broker == null
          ? (brokerError ?? "Checking the Auth Broker…")
          : broker.configured
            ? `Broker at ${broker.baseUrl} is ${broker.reachable ? "reachable" : "not reachable"}.`
            : `Broker at ${broker.baseUrl} — set up the Auth Broker below to connect GitHub accounts.`}
      </p>
      {brokerNeedsSetup && (
        <Card className="space-y-3 border-amber-200 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950">
          <p className="text-sm">
            Set up the Auth Broker once — no .env file needed. The App
            id and the private-key file are saved as settings; the setup
            token is created automatically on this device and never
            shown. The key itself never leaves its file.
          </p>
          <input
            className={inputClass}
            type="text"
            inputMode="numeric"
            placeholder="GitHub App id, e.g. 5896712"
            autoComplete="off"
            value={brokerAppId}
            onChange={(e) => setBrokerAppId(e.target.value)}
          />
          <div className="flex gap-3">
            <input
              className={inputClass}
              type="text"
              readOnly
              placeholder="No private-key file chosen"
              aria-label="Private-key file"
              value={brokerKeyPath}
            />
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() =>
                run(
                  () => pickBrokerKeyFile(),
                  (path) => {
                    if (path) setBrokerKeyPath(path);
                  },
                )
              }
            >
              Choose file…
            </Button>
          </div>
          <Button
            disabled={busy || !brokerAppId.trim() || !brokerKeyPath.trim()}
            onClick={() =>
              run(
                async () => {
                  await setSetting("broker.app_id", brokerAppId.trim());
                  await setSetting(
                    "broker.private_key_path",
                    brokerKeyPath.trim(),
                  );
                },
                () => {
                  setNotice(
                    "Broker setup saved — the broker starts with these settings.",
                  );
                },
              )
            }
          >
            Save broker setup
          </Button>
        </Card>
      )}

      <div className="flex flex-wrap gap-3">
        <Button
          variant="secondary"
          disabled={busy || broker?.configured !== true}
          onClick={() =>
            run(
              () => createBrokerSetupState(),
              (state) => {
                setNotice(
                  `Install page opened in your browser — complete the install there within 10 minutes (before ${new Date(state.expiresAt).toLocaleString()}). This page watches for the installation you choose and shows it here when it arrives — no need to press Refresh. Only the installation you pick comes in, whether it is new or already installed on that account.`,
                );
                startInstallPoll();
              },
            )
          }
        >
          Install GitHub App
        </Button>
        <Button
          variant="secondary"
          disabled={busy}
          onClick={() =>
            run(
              async () => undefined,
              () => setNotice("Accounts and installations refreshed."),
            )
          }
        >
          Refresh
        </Button>
      </div>

      {broker?.configured === true && installations.length === 0 && (
        <p className="text-sm text-zinc-600 dark:text-zinc-400">
          No installations yet. Install the GitHub App to pair this
          device.
        </p>
      )}

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
            {pairing.expiresAt
              ? `Expires ${new Date(pairing.expiresAt).toLocaleString()}. The code is single-use and bound to this device.`
              : "The code is single-use and bound to this device, and it does not expire."}
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
  );
}
