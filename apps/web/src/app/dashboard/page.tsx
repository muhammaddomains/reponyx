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
import { card } from "@/components/ui";

export default async function DashboardPage() {
  const client = getDb();
  const session = await getSessionFromCookies(client);
  if (!session) redirect("/auth/login");
  const { user } = session;
  const subscription = await findActiveSubscriptionForUser(client, user.id);
  const pkg =
    subscription?.package ?? (await findPackageByName(client, "Free"));
  const [linkedCount, deviceCount, devices, accounts] = await Promise.all([
    countLinkedAccountsForUser(client, user.id),
    countDevicesForUser(client, user.id),
    listDevicesForUser(client, user.id),
    listLinkedAccountsForUser(client, user.id),
  ]);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("dashboard");
  const stats = [
    {
      label: t("package"),
      value: pkg?.name ?? "—",
    },
    {
      label: t("linkedAccounts"),
      value: `${linkedCount} / ${pkg?.max_linked_accounts ?? "—"}`,
    },
    {
      label: t("devices"),
      value: `${deviceCount} / ${pkg?.max_devices ?? "—"}`,
    },
    {
      label: t("expires"),
      value: subscription?.expires_at
        ? formatDate(locale, new Date(subscription.expires_at))
        : t("noExpiry"),
    },
  ];
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <h1 className="text-2xl font-semibold tracking-tight">{t("title")}</h1>
      <p className="mt-1 text-muted-foreground">
        {t("welcome", { name: user.email })}
      </p>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("joined")}: {formatDate(locale, new Date(user.created_at))}
      </p>
      <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => (
          <section key={stat.label} className={`${card} p-4`}>
            <p className="text-sm text-muted-foreground">{stat.label}</p>
            <p className="mt-1 text-2xl font-semibold tracking-tight">
              {stat.value}
            </p>
          </section>
        ))}
      </div>
      <div className="mt-6 grid gap-4 lg:grid-cols-2">
        <section className={`${card} p-4`}>
          <h2 className="text-lg font-semibold tracking-tight">
            {t("devices")}
          </h2>
          {devices.length === 0 ? (
            <p className="mt-2 text-sm text-muted-foreground">
              {t("noDevices")}
            </p>
          ) : (
            <ul className="mt-2 divide-y divide-border">
              {devices.map((device) => (
                <li key={device.id} className="py-2 text-sm">
                  {device.machine_name ?? device.device_key} — {t("lastSeen")}:{" "}
                  {formatDate(locale, new Date(device.last_seen_at))}
                  {device.revoked_at ? ` (${t("revoked")})` : ""}
                </li>
              ))}
            </ul>
          )}
        </section>
        <section className={`${card} p-4`}>
          <h2 className="text-lg font-semibold tracking-tight">
            {t("linkedAccounts")}
          </h2>
          {accounts.length === 0 ? (
            <p className="mt-2 text-sm text-muted-foreground">—</p>
          ) : (
            <ul className="mt-2 divide-y divide-border">
              {accounts.map((account) => (
                <li key={account.id} className="py-2 text-sm">
                  {account.login} ({account.auth_kind})
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </main>
  );
}
