import { useState } from "react";
import { addPatAccount, webLinkPatAccount } from "@reponyx/api-client";
import { Button, Card } from "@reponyx/ui";
import AccountsTabs from "../components/AccountsTabs";
import BrokerSetup from "../components/BrokerSetup";
import { notifyWebAccountChanged } from "../webAccountSync";

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

/** Add account tab: a fine-grained PAT, and the GitHub App via
 *  the Auth Broker. Two methods of doing one thing — adding an
 *  account — so they sit as two stacked cards, not sub-tabs. */
export default function AddAccount() {
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [pat, setPat] = useState("");
  const [patLabel, setPatLabel] = useState("");
  const [patAccountType, setPatAccountType] = useState<
    "personal" | "organization"
  >("personal");
  const [patExpires, setPatExpires] = useState("");

  async function run<T>(
    action: () => Promise<T>,
    done?: (value: T) => void | Promise<void>,
  ) {
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      const value = await action();
      await done?.(value);
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
                    accountType: patAccountType,
                    expiresAt: expiryDateToRfc3339(patExpires),
                  }),
                async (account) => {
                  setPat("");
                  setPatLabel("");
                  setPatExpires("");
                  // Link the PAT login to the web account right
                  // away, so the dashboard count moves. A link
                  // failure does not undo the add — the account
                  // works locally, and the Accounts page
                  // reconcile retries the link on its next load.
                  try {
                    await webLinkPatAccount(account.username);
                    notifyWebAccountChanged();
                    setNotice(
                      `Added PAT account ${account.username} — linked to your web account.`,
                    );
                  } catch (e: unknown) {
                    setNotice(
                      `Added PAT account ${account.username}. Web link failed (${commandMessage(e)}) — it will be retried when the Accounts page loads.`,
                    );
                  }
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
        <BrokerSetup />
      </section>
    </div>
  );
}
