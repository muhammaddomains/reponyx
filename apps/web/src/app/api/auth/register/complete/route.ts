import { NextResponse } from "next/server";
import { z } from "zod";
import { hashPassword, randomToken } from "@reponyx/auth";
import {
  createUser,
  findLinkedProviderByProviderUserId,
  findUserByEmail,
  markUserVerified,
  recordAudit,
  recordRegistrationAttempt,
  upsertLinkedProvider,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { createSessionForUser, sessionCookie } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { verifyPendingRegistrationToken } from "@/lib/oauth";
import { checkRegistrationAllowed, clientIp, hashIp } from "@/lib/rate-limits";

const CompleteBody = z.object({
  token: z.string().min(1),
  name: z.string().trim().min(1).max(128),
});

/**
 * Finishes an OAuth registration whose provider did not share a name.
 * The signed token proves the provider verified the email; the name
 * the user supplies here is mandatory, so the profile is complete
 * before the account exists and the login is allowed.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, CompleteBody);
    const pending = verifyPendingRegistrationToken(body.token);
    if (!pending) throw new ApiError("invalid_token", 404);
    const client = getDb();
    const ip = clientIp(req);
    const ipHash = ip ? hashIp(ip) : null;
    if (ipHash) await checkRegistrationAllowed(client, ipHash);
    const existing = await findUserByEmail(client, pending.email);
    if (existing) throw new ApiError("email_taken", 409);
    const linked = await findLinkedProviderByProviderUserId(
      client,
      pending.provider,
      pending.providerUserId
    );
    if (linked) throw new ApiError("already_linked", 409);
    const passwordHash = await hashPassword(randomToken());
    const user = await createUser(client, {
      email: pending.email,
      passwordHash,
      name: body.name,
      hasPassword: false,
    });
    await markUserVerified(client, user.id);
    await upsertLinkedProvider(client, {
      userId: user.id,
      provider: pending.provider,
      providerUserId: pending.providerUserId,
      email: pending.email,
    });
    if (ipHash) await recordRegistrationAttempt(client, ipHash);
    const { token, session } = await createSessionForUser(client, user, null);
    await recordAudit(client, {
      userId: user.id,
      action: "auth.oauth_register",
      ipHash,
    });
    const res = NextResponse.json(
      { user: { id: user.id, email: user.email } },
      { status: 201 }
    );
    res.headers.set("Set-Cookie", sessionCookie(token, session.expires_at));
    return res;
  } catch (err) {
    return errorResponse(err);
  }
}
