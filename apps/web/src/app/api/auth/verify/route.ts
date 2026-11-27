import { NextResponse } from "next/server";
import { z } from "zod";
import { hashToken } from "@reponyx/auth";
import {
  consumeVerificationToken,
  findValidVerificationToken,
  markUserVerified,
  recordAudit,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const VerifyBody = z.object({ token: z.string().min(1) });

export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, VerifyBody);
    const client = getDb();
    const token = await findValidVerificationToken(client, hashToken(body.token));
    if (!token) throw new ApiError("invalid_token", 404);
    await markUserVerified(client, token.user_id);
    await consumeVerificationToken(client, token.id);
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: token.user_id,
      action: "auth.verify",
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ ok: true });
  } catch (err) {
    return errorResponse(err);
  }
}
