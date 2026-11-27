import { NextResponse } from "next/server";
import { listLinkedProvidersForUser } from "@reponyx/database";
import { errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";

/** The providers linked to the signed-in account. */
export async function GET(req: Request) {
  try {
    const client = getDb();
    const { user } = await requireSession(client, req);
    const linked = await listLinkedProvidersForUser(client, user.id);
    return NextResponse.json({
      linked: linked.map((row) => ({
        provider: row.provider,
        email: row.email,
        createdAt: row.created_at,
      })),
    });
  } catch (err) {
    return errorResponse(err);
  }
}
