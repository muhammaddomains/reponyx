import { NextResponse } from "next/server";
import { z } from "zod";
import { hashPassword, hashToken } from "@reponyx/auth";
import {
  consumeResetToken,
  findValidResetToken,
  recordAudit,
  revokeAllSessionsForUser,
  setUserPassword,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const ResetConfirmBody = z.object({
  token: z.string().min(1),
  password: z.string().min(8).max(128),
});

export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, ResetConfirmBody);
    const client = getDb();
    const token = await findValidResetToken(client, hashToken(body.token));
    if (!token) throw new ApiError("invalid_token", 404);
    // Claim the token first: the atomic consume fails when another
    // request already used this link, so one link resets the
    // password exactly once.
    const claimed = await consumeResetToken(client, token.id);
    if (!claimed) throw new ApiError("invalid_token", 404);
    await setUserPassword(client, {
      userId: token.user_id,
      passwordHash: await hashPassword(body.password),
    });
    // A password reset logs every session out.
    await revokeAllSessionsForUser(client, token.user_id);
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: token.user_id,
      action: "auth.reset_confirmed",
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ ok: true });
  } catch (err) {
    return errorResponse(err);
  }
}
