import { adminStats } from "@reponyx/database";
import { requireAdminPage } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { getWebTranslator } from "@/lib/i18n";
import { card, link } from "@/components/ui";

/** Admin home: usage totals and links to the users + packages pages. */
export default async function AdminPage() {
  const client = getDb();
  await requireAdminPage(client);
  const stats = await adminStats(client);
  const t = await getWebTranslator("admin");
  const totals = [
    { label: t("totalUsers"), value: stats.users },
    { label: t("linkedAccounts"), value: stats.linkedAccounts },
    { label: t("devices"), value: stats.activeDevices },
    { label: t("twoFactorUsers"), value: stats.twoFactorUsers },
  ];
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <h1 className="text-2xl font-semibold tracking-tight">{t("title")}</h1>
      <p className="mt-1 text-muted-foreground">{t("subtitle")}</p>
      <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {totals.map((total) => (
          <section key={total.label} className={`${card} p-4`}>
            <p className="text-sm text-muted-foreground">{total.label}</p>
            <p className="mt-1 text-2xl font-semibold tracking-tight">
              {total.value}
            </p>
          </section>
        ))}
      </div>
      <div className="mt-6 flex gap-6">
        <a href="/admin/users" className={link}>
          {t("users")}
        </a>
        <a href="/admin/packages" className={link}>
          {t("packages")}
        </a>
        <a href="/admin/billing" className={link}>
          {t("billing")}
        </a>
      </div>
    </main>
  );
}
