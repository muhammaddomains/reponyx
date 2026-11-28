import { NextResponse } from "next/server";
import { z } from "zod";
import { findPackageById, recordAudit, updatePackage } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const UpdateBody = z
  .object({
    name: z.string().min(1).max(64).optional(),
    maxLinkedAccounts: z.number().int().min(0).max(100000).optional(),
    maxDevices: z.number().int().min(0).max(100000).optional(),
    priceCents: z.number().int().min(0).max(100000000).optional(),
    isActive: z.boolean().optional(),
  })
  .refine((body) => Object.keys(body).length > 0, {
    message: "nothing to update",
  });

/**
 * Edits a package (limits, price, name, active). Retiring a package
 * (`isActive: false`) keeps it out of the public list without
 * deleting it — subscriptions reference it.
 */
export async function PATCH(
  req: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  try {
    const body = await parseJsonBody(req, UpdateBody);
    const { id } = await params;
    const client = getDb();
    const { user: admin } = await requireAdmin(client, req);
    const existing = await findPackageById(client, id);
    if (!existing) throw new ApiError("invalid_body", 400);
    let pkg;
    try {
      pkg = await updatePackage(client, id, {
        name: body.name,
        maxLinkedAccounts: body.maxLinkedAccounts,
        maxDevices: body.maxDevices,
        priceCents: body.priceCents,
        isActive: body.isActive,
      });
    } catch {
      throw new ApiError("invalid_body", 400, { field: "name" });
    }
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: admin.id,
      action: "admin.update_package",
      details: { packageId: id },
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ package: pkg });
  } catch (err) {
    return errorResponse(err);
  }
}
