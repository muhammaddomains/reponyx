import { NextResponse } from "next/server";
import { z } from "zod";
import { HOUR_MS, expiresIn, hashToken, randomToken } from "@reponyx/auth";
import {
  createPasswordResetToken,
  findUserByEmail,
  recordAudit,
} from "@reponyx/database";
import { errorResponse, parseJsonBody } from "@/lib/api-errors";
import { getDb } from "@/lib/db";
import { sendPasswordResetEmail } from "@/lib/email";
import { clientIp, hashIp } from "@/lib/rate-limits";

const ResetBody = z.object({ email: z.email() });

export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, ResetBody);
    const client = getDb();
    const user = await findUserByEmail(client, body.email);
    if (user) {
      const token = randomToken();
      await createPasswordResetToken(client, {
        userId: user.id,
        tokenHash: hashToken(token),
        expiresAt: expiresIn(HOUR_MS),
      });
      await sendPasswordResetEmail(user.email, token);
      const ip = clientIp(req);
      await recordAudit(client, {
        userId: user.id,
        action: "auth.reset_requested",
        ipHash: ip ? hashIp(ip) : null,
      });
    }
    // Always the same answer — no user enumeration.
    return NextResponse.json({ ok: true });
  } catch (err) {
    return errorResponse(err);
  }
}
