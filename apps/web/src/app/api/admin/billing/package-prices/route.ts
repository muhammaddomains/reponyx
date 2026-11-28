import { NextResponse } from "next/server";
import { z } from "zod";
import {
  findPackageById,
  listPackageProviderPrices,
  recordAudit,
  setPackageProviderPrice,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const SetPriceBody = z.object({
  packageId: z.string().uuid(),
  provider: z.enum(["stripe", "paypal", "polar", "freemius", "gumroad"]),
  providerPriceId: z.string().min(1).max(256),
});

/** Lists every package → provider price mapping (W6). */
export async function GET(req: Request) {
  try {
    const client = getDb();
    await requireAdmin(client, req);
    const prices = await listPackageProviderPrices(client);
    return NextResponse.json({ prices });
  } catch (err) {
    return errorResponse(err);
  }
}

/**
 * Sets the provider's price/product id for a package. A package
 * with no mapping for a provider cannot be sold through it.
 */
export async function PUT(req: Request) {
  try {
    const body = await parseJsonBody(req, SetPriceBody);
    const client = getDb();
    const { user: admin } = await requireAdmin(client, req);
    const pkg = await findPackageById(client, body.packageId);
    if (!pkg) throw new ApiError("package_not_found", 404);
    const price = await setPackageProviderPrice(
      client,
      body.packageId,
      body.provider,
      body.providerPriceId.trim()
    );
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: admin.id,
      action: "admin.set_package_provider_price",
      details: {
        packageId: body.packageId,
        provider: body.provider,
      },
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ price });
  } catch (err) {
    return errorResponse(err);
  }
}
