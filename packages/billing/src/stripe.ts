/**
 * Stripe provider (W6a). Checkout Sessions in subscription mode +
 * webhook verification via the Stripe SDK (offline HMAC — no network
 * needed to verify a signature, so this is unit-testable).
 *
 * The checkout session carries the account in its metadata
 * ({userId, packageId, providerPriceId}) and the subscription it
 * creates carries the same metadata, so every webhook event names
 * its account without an extra API call.
 */
import Stripe from "stripe";
import {
  stripeEnv,
  type BillingProvider,
  type CheckoutSessionInput,
  type CheckoutSessionResult,
  type NormalizedBillingEvent,
} from "./providers";

/** The narrow slice of the Stripe SDK this provider uses. */
export interface StripeClient {
  createCheckoutSession(params: {
    mode: "subscription";
    customer?: string;
    customerEmail?: string;
    lineItems: { price: string; quantity: number }[];
    metadata: Record<string, string>;
    subscriptionMetadata: Record<string, string>;
    successUrl: string;
    cancelUrl: string;
  }): Promise<{ url: string | null }>;
  constructWebhookEvent(
    rawBody: string,
    signature: string,
    secret: string
  ): unknown;
}

function createRealStripeClient(secretKey: string): StripeClient {
  const stripe = new Stripe(secretKey);
  return {
    async createCheckoutSession(params) {
      const session = await stripe.checkout.sessions.create({
        mode: "subscription",
        customer: params.customer,
        customer_email: params.customerEmail,
        line_items: params.lineItems.map((item) => ({
          price: item.price,
          quantity: item.quantity,
        })),
        metadata: params.metadata,
        subscription_data: { metadata: params.subscriptionMetadata },
        success_url: params.successUrl,
        cancel_url: params.cancelUrl,
      });
      return { url: session.url };
    },
    constructWebhookEvent(rawBody, signature, secret) {
      return stripe.webhooks.constructEvent(rawBody, signature, secret);
    },
  };
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)
    : null;
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function metadataOf(object: Record<string, unknown>): Record<string, string> {
  const metadata = asRecord(object["metadata"]);
  if (!metadata) return {};
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(metadata)) {
    if (typeof value === "string") out[key] = value;
  }
  return out;
}

export function createStripeProvider(options?: {
  env?: NodeJS.ProcessEnv;
  client?: StripeClient;
}): BillingProvider {
  const env = options?.env ?? process.env;
  return {
    key: "stripe",

    isConfigured(): boolean {
      return stripeEnv(env).configured;
    },

    async createCheckoutSession(
      input: CheckoutSessionInput
    ): Promise<CheckoutSessionResult> {
      const config = stripeEnv(env);
      if (!config.secretKey) throw new Error("stripe_not_configured");
      const client =
        options?.client ?? createRealStripeClient(config.secretKey);
      const metadata = {
        userId: input.userId,
        packageId: input.packageId,
        providerPriceId: input.providerPriceId,
      };
      const session = await client.createCheckoutSession({
        mode: "subscription",
        customer: input.customerId ?? undefined,
        customerEmail: input.customerId ? undefined : input.userEmail,
        lineItems: [{ price: input.providerPriceId, quantity: 1 }],
        metadata,
        subscriptionMetadata: {
          userId: input.userId,
          packageId: input.packageId,
        },
        successUrl: input.successUrl,
        cancelUrl: input.cancelUrl,
      });
      if (!session.url) throw new Error("stripe_no_checkout_url");
      return { url: session.url };
    },

    verifyWebhook(rawBody: string, signature: string | undefined): unknown {
      const config = stripeEnv(env);
      if (!config.webhookSecret) throw new Error("stripe_not_configured");
      if (!signature) throw new Error("stripe_missing_signature");
      const client =
        options?.client ?? createRealStripeClient(config.secretKey ?? "unused");
      return client.constructWebhookEvent(
        rawBody,
        signature,
        config.webhookSecret
      );
    },

    normalizeEvent(event: unknown): NormalizedBillingEvent | null {
      const record = asRecord(event);
      const type = asString(record?.["type"]);
      const data = asRecord(record?.["data"]);
      const object = asRecord(data?.["object"]);
      if (!type || !object) return null;
      const metadata = metadataOf(object);
      const userId = metadata["userId"] ?? null;
      const packageId = metadata["packageId"] ?? null;

      if (type === "checkout.session.completed") {
        const customer = asString(object["customer"]);
        const subscription = asString(object["subscription"]);
        if (object["mode"] !== "subscription") return null;
        return {
          type: "checkout_completed",
          userId,
          packageId,
          providerCustomerId: customer,
          providerSubscriptionId: subscription,
          providerPriceId: metadata["providerPriceId"] ?? null,
          providerStatus: "active",
          cancelAtPeriodEnd: false,
          currentPeriodEnd: null,
        };
      }

      if (
        type === "customer.subscription.created" ||
        type === "customer.subscription.updated" ||
        type === "customer.subscription.deleted"
      ) {
        const status = asString(object["status"]) ?? "unknown";
        const periodEnd = object["current_period_end"];
        const items = asRecord(object["items"]);
        const itemList = Array.isArray(items?.["data"])
          ? (items["data"] as unknown[])
          : [];
        const firstItem = asRecord(itemList[0]);
        const price = asRecord(firstItem?.["price"]);
        const priceId = asString(price?.["id"]);
        const customer = asString(object["customer"]);
        return {
          type:
            type === "customer.subscription.deleted"
              ? "subscription_deleted"
              : "subscription_updated",
          userId,
          packageId,
          providerCustomerId: customer,
          providerSubscriptionId: asString(object["id"]),
          providerPriceId: priceId,
          providerStatus: status,
          cancelAtPeriodEnd: object["cancel_at_period_end"] === true,
          currentPeriodEnd:
            typeof periodEnd === "number"
              ? new Date(periodEnd * 1000).toISOString()
              : null,
        };
      }

      return null;
    },

    eventId(event: unknown): string {
      const record = asRecord(event);
      const id = asString(record?.["id"]);
      if (!id) throw new Error("stripe_event_without_id");
      return id;
    },
  };
}
