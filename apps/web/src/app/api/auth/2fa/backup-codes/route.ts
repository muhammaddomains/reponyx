import { NextResponse } from "next/server";
import { z } from "zod";
import {
  generateBackupCodes,
  hashToken,
  normalizeBackupCode,
  verifyTotpCode,
} from "@reponyx/auth";
import { recordAudit, replaceBackupCodes } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const RegenerateBody = z.object({
  code: z.string().min(1).max(16),
});

/**
 * Issues a fresh set of backup codes. The caller proves possession
 * of the authenticator with a current TOTP code; the old codes stop
 * working immediately. Plaintext codes are returned exactly once.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, RegenerateBody);
    const client = getDb();
    const { user } = await requireSession(client, req);
    if (!user.totp_secret) throw new ApiError("two_factor_not_enabled", 404);
    if (!verifyTotpCode(user.totp_secret, body.code))
      throw new ApiError("invalid_2fa_code", 401);
    const backupCodes = generateBackupCodes();
    // Hash the normalized form (no dash, upper-case) so a code typed
    // with or without the dash, in any case, verifies.
    await replaceBackupCodes(
      client,
      user.id,
      backupCodes.map((code) => hashToken(normalizeBackupCode(code)))
    );
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: user.id,
      action: "auth.2fa.backup_codes_regenerated",
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({
      backupCodes,
      remaining: backupCodes.length,
    });
  } catch (err) {
    return errorResponse(err);
  }
}
