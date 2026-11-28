import { NextResponse } from "next/server";
import {
  countDevicesForUser,
  countLinkedAccountsForUser,
  findActiveSubscriptionForUser,
  findUserById,
  listDevicesForUser,
  listLinkedAccountsForUser,
} from "@reponyx/database";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { requireAdmin } from "@/lib/auth";
import { getDb } from "@/lib/db";

/** One user for the admin: profile, package, devices, linked accounts. */
export async function GET(
  req: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  try {
    const { id } = await params;
    const client = getDb();
    await requireAdmin(client, req);
    const user = await findUserById(client, id);
    if (!user) throw new ApiError("account_not_found", 404);
    const [subscription, devices, accounts, linkedCount, deviceCount] =
      await Promise.all([
        findActiveSubscriptionForUser(client, user.id),
        listDevicesForUser(client, user.id),
        listLinkedAccountsForUser(client, user.id),
        countLinkedAccountsForUser(client, user.id),
        countDevicesForUser(client, user.id),
      ]);
    return NextResponse.json({
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        isAdmin: user.is_admin,
        emailVerified: user.email_verified,
        twoFactorEnabled: Boolean(user.totp_secret),
        locale: user.locale,
        createdAt: user.created_at,
      },
      subscription: subscription
        ? {
            packageId: subscription.package_id,
            packageName: subscription.package.name,
            maxLinkedAccounts: subscription.package.max_linked_accounts,
            maxDevices: subscription.package.max_devices,
            status: subscription.status,
            expiresAt: subscription.expires_at,
          }
        : null,
      linkedCount,
      deviceCount,
      devices: devices.map((device) => ({
        id: device.id,
        machineName: device.machine_name,
        platform: device.platform,
        lastSeenAt: device.last_seen_at,
        revokedAt: device.revoked_at,
      })),
      linkedAccounts: accounts.map((account) => ({
        id: account.id,
        login: account.login,
        authKind: account.auth_kind,
        createdAt: account.created_at,
      })),
    });
  } catch (err) {
    return errorResponse(err);
  }
}
