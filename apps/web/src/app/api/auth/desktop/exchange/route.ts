import { NextResponse } from "next/server";
import { z } from "zod";
import { findUserById, recordAudit, upsertDevice } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { createSessionForUser } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { verifyDesktopAuthCode } from "@/lib/oauth";
import { checkAccountSwitchAllowed, clientIp, hashIp } from "@/lib/rate-limits";

const ExchangeBody = z.object({
  code: z.string().min(1),
  deviceKey: z.string().max(128).optional(),
  machineName: z.string().max(128).optional(),
  platform: z.string().max(64).optional(),
  appVersion: z.string().max(64).optional(),
});

/**
 * Finishes the desktop OAuth flow: the desktop app received a
 * short-lived code through the `reponyx://auth` deep link and
 * exchanges it here for a session token. The raw session token is
 * never in the deep-link URL — only this code, which proves the
 * provider verified the account. No cookie is set: the desktop
 * stores the token in the OS credential store itself.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, ExchangeBody);
    const client = getDb();
    const userId = verifyDesktopAuthCode(body.code);
    if (!userId) throw new ApiError("invalid_token", 404);
    const user = await findUserById(client, userId);
    if (!user) throw new ApiError("invalid_token", 404);
    const ip = clientIp(req);
    const ipHash = ip ? hashIp(ip) : null;
    await checkAccountSwitchAllowed(client, {
      userId: user.id,
      deviceKey: body.deviceKey ?? null,
      ipHash,
    });
    let deviceId: string | null = null;
    if (body.deviceKey) {
      const device = await upsertDevice(client, {
        userId: user.id,
        deviceKey: body.deviceKey,
        machineName: body.machineName ?? null,
        platform: body.platform ?? null,
        appVersion: body.appVersion ?? null,
        lastIp: ip,
      });
      deviceId = device.id;
    }
    const { token, session } = await createSessionForUser(
      client,
      user,
      deviceId
    );
    await recordAudit(client, {
      userId: user.id,
      action: "auth.desktop_exchange",
      ipHash,
    });
    return NextResponse.json({
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        emailVerified: user.email_verified,
      },
      session: { token, expiresAt: session.expires_at },
    });
  } catch (err) {
    return errorResponse(err);
  }
}
