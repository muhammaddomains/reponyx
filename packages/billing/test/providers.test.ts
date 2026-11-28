import test from "node:test";
import assert from "node:assert/strict";
import {
  availableProvidersForCountry,
  buyerCountry,
  freemiusEnv,
  gumroadEnv,
  isProviderConfigured,
  paypalEnv,
  polarEnv,
  resolveSubscriptionState,
  stripeEnv,
  supportsCheckout,
  type BillingProviderRow,
  type NormalizedBillingEvent,
} from "../src/providers";

function rows(active: string[], countries?: Record<string, string[] | null>) {
  return ["stripe", "paypal", "polar", "freemius", "gumroad"].map(
    (provider) => ({
      provider,
      is_active: active.includes(provider),
      countries: countries?.[provider] ?? null,
      created_at: "2026-11-28T00:00:00Z",
      updated_at: "2026-11-28T00:00:00Z",
    })
  ) as BillingProviderRow[];
}

test("supportsCheckout is false only for gumroad", () => {
  assert.equal(supportsCheckout("stripe"), true);
  assert.equal(supportsCheckout("paypal"), true);
  assert.equal(supportsCheckout("gumroad"), false);
});

test("availableProvidersForCountry needs active + configured + checkout", () => {
  const configured = (key: string) => key === "stripe" || key === "paypal";
  assert.deepEqual(
    availableProvidersForCountry(
      rows(["stripe", "paypal", "gumroad"]),
      configured,
      "US"
    ),
    ["stripe", "paypal"]
  );
  // Inactive stripe is not offered.
  assert.deepEqual(
    availableProvidersForCountry(rows(["paypal"]), configured, "US"),
    ["paypal"]
  );
  // Unconfigured paypal is not offered.
  assert.deepEqual(
    availableProvidersForCountry(
      rows(["stripe", "paypal"]),
      (key) => key === "stripe",
      "US"
    ),
    ["stripe"]
  );
});

test("availableProvidersForCountry honours country scopes", () => {
  const configured = () => true;
  const scoped = rows(["stripe", "paypal"], { paypal: ["US", "GB"] });
  assert.deepEqual(availableProvidersForCountry(scoped, configured, "US"), [
    "stripe",
    "paypal",
  ]);
  assert.deepEqual(availableProvidersForCountry(scoped, configured, "PK"), [
    "stripe",
  ]);
  // Unknown country gets only the worldwide providers.
  assert.deepEqual(availableProvidersForCountry(scoped, configured, null), [
    "stripe",
  ]);
});

test("buyerCountry reads the Vercel header first, then Cloudflare", () => {
  const headers = new Headers({ "x-vercel-ip-country": "pk" });
  assert.equal(buyerCountry(headers), "PK");
  const cf = new Headers({ "cf-ipcountry": "gb" });
  assert.equal(buyerCountry(cf), "GB");
  assert.equal(buyerCountry(new Headers()), null);
});

function event(
  overrides: Partial<NormalizedBillingEvent>
): NormalizedBillingEvent {
  return {
    type: "subscription_updated",
    userId: "u1",
    packageId: "p1",
    providerCustomerId: "cus_1",
    providerSubscriptionId: "sub_1",
    providerPriceId: "price_1",
    providerStatus: "active",
    cancelAtPeriodEnd: false,
    currentPeriodEnd: "2027-01-01T00:00:00.000Z",
    ...overrides,
  };
}

test("resolveSubscriptionState: live statuses stay active to period end", () => {
  for (const providerStatus of ["active", "trialing", "past_due"]) {
    assert.deepEqual(resolveSubscriptionState(event({ providerStatus })), {
      status: "active",
      expiresAt: "2027-01-01T00:00:00.000Z",
    });
  }
});

test("resolveSubscriptionState: cancel-at-period-end keeps the package until then", () => {
  assert.deepEqual(
    resolveSubscriptionState(
      event({ providerStatus: "canceled", cancelAtPeriodEnd: true })
    ),
    { status: "active", expiresAt: "2027-01-01T00:00:00.000Z" }
  );
});

test("resolveSubscriptionState: immediate cancel loses the package", () => {
  assert.deepEqual(
    resolveSubscriptionState(
      event({ providerStatus: "canceled", cancelAtPeriodEnd: false })
    ),
    { status: "cancelled", expiresAt: "2027-01-01T00:00:00.000Z" }
  );
});

test("resolveSubscriptionState: checkout activates with no expiry yet", () => {
  assert.deepEqual(
    resolveSubscriptionState(
      event({ type: "checkout_completed", currentPeriodEnd: null })
    ),
    { status: "active", expiresAt: null }
  );
});

test("env config: a provider is configured only when its keys exist", () => {
  const env = {
    STRIPE_SECRET_KEY: "sk_test_x",
    STRIPE_WEBHOOK_SECRET: "whsec_x",
  } as NodeJS.ProcessEnv;
  assert.equal(stripeEnv(env).configured, true);
  assert.equal(stripeEnv({} as NodeJS.ProcessEnv).configured, false);
  assert.equal(
    stripeEnv({ STRIPE_SECRET_KEY: "sk_test_x" } as NodeJS.ProcessEnv)
      .configured,
    false
  );
  assert.equal(paypalEnv(env).configured, false);
  assert.equal(polarEnv(env).configured, false);
  assert.equal(freemiusEnv(env).configured, false);
  assert.equal(gumroadEnv(env).configured, false);
  assert.equal(isProviderConfigured("stripe", env), true);
  assert.equal(isProviderConfigured("gumroad", env), false);
});
