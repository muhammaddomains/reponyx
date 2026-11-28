/**
 * Billing helpers for the web (W6). The Stripe provider is created
 * here so routes share one instance; tests replace it with
 * setBillingProviderForTests. The webhook apply logic lives here too
 * so the route stays thin and the behaviour is unit-testable.
 */
import {
  createStripeProvider,
  resolveSubscriptionState,
  type BillingProvider,
  type NormalizedBillingEvent,
} from "@reponyx/billing";
import {
  findPackageById,
  recordAudit,
  setUserSubscription,
  type DbClient,
} from "@reponyx/database";

let testProvider: BillingProvider | null = null;

export function setBillingProviderForTests(
  provider: BillingProvider | null
): void {
  testProvider = provider;
}

export function getStripeProvider(): BillingProvider {
  if (testProvider) return testProvider;
  return createStripeProvider();
}

export function appBaseUrl(): string {
  return process.env.NEXT_PUBLIC_APP_URL ?? "http://localhost:3000";
}

/**
 * Applies one normalized webhook event to the database. Returns
 * "applied" when the subscription was written, "skipped" when the
 * event carries no account (missing metadata — logged for the admin
 * to see) or no matching package. The caller handles idempotency
 * (recordBillingEvent) before calling this.
 */
export async function applyBillingEvent(
  client: DbClient,
  provider: string,
  event: NormalizedBillingEvent
): Promise<"applied" | "skipped"> {
  if (!event.userId || !event.packageId) {
    await recordAudit(client, {
      userId: event.userId,
      action: "billing.webhook_skipped",
      details: {
        provider,
        type: event.type,
        reason: "missing_metadata",
      },
    });
    return "skipped";
  }
  const pkg = await findPackageById(client, event.packageId);
  if (!pkg) {
    await recordAudit(client, {
      userId: event.userId,
      action: "billing.webhook_skipped",
      details: {
        provider,
        type: event.type,
        reason: "package_not_found",
        packageId: event.packageId,
      },
    });
    return "skipped";
  }
  const state = resolveSubscriptionState(event);
  await setUserSubscription(client, {
    userId: event.userId,
    packageId: event.packageId,
    status: state.status,
    expiresAt: state.expiresAt,
    provider,
    providerSubscriptionId: event.providerSubscriptionId,
    providerCustomerId: event.providerCustomerId,
  });
  await recordAudit(client, {
    userId: event.userId,
    action: "billing.webhook_applied",
    details: {
      provider,
      type: event.type,
      package: pkg.name,
      status: state.status,
      expiresAt: state.expiresAt,
    },
  });
  return "applied";
}
