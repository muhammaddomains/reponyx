import { NextResponse } from "next/server";
import { z } from "zod";
import { verifyPassword } from "@reponyx/auth";
import { findUserByEmail, recordAudit, upsertDevice } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import {
  createSessionForUser,
  DUMMY_PASSWORD_HASH,
  sessionCookie,
} from "@/lib/auth";
import { getDb } from "@/lib/db";
import { checkAccountSwitchAllowed, clientIp, hashIp } from "@/lib/rate-limits";

const LoginBody = z.object({
  email: z.email(),
  password: z.string().min(1),
  deviceKey: z.string().max(128).optional(),
  machineName: z.string().max(128).optional(),
  platform: z.string().max(64).optional(),
  appVersion: z.string().max(64).optional(),
});

export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, LoginBody);
    const client = getDb();
    const user = await findUserByEmail(client, body.email);
    // Verify against a dummy hash when the user does not exist, so a
    // wrong-email login costs the same as a wrong-password login.
    const passwordOk = await verifyPassword(
      body.password,
      user ? user.password_hash : DUMMY_PASSWORD_HASH
    );
    if (!user || !passwordOk) throw new ApiError("invalid_credentials", 401);
    if (!user.email_verified) throw new ApiError("email_not_verified", 403);
    const ip = clientIp(req);
    const ipHash = ip ? hashIp(ip) : null;
    await checkAccountSwitchAllowed(client, {
      userId: user.id,
      deviceKey: body.deviceKey ?? null,
      ipHash,
    });
    // 2FA step (W2/W3): the session is never returned until the code verifies.
    if (user.totp_secret) {
      return NextResponse.json({ step: "2fa_required" });
    }
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
