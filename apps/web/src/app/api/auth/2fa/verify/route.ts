import { NextResponse } from "next/server";
import { z } from "zod";
import { verifyTotpCode } from "@reponyx/auth";
import { findUserById, recordAudit, upsertDevice } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import {
  createSessionForUser,
  sessionCookie,
  verifyPendingLoginToken,
} from "@/lib/auth";
import { getDb } from "@/lib/db";
import { checkAccountSwitchAllowed, clientIp, hashIp } from "@/lib/rate-limits";

const VerifyBody = z.object({
  token: z.string().min(1),
  code: z.string().min(1).max(16),
  deviceKey: z.string().max(128).optional(),
  machineName: z.string().max(128).optional(),
  platform: z.string().max(64).optional(),
  appVersion: z.string().max(64).optional(),
});

/**
 * Completes a login that stopped at the 2FA step. The pending token
 * proves the password step passed (password login) or the provider
 * verified the account (OAuth login); the authenticator code is
 * checked against the stored TOTP secret, and only then is the
 * session created — never before.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, VerifyBody);
    const client = getDb();
    const userId = verifyPendingLoginToken(body.token);
    if (!userId) throw new ApiError("invalid_token", 404);
    const user = await findUserById(client, userId);
    if (!user || !user.totp_secret) throw new ApiError("invalid_token", 404);
    if (!verifyTotpCode(user.totp_secret, body.code)) {
      throw new ApiError("invalid_2fa_code", 401);
    }
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
      action: "auth.login",
      ipHash,
    });
    const res = NextResponse.json({
      user: {
        id: user.id,
        email: user.email,
        emailVerified: user.email_verified,
      },
      session: { token, expiresAt: session.expires_at },
    });
    res.headers.set("Set-Cookie", sessionCookie(token, session.expires_at));
    return res;
  } catch (err) {
    return errorResponse(err);
  }
}
