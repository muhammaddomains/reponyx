import { NextResponse } from "next/server";
import { z } from "zod";
import { isProviderConfigured } from "@reponyx/billing";
import {
  listBillingProviders,
  recordAudit,
  setBillingProviderActive,
  setBillingProviderCountries,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const UpdateBody = z
  .object({
    provider: z.enum(["stripe", "paypal", "polar", "freemius", "gumroad"]),
    isActive: z.boolean().optional(),
    /** Country codes, or null = worldwide. */
    countries: z
      .array(z.string().regex(/^[A-Za-z]{2}$/))
      .max(250)
      .nullable()
      .optional(),
  })
  .refine(
    (body) => body.isActive !== undefined || body.countries !== undefined,
    { message: "nothing to update" }
  );

/** Lists the billing providers with their configured state (W6). */
export async function GET(req: Request) {
  try {
    const client = getDb();
    await requireAdmin(client, req);
    const providers = await listBillingProviders(client);
    return NextResponse.json({
      providers: providers.map((row) => ({
        provider: row.provider,
        isActive: row.is_active,
        countries: row.countries,
        configured: isProviderConfigured(
          row.provider as "stripe" | "paypal" | "polar" | "freemius" | "gumroad"
        ),
      })),
    });
  } catch (err) {
    return errorResponse(err);
  }
}

/**
 * Toggles a provider or sets its country scope. A provider can only
 * be enabled when its env credentials exist (plan §9) — without them
 * it stays disabled and never appears at checkout. Keys are never
 * shown — only whether they are set.
 */
export async function PATCH(req: Request) {
  try {
    const body = await parseJsonBody(req, UpdateBody);
    const client = getDb();
    const { user: admin } = await requireAdmin(client, req);
    if (body.isActive === true && !isProviderConfigured(body.provider)) {
      throw new ApiError("billing_not_configured", 400, {
        provider: body.provider,
      });
    }
    let row = null;
    if (body.isActive !== undefined) {
      row = await setBillingProviderActive(
        client,
        body.provider,
        body.isActive
      );
    }
    if (body.countries !== undefined) {
      row = await setBillingProviderCountries(
        client,
        body.provider,
        body.countries === null
          ? null
          : body.countries.map((code) => code.toUpperCase())
      );
    }
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: admin.id,
      action: "admin.update_billing_provider",
      details: {
        provider: body.provider,
        isActive: body.isActive ?? undefined,
        countries: body.countries ?? undefined,
      },
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ provider: row });
  } catch (err) {
    return errorResponse(err);
  }
}
