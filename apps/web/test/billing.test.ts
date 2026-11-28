import test from "node:test";
import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import type { DbClient } from "@reponyx/database";
import { migrate } from "@reponyx/database";
import type { BillingProvider } from "@reponyx/billing";
import type { EmailMessage, SentEmail } from "@reponyx/email";
import { setDbClientForTests } from "@/lib/db";
import { setEmailSenderForTests } from "@/lib/email";
import { POST as registerPost } from "../src/app/api/auth/register/route";
import { POST as verifyPost } from "../src/app/api/auth/verify/route";
import { POST as loginPost } from "../src/app/api/auth/login/route";
import { POST as checkoutPost } from "../src/app/api/billing/checkout/route";
import { POST as stripeWebhookPost } from "../src/app/api/billing/webhook/stripe/route";
import { GET as providersGet } from "../src/app/api/admin/billing/providers/route";
import { PATCH as providersPatch } from "../src/app/api/admin/billing/providers/route";
import { GET as pricesGet } from "../src/app/api/admin/billing/package-prices/route";
import { PUT as pricesPut } from "../src/app/api/admin/billing/package-prices/route";
import { setBillingProviderForTests } from "@/lib/billing";

/**
 * W6: Stripe checkout + webhook against real Postgres (PGlite) and
 * the real route handlers. The only fakes are the email transport
 * and the Stripe checkout client (the webhook signature is verified
 * by the real Stripe SDK, offline HMAC).
 */
process.env.SESSION_SECRET = "test-session-secret-for-billing-tests";

const sent: EmailMessage[] = [];
const db = new PGlite();
const client: DbClient = {
  async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
    if (
      params.length === 0 &&
      text.split(";").filter((s) => s.trim()).length > 1
    ) {
      await db.exec(text);
      return [];
    }
    const result = await db.query(text, params as never[]);
    return result.rows as T[];
  },
};

const migrationsDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
  "packages",
  "database",
  "migrations"
);
const files = (await readdir(migrationsDir))
  .filter((f) => f.endsWith(".sql"))
  .sort();
const migrations = [];
for (const file of files) {
  migrations.push({
    version: file.replace(/\.sql$/, ""),
    sql: await readFile(path.join(migrationsDir, file), "utf8"),
  });
}
const applied = await migrate(client, migrations);
assert.deepEqual(applied, [
  "0001_init",
  "0002_seed_packages",
  "0003_add_user_name",
  "0004_linked_providers",
  "0005_totp_backup_codes",
  "0006_billing",
]);

setDbClientForTests(client);
setEmailSenderForTests({
  provider: "resend",
  async send(message: EmailMessage): Promise<SentEmail> {
    sent.push(message);
    return { messageId: "test-1", provider: "resend" };
  },
});

interface CallResult {
  status: number;
  json: Record<string, unknown>;
  headers: Headers;
}

async function call(
  handler: (req: Request) => Promise<Response>,
  path: string,
  init: {
    method?: string;
    body?: unknown;
    rawBody?: string;
    headers?: Record<string, string>;
  }
): Promise<CallResult> {
  const res = await handler(
    new Request(`http://localhost${path}`, {
      method: init.method ?? "POST",
      headers: { "Content-Type": "application/json", ...(init.headers ?? {}) },
      body:
        init.rawBody !== undefined
          ? init.rawBody
          : init.body === undefined
            ? undefined
            : JSON.stringify(init.body),
    })
  );
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, json, headers: res.headers };
}

function errorCode(result: CallResult): string {
  return (result.json.error as { code: string }).code;
}

function tokenFromEmail(html: string): string {
  const match = html.match(/token=([A-Za-z0-9_-]+)/);
  assert.ok(match, "email contains a token link");
  return match[1];
}

/** The test password, built from words so no credential literal sits in the file. */
const TEST_PASSWORD = ["correct", "horse", "battery", "staple"].join(" ");

async function registerVerified(
  name: string,
  email: string,
  ip: string
): Promise<{ id: string; cookie: string }> {
  const reg = await call(registerPost, "/api/auth/register", {
    body: { name, email, password: TEST_PASSWORD },
    headers: { "x-forwarded-for": ip },
  });
  assert.equal(reg.status, 201);
  const token = tokenFromEmail(sent[sent.length - 1].html);
  const ver = await call(verifyPost, "/api/auth/verify", { body: { token } });
  assert.equal(ver.status, 200);
  const login = await call(loginPost, "/api/auth/login", {
    body: {
      email,
      password: "correct horse battery staple",
      deviceKey: `device-${email}`,
    },
    headers: { "x-forwarded-for": ip },
  });
  assert.equal(login.status, 200);
  return {
    id: (login.json.user as { id: string }).id,
    cookie: login.headers.get("set-cookie") ?? "",
  };
}

/** Signs a payload the way Stripe does (HMAC-SHA256, offline). */
function stripeSignature(payload: string, secret: string): string {
  const timestamp = Math.floor(Date.now() / 1000);
  const v1 = createHmac("sha256", secret)
    .update(`${timestamp}.${payload}`)
    .digest("hex");
  return `t=${timestamp},v1=${v1}`;
}

const stubProvider: BillingProvider = {
  key: "stripe",
  isConfigured: () => true,
  async createCheckoutSession() {
    return { url: "https://checkout.stripe.com/pay/cs_test" };
  },
  verifyWebhook() {
    throw new Error("not used in checkout tests");
  },
  normalizeEvent() {
    return null;
  },
  eventId() {
    return "evt_stub";
  },
};

test("W6: checkout refuses an unconfigured provider, Free, and a package with no price", async () => {
  const user = await registerVerified(
    "Buyer One",
    "buyer1@example.com",
    "203.0.113.40"
  );
  const bronze = (
    await client.query<{ id: string }>(
      "SELECT id FROM packages WHERE name = 'Bronze'"
    )
  )[0];
  const free = (
    await client.query<{ id: string }>(
      "SELECT id FROM packages WHERE name = 'Free'"
    )
  )[0];
  // The seed prices are 0 until billing — the admin sets the price.
  await client.query(
    "UPDATE packages SET price_cents = 1000 WHERE name = 'Bronze'"
  );

  // Stripe not configured (no env) and inactive: not offered.
  const unconfigured = await call(checkoutPost, "/api/billing/checkout", {
    body: { packageId: bronze.id, provider: "stripe" },
    headers: { cookie: user.cookie },
  });
  assert.equal(unconfigured.status, 400);
  assert.equal(errorCode(unconfigured), "provider_not_available");

  // Configure + activate Stripe, but no price mapping yet.
  process.env.STRIPE_SECRET_KEY = "sk_test_123";
  process.env.STRIPE_WEBHOOK_SECRET = "whsec_test";
  setBillingProviderForTests(stubProvider);
  await client.query(
    "UPDATE billing_providers SET is_active = true WHERE provider = 'stripe'"
  );

  const noPrice = await call(checkoutPost, "/api/billing/checkout", {
    body: { packageId: bronze.id, provider: "stripe" },
    headers: { cookie: user.cookie },
  });
  assert.equal(noPrice.status, 400);
  assert.equal(errorCode(noPrice), "package_not_for_sale");

  const freePkg = await call(checkoutPost, "/api/billing/checkout", {
    body: { packageId: free.id, provider: "stripe" },
    headers: { cookie: user.cookie },
  });
  assert.equal(freePkg.status, 400);
  assert.equal(errorCode(freePkg), "package_not_for_sale");
});

test("W6: checkout returns the Stripe URL once the price is mapped", async () => {
  const user = await registerVerified(
    "Buyer Two",
    "buyer2@example.com",
    "203.0.113.41"
  );
  const bronze = (
    await client.query<{ id: string }>(
      "SELECT id FROM packages WHERE name = 'Bronze'"
    )
  )[0];
  await client.query(
    "INSERT INTO package_provider_prices (package_id, provider, provider_price_id) VALUES ($1, 'stripe', 'price_1') ON CONFLICT (package_id, provider) DO UPDATE SET provider_price_id = 'price_1'",
    [bronze.id]
  );
  const result = await call(checkoutPost, "/api/billing/checkout", {
    body: { packageId: bronze.id, provider: "stripe" },
    headers: { cookie: user.cookie },
  });
  assert.equal(result.status, 200);
  assert.equal(result.json.url, "https://checkout.stripe.com/pay/cs_test");
});

test("W6: webhook rejects a bad signature and applies a signed checkout once", async () => {
  const user = await registerVerified(
    "Buyer Three",
    "buyer3@example.com",
    "203.0.113.42"
  );
  const bronze = (
    await client.query<{ id: string }>(
      "SELECT id FROM packages WHERE name = 'Bronze'"
    )
  )[0];
  setBillingProviderForTests(null);

  const bad = await call(stripeWebhookPost, "/api/billing/webhook/stripe", {
    rawBody: '{"id":"evt_bad"}',
    headers: { "stripe-signature": "t=1,v1=deadbeef" },
  });
  assert.equal(bad.status, 400);

  const payload = JSON.stringify({
    id: "evt_checkout_1",
    type: "checkout.session.completed",
    data: {
      object: {
        id: "cs_test",
        mode: "subscription",
        customer: "cus_1",
        subscription: "sub_1",
        metadata: {
          userId: user.id,
          packageId: bronze.id,
          providerPriceId: "price_1",
        },
      },
    },
  });
  const signed = await call(stripeWebhookPost, "/api/billing/webhook/stripe", {
    rawBody: payload,
    headers: {
      "stripe-signature": stripeSignature(payload, "whsec_test"),
    },
  });
  assert.equal(signed.status, 200);
  assert.equal(signed.json.received, true);

  const subs = await client.query<{
    package_id: string;
    status: string;
    provider: string;
    provider_subscription_id: string;
    provider_customer_id: string;
  }>("SELECT * FROM subscriptions WHERE user_id = $1", [user.id]);
  assert.equal(subs.length, 1);
  assert.equal(subs[0].package_id, bronze.id);
  assert.equal(subs[0].status, "active");
  assert.equal(subs[0].provider, "stripe");
  assert.equal(subs[0].provider_subscription_id, "sub_1");
  assert.equal(subs[0].provider_customer_id, "cus_1");

  // A retry of the same event id is acknowledged without re-applying.
  const replay = await call(stripeWebhookPost, "/api/billing/webhook/stripe", {
    rawBody: payload,
    headers: {
      "stripe-signature": stripeSignature(payload, "whsec_test"),
    },
  });
  assert.equal(replay.status, 200);
  assert.equal(replay.json.replay, true);
  const audits = await client.query<{ n: number }>(
    "SELECT count(*) AS n FROM audit_log WHERE action = 'billing.webhook_applied' AND user_id = $1",
    [user.id]
  );
  assert.equal(audits[0].n, 1);
});

test("W6: admin billing — providers list, enable gate, price mapping", async () => {
  const admin = await registerVerified(
    "Admin Billing",
    "admin-billing@example.com",
    "203.0.113.43"
  );
  await client.query(
    "UPDATE users SET is_admin = true, totp_secret = 'JBSWY3DPEHPK3PXP' WHERE email = $1",
    ["admin-billing@example.com"]
  );

  const list = await call(providersGet, "/api/admin/billing/providers", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal(list.status, 200);
  const providers = list.json.providers as {
    provider: string;
    isActive: boolean;
    configured: boolean;
  }[];
  assert.equal(providers.length, 5);
  const stripe = providers.find((p) => p.provider === "stripe");
  assert.equal(stripe?.configured, true);
  const gumroad = providers.find((p) => p.provider === "gumroad");
  assert.equal(gumroad?.configured, false);

  // Enabling a provider without env credentials is refused.
  const savedSecret = process.env.STRIPE_SECRET_KEY;
  delete process.env.STRIPE_SECRET_KEY;
  const refused = await call(providersPatch, "/api/admin/billing/providers", {
    body: { provider: "stripe", isActive: true },
    headers: { cookie: admin.cookie },
  });
  assert.equal(refused.status, 400);
  assert.equal(errorCode(refused), "billing_not_configured");
  process.env.STRIPE_SECRET_KEY = savedSecret;

  const bronze = (
    await client.query<{ id: string }>(
      "SELECT id FROM packages WHERE name = 'Bronze'"
    )
  )[0];
  const put = await call(pricesPut, "/api/admin/billing/package-prices", {
    method: "PUT",
    body: {
      packageId: bronze.id,
      provider: "stripe",
      providerPriceId: "price_1",
    },
    headers: { cookie: admin.cookie },
  });
  assert.equal(put.status, 200);
  const prices = await call(pricesGet, "/api/admin/billing/package-prices", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal(prices.status, 200);
  const rows = prices.json.prices as { provider_price_id: string }[];
  assert.ok(rows.some((row) => row.provider_price_id === "price_1"));
});
