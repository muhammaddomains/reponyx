import test from "node:test";
import assert from "node:assert/strict";
import Stripe from "stripe";
import { createStripeProvider, type StripeClient } from "../src/stripe";

const ENV = {
  STRIPE_SECRET_KEY: "sk_test_123",
  STRIPE_WEBHOOK_SECRET: "whsec_test",
} as NodeJS.ProcessEnv;

function stubClient(overrides?: Partial<StripeClient>): StripeClient {
  return {
    async createCheckoutSession() {
      return { url: "https://checkout.stripe.com/pay/cs_test" };
    },
    constructWebhookEvent(rawBody: string) {
      return JSON.parse(rawBody) as unknown;
    },
    ...overrides,
  };
}

test("createCheckoutSession returns the Stripe URL and carries metadata", async () => {
  let seen: Parameters<StripeClient["createCheckoutSession"]>[0] | null = null;
  const client = stubClient({
    async createCheckoutSession(params) {
      seen = params;
      return { url: "https://checkout.stripe.com/pay/cs_test" };
    },
  });
  const provider = createStripeProvider({ env: ENV, client });
  const result = await provider.createCheckoutSession({
    userId: "u1",
    userEmail: "a@example.com",
    packageId: "p1",
    packageName: "Bronze",
    providerPriceId: "price_1",
    successUrl: "https://reponyx.bond/billing/success",
    cancelUrl: "https://reponyx.bond/billing/cancel",
  });
  assert.equal(result.url, "https://checkout.stripe.com/pay/cs_test");
  assert.ok(seen);
  assert.equal(seen.mode, "subscription");
  assert.deepEqual(seen.lineItems, [{ price: "price_1", quantity: 1 }]);
  assert.equal(seen.metadata.userId, "u1");
  assert.equal(seen.metadata.packageId, "p1");
  assert.equal(seen.metadata.providerPriceId, "price_1");
  assert.equal(seen.subscriptionMetadata.userId, "u1");
  assert.equal(seen.customerEmail, "a@example.com");
});

test("createCheckoutSession throws when Stripe is not configured", async () => {
  const provider = createStripeProvider({
    env: {} as NodeJS.ProcessEnv,
    client: stubClient(),
  });
  assert.equal(provider.isConfigured(), false);
  await assert.rejects(
    provider.createCheckoutSession({
      userId: "u1",
      userEmail: "a@example.com",
      packageId: "p1",
      packageName: "Bronze",
      providerPriceId: "price_1",
      successUrl: "https://example.com/success",
      cancelUrl: "https://example.com/cancel",
    }),
    /stripe_not_configured/
  );
});

test("createCheckoutSession reuses an existing customer id", async () => {
  let seen: Parameters<StripeClient["createCheckoutSession"]>[0] | null = null;
  const client = stubClient({
    async createCheckoutSession(params) {
      seen = params;
      return { url: "https://checkout.stripe.com/pay/cs_test" };
    },
  });
  const provider = createStripeProvider({ env: ENV, client });
  await provider.createCheckoutSession({
    userId: "u1",
    userEmail: "a@example.com",
    packageId: "p1",
    packageName: "Bronze",
    providerPriceId: "price_1",
    successUrl: "https://example.com/success",
    cancelUrl: "https://example.com/cancel",
    customerId: "cus_9",
  });
  assert.ok(seen);
  assert.equal(seen.customer, "cus_9");
  assert.equal(seen.customerEmail, undefined);
});

/** Signs a payload the way Stripe does, using the real SDK (offline). */
function signedPayload(payload: string): {
  rawBody: string;
  signature: string;
} {
  const stripe = new Stripe("sk_test_unused");
  const signature = stripe.webhooks.generateTestHeaderString({
    payload,
    secret: "whsec_test",
  });
  return { rawBody: payload, signature };
}

test("webhook: a signed checkout.session.completed normalizes", () => {
  const payload = JSON.stringify({
    id: "evt_1",
    type: "checkout.session.completed",
    data: {
      object: {
        id: "cs_test",
        mode: "subscription",
        customer: "cus_1",
        subscription: "sub_1",
        metadata: { userId: "u1", packageId: "p1", providerPriceId: "price_1" },
      },
    },
  });
  const { rawBody, signature } = signedPayload(payload);
  const provider = createStripeProvider({ env: ENV });
  const event = provider.verifyWebhook(rawBody, signature);
  assert.equal(provider.eventId(event), "evt_1");
  assert.deepEqual(provider.normalizeEvent(event), {
    type: "checkout_completed",
    userId: "u1",
    packageId: "p1",
    providerCustomerId: "cus_1",
    providerSubscriptionId: "sub_1",
    providerPriceId: "price_1",
    providerStatus: "active",
    cancelAtPeriodEnd: false,
    currentPeriodEnd: null,
  });
});

test("webhook: a bad signature is rejected", () => {
  const provider = createStripeProvider({ env: ENV });
  assert.throws(
    () => provider.verifyWebhook('{"id":"evt_1"}', "t=1,v1=deadbeef"),
    /signature/i
  );
  assert.throws(
    () => provider.verifyWebhook('{"id":"evt_1"}', undefined),
    /stripe_missing_signature/
  );
});

test("webhook: subscription.updated normalizes with the price id and period end", () => {
  const payload = JSON.stringify({
    id: "evt_2",
    type: "customer.subscription.updated",
    data: {
      object: {
        id: "sub_1",
        status: "active",
        customer: "cus_1",
        current_period_end: 1798761600,
        cancel_at_period_end: false,
        metadata: { userId: "u1", packageId: "p1" },
        items: { data: [{ price: { id: "price_1" } }] },
      },
    },
  });
  const { rawBody, signature } = signedPayload(payload);
  const provider = createStripeProvider({ env: ENV });
  const event = provider.verifyWebhook(rawBody, signature);
  assert.equal(provider.eventId(event), "evt_2");
  assert.deepEqual(provider.normalizeEvent(event), {
    type: "subscription_updated",
    userId: "u1",
    packageId: "p1",
    providerCustomerId: "cus_1",
    providerSubscriptionId: "sub_1",
    providerPriceId: "price_1",
    providerStatus: "active",
    cancelAtPeriodEnd: false,
    currentPeriodEnd: "2027-01-01T00:00:00.000Z",
  });
});

test("webhook: subscription.deleted normalizes as deleted", () => {
  const payload = JSON.stringify({
    id: "evt_3",
    type: "customer.subscription.deleted",
    data: {
      object: {
        id: "sub_1",
        status: "canceled",
        customer: "cus_1",
        current_period_end: 1798761600,
        cancel_at_period_end: true,
        metadata: { userId: "u1", packageId: "p1" },
        items: { data: [{ price: { id: "price_1" } }] },
      },
    },
  });
  const { rawBody, signature } = signedPayload(payload);
  const provider = createStripeProvider({ env: ENV });
  const event = provider.verifyWebhook(rawBody, signature);
  const normalized = provider.normalizeEvent(event);
  assert.equal(normalized?.type, "subscription_deleted");
  assert.equal(normalized?.cancelAtPeriodEnd, true);
});

test("webhook: unrelated event kinds are ignored", () => {
  const payload = JSON.stringify({
    id: "evt_4",
    type: "invoice.paid",
    data: { object: { id: "in_1" } },
  });
  const { rawBody, signature } = signedPayload(payload);
  const provider = createStripeProvider({ env: ENV });
  const event = provider.verifyWebhook(rawBody, signature);
  assert.equal(provider.normalizeEvent(event), null);
});
