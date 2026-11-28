import { NextResponse } from "next/server";
import { z } from "zod";
import {
  availableProvidersForCountry,
  buyerCountry,
  isProviderConfigured,
} from "@reponyx/billing";
import {
  findActiveSubscriptionForUser,
  findPackageById,
  findPackageProviderPrice,
  listBillingProviders,
  recordAudit,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { appBaseUrl, getStripeProvider } from "@/lib/billing";
import { getDb } from "@/lib/db";

const CheckoutBody = z.object({
  packageId: z.string().uuid(),
  provider: z.enum(["stripe", "paypal", "polar", "freemius", "gumroad"]),
  /** ISO 3166-1 alpha-2, when the buyer picks their country. */
  country: z
    .string()
    .regex(/^[A-Za-z]{2}$/)
    .optional(),
});

/**
 * Start a checkout for a package (W6). Only Stripe sells in W6a —
 * the other providers are registered but their checkout is W6b, so
 * they are refused here with `provider_not_available`. The buyer is
 * only offered providers active for their country (plan §9).
 */
export async function POST(req: Request) {
  try {
    const client = getDb();
    const session = await requireSession(client, req);
    const body = await parseJsonBody(req, CheckoutBody);

    const pkg = await findPackageById(client, body.packageId);
    if (!pkg || !pkg.is_active) {
      throw new ApiError("package_not_found", 404);
    }
    if (pkg.name === "Free" || pkg.price_cents <= 0) {
      throw new ApiError("package_not_for_sale", 400);
    }

    const country = body.country?.toUpperCase() ?? buyerCountry(req.headers);
    const providers = await listBillingProviders(client);
    const available = availableProvidersForCountry(
      providers,
      (key) =>
        key === "stripe"
          ? getStripeProvider().isConfigured()
          : isProviderConfigured(key),
      country
    );
    if (!available.includes(body.provider)) {
      throw new ApiError("provider_not_available", 400, {
        provider: body.provider,
      });
    }
    if (body.provider !== "stripe") {
      // Registered but no checkout yet (W6b).
      throw new ApiError("provider_not_available", 400, {
        provider: body.provider,
      });
    }

    const price = await findPackageProviderPrice(
      client,
      body.packageId,
      body.provider
    );
    if (!price) {
      throw new ApiError("package_not_for_sale", 400, {
        reason: "no_provider_price",
      });
    }

    const active = await findActiveSubscriptionForUser(client, session.user.id);
    const checkout = await getStripeProvider().createCheckoutSession({
      userId: session.user.id,
      userEmail: session.user.email,
      packageId: pkg.id,
      packageName: pkg.name,
      providerPriceId: price.provider_price_id,
      successUrl: `${appBaseUrl()}/billing/success`,
      cancelUrl: `${appBaseUrl()}/billing/cancel`,
      customerId: active?.provider_customer_id ?? null,
    });

    await recordAudit(client, {
      userId: session.user.id,
      action: "billing.checkout",
      details: {
        provider: body.provider,
        package: pkg.name,
        country,
      },
    });
    return NextResponse.json({ url: checkout.url });
  } catch (err) {
    return errorResponse(err);
  }
}
