import { NextResponse } from "next/server";
import QRCode from "qrcode";
import { generateTotpSecret, totpSetupUrl } from "@reponyx/auth";
import { recordAudit, setTotpPendingSecret } from "@reponyx/database";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

/**
 * Starts TOTP enrollment. Generates a fresh secret, holds it
 * server-side as the user's pending secret, and returns the
 * `otpauth://` URL (for manual entry) plus a QR image of it. The
 * secret becomes live only when the user confirms it with a code
 * (POST /api/auth/2fa/confirm). Refused when 2FA is already on.
 */
export async function POST(req: Request) {
  try {
    const client = getDb();
    const { user } = await requireSession(client, req);
    if (user.totp_secret) throw new ApiError("two_factor_already_enabled", 409);
    const secret = generateTotpSecret();
    await setTotpPendingSecret(client, user.id, secret);
    const otpauthUrl = totpSetupUrl(secret, user.email);
    const qrDataUrl = await QRCode.toDataURL(otpauthUrl);
    const ip = clientIp(req);
    await recordAudit(client, {
      userId: user.id,
      action: "auth.2fa.setup",
      ipHash: ip ? hashIp(ip) : null,
    });
    return NextResponse.json({ otpauthUrl, qrDataUrl, secret });
  } catch (err) {
    return errorResponse(err);
  }
}
