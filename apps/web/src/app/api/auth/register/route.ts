import { NextResponse } from "next/server";
import { z } from "zod";
import {
  DAY_MS,
  expiresIn,
  hashPassword,
  hashToken,
  randomToken,
} from "@reponyx/auth";
import {
  createEmailVerificationToken,
  createUser,
  deleteUser,
  findUserByEmail,
  recordAudit,
  recordRegistrationAttempt,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { getDb } from "@/lib/db";
import { sendVerificationEmail } from "@/lib/email";
import { checkRegistrationAllowed, clientIp, hashIp } from "@/lib/rate-limits";

const RegisterBody = z.object({
  email: z.email(),
  password: z.string().min(8).max(128),
});

export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, RegisterBody);
    const client = getDb();
    const ip = clientIp(req);
    const ipHash = ip ? hashIp(ip) : null;
    if (ipHash) await checkRegistrationAllowed(client, ipHash);
    const existing = await findUserByEmail(client, body.email);
    if (existing) throw new ApiError("email_taken", 409);
    const passwordHash = await hashPassword(body.password);
    const user = await createUser(client, { email: body.email, passwordHash });
    const token = randomToken();
    await createEmailVerificationToken(client, {
      userId: user.id,
      tokenHash: hashToken(token),
      expiresAt: expiresIn(DAY_MS),
    });
    try {
      await sendVerificationEmail(user.email, token);
    } catch (err) {
      // No dangling unverified user when the email never went out.
      await deleteUser(client, user.id);
      console.error("verification email failed", err);
      throw new ApiError("email_unavailable", 503);
    }
    if (ipHash) await recordRegistrationAttempt(client, ipHash);
    await recordAudit(client, {
      userId: user.id,
      action: "auth.register",
      ipHash,
    });
    return NextResponse.json(
      { user: { id: user.id, email: user.email } },
      { status: 201 }
    );
  } catch (err) {
    return errorResponse(err);
  }
}
