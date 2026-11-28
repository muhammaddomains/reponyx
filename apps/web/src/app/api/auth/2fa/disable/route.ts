import { NextResponse } from "next/server";
import { z } from "zod";
import { verifyPassword, verifyTotpCode } from "@reponyx/auth";
import { clearTotp, recordAudit } from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

const DisableBody = z.object({
  password: z.string().min(1).optional(),
  code: z.string().min(1).max(16).optional(),
});

/**
 * Disables 2FA: clears the secret and all backup codes. Proof is the
 * current password (password accounts) or a TOTP code (OAuth-only
 * accounts). Admins are refused — 2FA is mandatory for admin
 * accounts (plan §10).
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, DisableBody);
    const client = getDb();
    const { user } = await requireSession(client, req);
    if (!user.totp_secret) throw new ApiError("two_factor_not_enabled", 404);
    if (user.is_admin) throw new ApiError("admin_2fa_required", 403);
    if (user.has_password) {
      if (!body.password) throw new ApiError("invalid_body", 400);
      const ok = await verifyPassword(body.password, user.password_hash);
      if (!ok) throw new ApiError("invalid_credentials", 401);
    } else {
      if (!body.code) throw new ApiError("invalid_body", 400);
      if (!verifyTotpCode(user.totp_secret, body.code))
        throw new ApiError("invalid_2fa_code", 401);
    }
    await clearTotp(client, user.id);
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: user.id,
      action: "auth.2fa.disabled",
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ twoFactorEnabled: false });
  } catch (err) {
    return errorResponse(err);
  }
}
