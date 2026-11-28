import { NextResponse } from "next/server";
import { recordBillingEvent } from "@reponyx/database";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { applyBillingEvent, getStripeProvider } from "@/lib/billing";
import { getDb } from "@/lib/db";

/**
 * Stripe webhook (W6). Verifies the signature with
 * STRIPE_WEBHOOK_SECRET (offline HMAC), records the event id so a
 * retry is acknowledged without re-applying, then applies the event
 * to the user's subscription. Always returns 200 for a verified
 * event — even one we skip — so Stripe stops retrying it.
 */
export async function POST(req: Request) {
  try {
    const client = getDb();
    const provider = getStripeProvider();
    if (!provider.isConfigured()) {
      throw new ApiError("billing_not_configured", 503);
    }
    const rawBody = await req.text();
    const signature = req.headers.get("stripe-signature") ?? undefined;
    let event: unknown;
    try {
      event = provider.verifyWebhook(rawBody, signature);
    } catch {
      throw new ApiError("invalid_body", 400);
    }
    const eventId = provider.eventId(event);
    const firstSeen = await recordBillingEvent(client, "stripe", eventId);
    if (!firstSeen) {
      return NextResponse.json({ received: true, replay: true });
    }
    const normalized = provider.normalizeEvent(event);
    if (normalized) {
      await applyBillingEvent(client, "stripe", normalized);
    }
    return NextResponse.json({ received: true });
  } catch (err) {
    return errorResponse(err);
  }
}
