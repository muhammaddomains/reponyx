import { redirect } from "next/navigation";
import { formatDate } from "@reponyx/i18n";
import {
  countDevicesForUser,
  countLinkedAccountsForUser,
  findActiveSubscriptionForUser,
  findPackageByName,
  listDevicesForUser,
  listLinkedAccountsForUser,
} from "@reponyx/database";
import { getSessionFromCookies } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { getRequestLocale, getWebTranslator } from "@/lib/i18n";

export default async function DashboardPage() {
  const client = getDb();
  const session = await getSessionFromCookies(client);
  if (!session) redirect("/auth/login");
  const { user } = session;
  const subscription = await findActiveSubscriptionForUser(client, user.id);
  const pkg = subscription?.package ?? (await findPackageByName(client, "Free"));
  const [linkedCount, deviceCount, devices, accounts] = await Promise.all([
    countLinkedAccountsForUser(client, user.id),
    countDevicesForUser(client, user.id),
    listDevicesForUser(client, user.id),
    listLinkedAccountsForUser(client, user.id),
  ]);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("dashboard");
  return (
    <main>
      <h1>{t("title")}</h1>
      <p>{t("welcome", { name: user.email })}</p>
      <section>
        <h2>
          {t("package")}: {pkg?.name ?? "—"}
        </h2>
        <p>
          {t("linkedAccounts")}: {linkedCount} / {pkg?.max_linked_accounts ?? "—"}
        </p>
        <p>
          {t("devices")}: {deviceCount} / {pkg?.max_devices ?? "—"}
        </p>
        <p>
          {t("expires")}:{" "}
          {subscription?.expires_at
            ? formatDate(locale, new Date(subscription.expires_at))
            : t("noExpiry")}
        </p>
        <p>
          {t("joined")}: {formatDate(locale, new Date(user.created_at))}
        </p>
      </section>
      <section>
        <h2>{t("devices")}</h2>
        {devices.length === 0 ? (
          <p>{t("noDevices")}</p>
        ) : (
          <ul>
            {devices.map((device) => (
              <li key={device.id}>
                {device.machine_name ?? device.device_key} — {t("lastSeen")}:{" "}
                {formatDate(locale, new Date(device.last_seen_at))}
                {device.revoked_at ? ` (${t("revoked")})` : ""}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section>
        <h2>{t("linkedAccounts")}</h2>
        {accounts.length === 0 ? (
          <p>—</p>
        ) : (
          <ul>
            {accounts.map((account) => (
              <li key={account.id}>
                {account.login} ({account.auth_kind})
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}
