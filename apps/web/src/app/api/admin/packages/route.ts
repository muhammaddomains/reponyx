import { NextResponse } from "next/server";
import { z } from "zod";
import { createPackage, listAllPackages, recordAudit } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const CreateBody = z.object({
  name: z.string().min(1).max(64),
  maxLinkedAccounts: z.number().int().min(0).max(100000),
  maxDevices: z.number().int().min(0).max(100000),
  priceCents: z.number().int().min(0).max(100000000).default(0),
});

/** All packages, including retired ones — the admin manages the whole set. */
export async function GET(req: Request) {
  try {
    const client = getDb();
    await requireAdmin(client, req);
    const packages = await listAllPackages(client);
    return NextResponse.json({ packages });
  } catch (err) {
    return errorResponse(err);
  }
}

/** Creates a package. Names are unique; a duplicate is a 400. */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, CreateBody);
    const client = getDb();
    const { user: admin } = await requireAdmin(client, req);
    let pkg;
    try {
      pkg = await createPackage(client, body);
    } catch {
      throw new ApiError("invalid_body", 400, { field: "name" });
    }
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: admin.id,
      action: "admin.create_package",
      details: { packageId: pkg.id, name: pkg.name },
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ package: pkg }, { status: 201 });
  } catch (err) {
    return errorResponse(err);
  }
}
