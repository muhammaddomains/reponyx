import { NextResponse } from "next/server";
import { z } from "zod";
import {
  findPackageById,
  findUserById,
  recordAudit,
  setUserPackage,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const AssignBody = z.object({
  packageId: z.string().min(1),
  expiresAt: z.string().nullable().optional(),
});

/**
 * Assigns a package to a user (upserts their subscription as active).
 * `expiresAt` null means no expiry. The admin's own assignment is
 * audited.
 */
export async function POST(
  req: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  try {
    const body = await parseJsonBody(req, AssignBody);
    const { id } = await params;
    const client = getDb();
    const { user: admin } = await requireAdmin(client, req);
    const user = await findUserById(client, id);
    if (!user) throw new ApiError("account_not_found", 404);
    const pkg = await findPackageById(client, body.packageId);
    if (!pkg) throw new ApiError("invalid_body", 400);
    const subscription = await setUserPackage(
      client,
      user.id,
      pkg.id,
      body.expiresAt ?? null
    );
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: admin.id,
      action: "admin.set_package",
      details: { targetUserId: user.id, packageId: pkg.id },
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({
      subscription: {
        packageId: subscription.package_id,
        status: subscription.status,
        expiresAt: subscription.expires_at,
      },
    });
  } catch (err) {
    return errorResponse(err);
  }
}
