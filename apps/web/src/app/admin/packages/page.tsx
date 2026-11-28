import { listAllPackages } from "@reponyx/database";
import { requireAdminPage } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { PackageManager } from "@/components/auth/PackageManager";

/** All packages (including retired ones) with create/edit forms. */
export default async function AdminPackagesPage() {
  const client = getDb();
  await requireAdminPage(client);
  const packages = await listAllPackages(client);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("admin");
  const tCommon = await getWebTranslator("common");
  const errors = await getErrorStrings();
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <h1 className="text-2xl font-semibold tracking-tight">{t("packages")}</h1>
      <p className="mt-1 text-muted-foreground">{t("subtitle")}</p>
      <PackageManager
        initial={packages.map((pkg) => ({
          id: pkg.id,
          name: pkg.name,
          maxLinkedAccounts: pkg.max_linked_accounts,
          maxDevices: pkg.max_devices,
          priceCents: pkg.price_cents,
          isActive: pkg.is_active,
        }))}
        strings={{
          packageName: t("packageName"),
          maxLinkedAccounts: t("maxLinkedAccounts"),
          maxDevices: t("maxDevices"),
          priceCents: t("priceCents"),
          active: t("active"),
          createPackage: t("createPackage"),
          packageCreated: t("packageCreated"),
          packageUpdated: t("packageUpdated"),
          retired: t("retired"),
          edit: t("edit"),
          save: tCommon("save"),
        }}
        errors={errors}
        locale={locale}
      />
    </main>
  );
}
