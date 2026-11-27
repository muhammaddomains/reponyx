import { NextResponse } from "next/server";
import {
  countDevicesForUser,
  countLinkedAccountsForUser,
  findActiveSubscriptionForUser,
  findPackageByName,
} from "@reponyx/database";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";

export async function GET(req: Request) {
  try {
    const client = getDb();
    const { user } = await requireSession(client, req);
    const subscription = await findActiveSubscriptionForUser(client, user.id);
    const pkg = subscription?.package ?? (await findPackageByName(client, "Free"));
    if (!pkg) throw new ApiError("not_configured", 503);
    const [linkedAccounts, devices] = await Promise.all([
      countLinkedAccountsForUser(client, user.id),
      countDevicesForUser(client, user.id),
    ]);
    return NextResponse.json({
      user: { id: user.id, email: user.email, locale: user.locale, createdAt: user.created_at },
      package: {
        name: pkg.name,
        maxLinkedAccounts: pkg.max_linked_accounts,
        maxDevices: pkg.max_devices,
        expiresAt: subscription?.expires_at ?? null,
      },
      usage: { linkedAccounts, devices },
    });
  } catch (err) {
    return errorResponse(err);
  }
}
