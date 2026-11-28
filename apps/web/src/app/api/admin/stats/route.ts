import { NextResponse } from "next/server";
import { adminStats } from "@reponyx/database";
import { errorResponse } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";

/** Totals for the admin home: users, linked accounts, active devices, 2FA users. */
export async function GET(req: Request) {
  try {
    const client = getDb();
    await requireAdmin(client, req);
    const stats = await adminStats(client);
    return NextResponse.json({ stats });
  } catch (err) {
    return errorResponse(err);
  }
}
