import { createHash } from "crypto";
import { DAY_MS, HOUR_MS } from "@reponyx/auth";
import {
  countRecentRegistrations,
  findOldestRecentRegistration,
  findRecentLogoutForDevice,
  findRecentLogoutForIp,
  type DbClient,
} from "@reponyx/database";
import { ApiError } from "./api-errors";

/** First public IP from the proxy headers (Vercel sets x-forwarded-for). */
export function clientIp(req: Request): string | null {
  const forwarded = req.headers.get("x-forwarded-for");
  if (forwarded) return forwarded.split(",")[0].trim();
  return req.headers.get("x-real-ip");
}

/** IPs are never stored raw — only their SHA-256 hash. */
export function hashIp(ip: string): string {
  return createHash("sha256").update(ip, "utf8").digest("hex");
}

/** Registration: 1 per IP per 24 hours (plan §7). Counts only successful registrations. */
export async function checkRegistrationAllowed(
  client: DbClient,
  ipHash: string,
): Promise<void> {
  const since = new Date(Date.now() - DAY_MS);
  const count = await countRecentRegistrations(client, { ipHash, since });
  if (count < 1) return;
  const oldest = await findOldestRecentRegistration(client, { ipHash, since });
  const retryAt = oldest
    ? new Date(new Date(oldest.created_at).getTime() + DAY_MS)
    : new Date(Date.now() + DAY_MS);
  throw new ApiError("registration_rate_limited", 429, { retry_at: retryAt.toISOString() });
}

/**
 * Account-switch: 1 hour, device-first, IP fallback (plan §7). After a
 * logout, the SAME account can log back in immediately; a DIFFERENT
 * account from the same device cannot log in for 1 hour.
 */
export async function checkAccountSwitchAllowed(
  client: DbClient,
  args: { userId: string; deviceKey: string | null; ipHash: string | null },
): Promise<void> {
  const since = new Date(Date.now() - HOUR_MS);
  const byDevice = args.deviceKey
    ? await findRecentLogoutForDevice(client, { deviceKey: args.deviceKey, since })
    : null;
  const byIp =
    !byDevice && args.ipHash
      ? await findRecentLogoutForIp(client, { ipHash: args.ipHash, since })
      : null;
  const recent = byDevice ?? byIp;
  if (recent && recent.user_id !== args.userId) {
    const retryAt = new Date(new Date(recent.created_at).getTime() + HOUR_MS);
    throw new ApiError("account_switch_blocked", 403, { retry_at: retryAt.toISOString() });
  }
}
