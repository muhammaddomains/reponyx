import { NextResponse } from "next/server";
import { z } from "zod";
import {
  generateBackupCodes,
  hashToken,
  normalizeBackupCode,
  verifyTotpCode,
} from "@reponyx/auth";
import {
  confirmTotpSecret,
  findUserById,
  recordAudit,
  replaceBackupCodes,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const ConfirmBody = z.object({
  code: z.string().min(1).max(16),
});

/**
 * Completes TOTP enrollment: the code must match the pending secret.
 * The pending secret becomes the live one, and a fresh set of backup
 * codes is issued — the plaintext codes are returned exactly once,
 * only their hashes are stored.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, ConfirmBody);
    const client = getDb();
    const { user } = await requireSession(client, req);
    if (!user.totp_pending_secret) throw new ApiError("no_pending_2fa", 404);
    if (!verifyTotpCode(user.totp_pending_secret, body.code))
      throw new ApiError("invalid_2fa_code", 401);
    const confirmed = await confirmTotpSecret(client, user.id);
    if (!confirmed) throw new ApiError("no_pending_2fa", 404);
    const backupCodes = generateBackupCodes();
    // Hash the normalized form (no dash, upper-case) so a code typed
    // with or without the dash, in any case, verifies.
    await replaceBackupCodes(
      client,
      user.id,
      backupCodes.map((code) => hashToken(normalizeBackupCode(code)))
    );
    const fresh = await findUserById(client, user.id);
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: user.id,
      action: "auth.2fa.enabled",
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({
      backupCodes,
      remaining: backupCodes.length,
      twoFactorEnabled: Boolean(fresh?.totp_secret),
    });
  } catch (err) {
    return errorResponse(err);
  }
}
