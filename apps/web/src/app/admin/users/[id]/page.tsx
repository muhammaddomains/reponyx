import { notFound } from "next/navigation";
import { formatDate } from "@reponyx/i18n";
import {
  findActiveSubscriptionForUser,
  findUserById,
  listAllPackages,
  listDevicesForUser,
  listLinkedAccountsForUser,
} from "@reponyx/database";
import { requireAdminPage } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AssignPackageForm } from "@/components/auth/AssignPackageForm";
import { card, link } from "@/components/ui";

/** One user for the admin: profile, package, devices, linked accounts. */
export default async function AdminUserPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const client = getDb();
  await requireAdminPage(client);
  const user = await findUserById(client, id);
  if (!user) notFound();
  const [subscription, devices, accounts, packages] = await Promise.all([
    findActiveSubscriptionForUser(client, user.id),
    listDevicesForUser(client, user.id),
    listLinkedAccountsForUser(client, user.id),
    listAllPackages(client),
  ]);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("admin");
  const errors = await getErrorStrings();
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <a href="/admin/users" className={link}>
        ← {t("backToUsers")}
      </a>
      <h1 className="mt-4 text-2xl font-semibold tracking-tight">
        {user.name ?? user.email}
      </h1>
      <p className="mt-1 text-muted-foreground">{user.email}</p>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("joined")}: {formatDate(locale, new Date(user.created_at))}
      </p>
      <div className="mt-6 grid gap-4 lg:grid-cols-2">
        <section className={`${card} p-4`}>
          <h2 className="text-lg font-semibold tracking-tight">
            {t("package")}
          </h2>
          <p className="mt-2 text-sm">
            {subscription?.package.name ?? "—"}
            {subscription?.expires_at
              ? ` — ${t("expiresAt")}: ${formatDate(locale, new Date(subscription.expires_at))}`
              : ` — ${t("noExpiry")}`}
          </p>
          <AssignPackageForm
            userId={user.id}
            packages={packages.map((pkg) => ({
              id: pkg.id,
              name: pkg.name,
              maxLinkedAccounts: pkg.max_linked_accounts,
              maxDevices: pkg.max_devices,
            }))}
            currentPackageId={subscription?.package_id ?? null}
            strings={{
              package: t("package"),
              expiresAt: t("expiresAt"),
              assign: t("assign"),
              packageAssigned: t("packageAssigned"),
            }}
            errors={errors}
            locale={locale}
          />
        </section>
        <section className={`${card} p-4`}>
          <h2 className="text-lg font-semibold tracking-tight">
            {t("devices")}
          </h2>
          {devices.length === 0 ? (
            <p className="mt-2 text-sm text-muted-foreground">—</p>
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
          <h2 className="mt-6 text-lg font-semibold tracking-tight">
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
