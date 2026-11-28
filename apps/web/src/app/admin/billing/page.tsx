import { isProviderConfigured } from "@reponyx/billing";
import {
  listAllPackages,
  listBillingProviders,
  listPackageProviderPrices,
} from "@reponyx/database";
import { requireAdminPage } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { BillingManager } from "@/components/auth/BillingManager";

/** Admin billing: provider toggles + country scopes + price ids (W6). */
export default async function AdminBillingPage() {
  const client = getDb();
  await requireAdminPage(client);
  const providers = await listBillingProviders(client);
  const packages = await listAllPackages(client);
  const prices = await listPackageProviderPrices(client);
  const locale = await getRequestLocale();
  const t = await getWebTranslator("billing");
  const tCommon = await getWebTranslator("common");
  const errors = await getErrorStrings();
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <h1 className="text-2xl font-semibold tracking-tight">
        {t("adminTitle")}
      </h1>
      <p className="mt-1 text-muted-foreground">{t("adminSubtitle")}</p>
      <BillingManager
        initialProviders={providers.map((row) => ({
          provider: row.provider,
          isActive: row.is_active,
          countries: row.countries,
          configured: isProviderConfigured(
            row.provider as
              "stripe" | "paypal" | "polar" | "freemius" | "gumroad"
          ),
        }))}
        packages={packages.map((pkg) => ({
          id: pkg.id,
          name: pkg.name,
          priceCents: pkg.price_cents,
        }))}
        initialPrices={prices.map((price) => ({
          package_id: price.package_id,
          provider: price.provider,
          provider_price_id: price.provider_price_id,
        }))}
        strings={{
          provider: t("provider"),
          configured: t("configured"),
          notConfigured: t("notConfigured"),
          active: t("active"),
          countries: t("countries"),
          worldwide: t("worldwide"),
          priceId: t("priceId"),
          setPrice: t("setPrice"),
          priceSaved: t("priceSaved"),
          providerUpdated: t("providerUpdated"),
          noPrice: t("noPrice"),
          save: tCommon("save"),
        }}
        errors={errors}
        locale={locale}
      />
    </main>
  );
}
