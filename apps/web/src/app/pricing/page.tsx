import { headers } from "next/headers";
import {
  availableProvidersForCountry,
  buyerCountry,
  isProviderConfigured,
} from "@reponyx/billing";
import {
  findActiveSubscriptionForUser,
  listBillingProviders,
  listPackages,
} from "@reponyx/database";
import { getSessionFromCookies } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { PricingPlans } from "@/components/auth/PricingPlans";

/** Public pricing page (W6): packages, prices and subscribe buttons. */
export default async function PricingPage() {
  const client = getDb();
  const session = await getSessionFromCookies(client);
  const packages = await listPackages(client);
  const providers = await listBillingProviders(client);
  const country = buyerCountry(await headers());
  const available = availableProvidersForCountry(
    providers,
    (key) => isProviderConfigured(key),
    country
  );
  const active = session
    ? await findActiveSubscriptionForUser(client, session.user.id)
    : null;
  const locale = await getRequestLocale();
  const t = await getWebTranslator("billing");
  const errors = await getErrorStrings();
  return (
    <main id="main-content" className="mx-auto w-full max-w-5xl px-6 py-8">
      <h1 className="text-2xl font-semibold tracking-tight">{t("title")}</h1>
      <p className="mt-1 text-muted-foreground">{t("subtitle")}</p>
      <PricingPlans
        packages={packages.map((pkg) => ({
          id: pkg.id,
          name: pkg.name,
          maxLinkedAccounts: pkg.max_linked_accounts,
          maxDevices: pkg.max_devices,
          priceCents: pkg.price_cents,
        }))}
        providers={available}
        currentPackageId={active?.package_id ?? null}
        signedIn={session !== null}
        strings={{
          subscribe: t("subscribe"),
          loginToSubscribe: t("loginToSubscribe"),
          perMonth: t("perMonth"),
          free: t("free"),
          linkedAccounts: t("linkedAccounts"),
          devices: t("devices"),
          currentPlan: t("currentPlan"),
          chooseProvider: t("chooseProvider"),
          checkoutRedirect: t("checkoutRedirect"),
        }}
        errors={errors}
        locale={locale}
      />
    </main>
  );
}
