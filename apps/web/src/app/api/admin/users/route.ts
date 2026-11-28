import { NextResponse } from "next/server";
import { countUsersAdmin, listUsersAdmin } from "@reponyx/database";
import { errorResponse } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";

/**
 * Every user, newest first, with package and usage counts.
 * `?q=` searches email and name; `?limit=` (default 50, max 200)
 * and `?offset=` page through the list.
 */
export async function GET(req: Request) {
  try {
    const client = getDb();
    await requireAdmin(client, req);
    const url = new URL(req.url);
    const q = url.searchParams.get("q") ?? undefined;
    const limit = Math.min(
      Math.max(Number(url.searchParams.get("limit") ?? 50) || 50, 1),
      200
    );
    const offset = Math.max(
      Number(url.searchParams.get("offset") ?? 0) || 0,
      0
    );
    const [users, total] = await Promise.all([
      listUsersAdmin(client, { q, limit, offset }),
      countUsersAdmin(client, q),
    ]);
    return NextResponse.json({
      users: users.map((user) => ({
        id: user.id,
        email: user.email,
        name: user.name,
        isAdmin: user.is_admin,
        emailVerified: user.email_verified,
        twoFactorEnabled: Boolean(user.totp_secret),
        // No subscription row (or an expired one) means the Free
        // fallback, same as the limits API and the dashboard.
        packageName: user.package_name ?? "Free",
        subscriptionExpiresAt: user.subscription_expires_at,
        linkedCount: Number(user.linked_count),
        deviceCount: Number(user.device_count),
        createdAt: user.created_at,
      })),
      total,
      limit,
      offset,
    });
  } catch (err) {
    return errorResponse(err);
  }
}
