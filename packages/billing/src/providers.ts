/**
 * Billing providers (plan §9). A provider is a payment service the web
 * can sell a package through. Nothing is hardcoded: the admin toggles
 * providers and their country scopes in the database, and a provider
 * can only sell when its env credentials exist (checked here) and the
 * admin has enabled it. Gumroad is registered only — it has no API to
 * create a payment or a subscription, so it never appears at checkout.
 */

export type BillingProviderKey =
  "stripe" | "paypal" | "polar" | "freemius" | "gumroad";

/** Every provider the web knows about, in admin-UI order. */
export const BILLING_PROVIDERS: readonly BillingProviderKey[] = [
  "stripe",
  "paypal",
  "polar",
  "freemius",
  "gumroad",
] as const;

export const BILLING_PROVIDER_NAMES: Record<BillingProviderKey, string> = {
  stripe: "Stripe",
  paypal: "PayPal",
  polar: "Polar",
  freemius: "Freemius",
  gumroad: "Gumroad",
};

/** Providers that can sell through checkout. Gumroad never can. */
export function supportsCheckout(key: BillingProviderKey): boolean {
  return key !== "gumroad";
}

export interface CheckoutSessionInput {
  userId: string;
  userEmail: string;
  packageId: string;
  packageName: string;
  providerPriceId: string;
  successUrl: string;
  cancelUrl: string;
  customerId?: string | null;
}

export interface CheckoutSessionResult {
  url: string;
}

export type NormalizedBillingEventType =
  "checkout_completed" | "subscription_updated" | "subscription_deleted";

/**
 * A provider webhook event, normalized. `userId` and `packageId` come
 * from the metadata we set when the checkout session was created, so
 * every event carries the account it belongs to without an extra API
 * call. `providerStatus` is the provider's own status word; the local
 * status + expiry are decided by resolveSubscriptionState.
 */
export interface NormalizedBillingEvent {
  type: NormalizedBillingEventType;
  userId: string | null;
  packageId: string | null;
  providerCustomerId: string | null;
  providerSubscriptionId: string | null;
  providerPriceId: string | null;
  providerStatus: string;
  /** True when the subscription was cancelled but runs to period end. */
  cancelAtPeriodEnd: boolean;
  /** ISO timestamp of the current period end, or null. */
  currentPeriodEnd: string | null;
}

export interface BillingProvider {
  key: BillingProviderKey;
  /** True when the provider's env credentials exist. */
  isConfigured(): boolean;
  createCheckoutSession(
    input: CheckoutSessionInput
  ): Promise<CheckoutSessionResult>;
  /**
   * Verifies the webhook signature and returns the provider's event.
   * Throws on a missing or bad signature — the caller must return 400.
   */
  verifyWebhook(rawBody: string, signature: string | undefined): unknown;
  /**
   * Maps a verified provider event to our normalized shape, or null
   * when the event kind is one we ignore.
   */
  normalizeEvent(event: unknown): NormalizedBillingEvent | null;
  /** The provider's event id, for webhook idempotency. */
  eventId(event: unknown): string;
}

/** One row of `billing_providers` (see @reponyx/database). */
export interface BillingProviderRow {
  provider: string;
  is_active: boolean;
  /** Country codes the provider serves, or null = worldwide. */
  countries: string[] | null;
}

/**
 * Which providers may be offered to a buyer in `country` (ISO 3166-1
 * alpha-2, uppercase, or null when unknown). A provider is offered
 * only when the admin enabled it, its env credentials exist, it can
 * sell through checkout, and the country is in its scope (null scope
 * = worldwide). An unknown country gets only the worldwide providers.
 */
export function availableProvidersForCountry(
  rows: BillingProviderRow[],
  isConfigured: (key: BillingProviderKey) => boolean,
  country: string | null
): BillingProviderKey[] {
  const out: BillingProviderKey[] = [];
  for (const key of BILLING_PROVIDERS) {
    if (!supportsCheckout(key)) continue;
    if (!isConfigured(key)) continue;
    const row = rows.find((r) => r.provider === key);
    if (!row || !row.is_active) continue;
    if (row.countries !== null && country !== null) {
      if (!row.countries.includes(country.toUpperCase())) continue;
    }
    if (row.countries !== null && country === null) continue;
    out.push(key);
  }
  return out;
}

/**
 * The buyer's country from the request headers. Vercel sets
 * `X-Vercel-IP-Country`; Cloudflare sets `CF-IPCountry`. Returns null
 * when neither is present — the buyer then picks their country.
 */
export function buyerCountry(headers: {
  get(name: string): string | null;
}): string | null {
  const vercel = headers.get("x-vercel-ip-country");
  if (vercel) return vercel.toUpperCase();
  const cf = headers.get("cf-ipcountry");
  if (cf) return cf.toUpperCase();
  return null;
}

/**
 * Maps a normalized event to the local subscription state. The local
 * model has three states (active | expired | cancelled) and an expiry
 * timestamp; the existing limits rule grants a package only while the
 * subscription is active and unexpired.
 *
 * - A live subscription (active, trialing, past_due) stays active
 *   until its current period end.
 * - A subscription cancelled but running to period end keeps the
 *   package until then, so it stays active with that expiry.
 * - An immediately-cancelled subscription loses the package at once.
 * - A checkout completion activates the package with no expiry yet —
 *   the subscription's created/updated event (sent right after)
 *   sets the period end. If that event never arrives the subscriber
 *   keeps access rather than being cut off after paying.
 */
export function resolveSubscriptionState(event: NormalizedBillingEvent): {
  status: "active" | "expired" | "cancelled";
  expiresAt: string | null;
} {
  if (event.type === "checkout_completed") {
    return { status: "active", expiresAt: null };
  }
  const live = ["active", "trialing", "past_due"].includes(
    event.providerStatus
  );
  if (live) {
    return { status: "active", expiresAt: event.currentPeriodEnd };
  }
  if (event.cancelAtPeriodEnd) {
    return { status: "active", expiresAt: event.currentPeriodEnd };
  }
  return { status: "cancelled", expiresAt: event.currentPeriodEnd };
}

/** Env credentials for one provider, read at call time. */
export interface ProviderEnv {
  configured: boolean;
  secretKey: string | null;
  webhookSecret: string | null;
}

function envValue(env: NodeJS.ProcessEnv, key: string): string | null {
  const value = env[key];
  return value !== undefined && value !== "" ? value : null;
}

/** Stripe credentials from the env contract (plan §9). */
export function stripeEnv(env: NodeJS.ProcessEnv = process.env): ProviderEnv {
  const secretKey = envValue(env, "STRIPE_SECRET_KEY");
  const webhookSecret = envValue(env, "STRIPE_WEBHOOK_SECRET");
  return {
    configured: secretKey !== null && webhookSecret !== null,
    secretKey,
    webhookSecret,
  };
}

/** PayPal credentials from the env contract (plan §9). */
export function paypalEnv(env: NodeJS.ProcessEnv = process.env): ProviderEnv {
  const clientId = envValue(env, "PAYPAL_CLIENT_ID");
  const clientSecret = envValue(env, "PAYPAL_CLIENT_SECRET");
  return {
    configured: clientId !== null && clientSecret !== null,
    secretKey: clientSecret,
    webhookSecret: envValue(env, "PAYPAL_WEBHOOK_ID"),
  };
}

/** Polar credentials from the env contract (plan §9). */
export function polarEnv(env: NodeJS.ProcessEnv = process.env): ProviderEnv {
  const token = envValue(env, "POLAR_ACCESS_TOKEN");
  return {
    configured: token !== null,
    secretKey: token,
    webhookSecret: envValue(env, "POLAR_WEBHOOK_SECRET"),
  };
}

/** Freemius credentials from the env contract (plan §9). */
export function freemiusEnv(env: NodeJS.ProcessEnv = process.env): ProviderEnv {
  const apiKey = envValue(env, "FREEMIUS_API_KEY");
  const secretKey = envValue(env, "FREEMIUS_SECRET_KEY");
  return {
    configured:
      envValue(env, "FREEMIUS_PRODUCT_ID") !== null &&
      apiKey !== null &&
      secretKey !== null,
    secretKey,
    webhookSecret: envValue(env, "FREEMIUS_PUBLIC_KEY"),
  };
}

/** Gumroad credentials from the env contract (plan §9). */
export function gumroadEnv(env: NodeJS.ProcessEnv = process.env): ProviderEnv {
  return {
    configured:
      envValue(env, "GUMROAD_APP_ID") !== null &&
      envValue(env, "GUMROAD_APP_SECRET") !== null,
    secretKey: envValue(env, "GUMROAD_APP_SECRET"),
    webhookSecret: null,
  };
}

/** True when the provider's env credentials exist (W6b uses these). */
export function isProviderConfigured(
  key: BillingProviderKey,
  env: NodeJS.ProcessEnv = process.env
): boolean {
  switch (key) {
    case "stripe":
      return stripeEnv(env).configured;
    case "paypal":
      return paypalEnv(env).configured;
    case "polar":
      return polarEnv(env).configured;
    case "freemius":
      return freemiusEnv(env).configured;
    case "gumroad":
      return gumroadEnv(env).configured;
  }
}
