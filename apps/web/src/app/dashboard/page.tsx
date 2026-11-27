import { redirect } from "next/navigation";
import { formatDate } from "@reponyx/i18n";
import {
  countDevicesForUser,
  countLinkedAccountsForUser,
  findActiveSubscriptionForUser,
  findPackageByName,
  listDevicesForUser,
  listLinkedAccountsForUser,
  listLinkedProvidersForUser,
} from "@reponyx/database";
import { getSessionFromCookies } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { LinkedAccounts } from "@/components/auth/LinkedAccounts";
import { ProfileNameForm } from "@/components/auth/ProfileNameForm";
import { alertError, alertSuccess, card } from "@/components/ui";

const LINK_ERROR_KEYS: Record<string, string> = {
  email_mismatch: "emailMismatch",
  already_linked: "alreadyLinked",
  oauth_not_configured: "notConfigured",
};

export default async function DashboardPage(props: {
  searchParams: Promise<{ linked?: string; link_error?: string }>;
}) {
  const { linked: linkedParam, link_error: linkError } =
    await props.searchParams;
  const client = getDb();
  const session = await getSessionFromCookies(client);
  if (!session) redirect("/login");
  const { user } = session;
  const subscription = await findActiveSubscriptionForUser(client, user.id);
  const pkg =
    subscription?.package ?? (await findPackageByName(client, "Free"));
  const [linkedCount, deviceCount, devices, accounts, providers] =
    await Promise.all([
      countLinkedAccountsForUser(client, user.id),
      countDevicesForUser(client, user.id),
      listDevicesForUser(client, user.id),
      listLinkedAccountsForUser(client, user.id),
      listLinkedProvidersForUser(client, user.id),
    ]);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("dashboard");
  const tCommon = await getWebTranslator("common");
  const errors = await getErrorStrings();
  const linkErrorMessage =
    linkError && LINK_ERROR_KEYS[linkError]
      ? errors[LINK_ERROR_KEYS[linkError]]
      : null;
  const linkedSuccess =
    linkedParam === "google" || linkedParam === "github"
      ? t("linkedSuccess")
      : null;
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
        {t("welcome", { name: user.name ?? user.email })}
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
      {linkedSuccess ? (
        <p className={`${alertSuccess} mt-6`}>{linkedSuccess}</p>
      ) : null}
      {linkErrorMessage ? (
        <p role="alert" className={`${alertError} mt-6`}>
          {linkErrorMessage}
        </p>
      ) : null}
      <div className="mt-6 grid gap-4 lg:grid-cols-2">
        <section className={`${card} p-4`}>
          <h2 className="text-lg font-semibold tracking-tight">
            {t("profile")}
          </h2>
          <p className="mt-1 text-sm text-muted-foreground">{user.email}</p>
          <ProfileNameForm
            initialName={user.name}
            strings={{
              fullName: t("fullName"),
              save: tCommon("save"),
              nameUpdated: t("nameUpdated"),
            }}
            errors={errors}
            locale={locale}
          />
        </section>
        <section className={`${card} p-4`}>
          <h2 className="text-lg font-semibold tracking-tight">
            {t("signInMethods")}
          </h2>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("signInMethodsSubtitle")}
          </p>
          <LinkedAccounts
            initial={providers.map((row) => ({
              provider: row.provider,
              email: row.email,
              createdAt: row.created_at,
            }))}
            hasPassword={user.has_password}
            strings={{
              linked: t("linked"),
              notLinked: t("notLinked"),
              link: t("link"),
              unlink: t("unlink"),
              unlinkedSuccess: t("unlinkedSuccess"),
              noPasswordHint: t("noPasswordHint"),
              setPassword: t("setPassword"),
            }}
            errors={errors}
            locale={locale}
          />
        </section>
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
