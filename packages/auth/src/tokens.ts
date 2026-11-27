import { createHash, randomBytes } from "crypto";

/** URL-safe random token (shown to the user once, never stored raw). */
export function randomToken(bytes = 32): string {
  return randomBytes(bytes).toString("base64url");
}

/** SHA-256 hex of a token — what the database stores. */
export function hashToken(token: string): string {
  return createHash("sha256").update(token, "utf8").digest("hex");
}

export function isExpired(expiresAt: Date, now = new Date()): boolean {
  return expiresAt.getTime() <= now.getTime();
}

/** `Date` N milliseconds from now (default: 24 hours). */
export function expiresIn(ms: number, now = new Date()): Date {
  return new Date(now.getTime() + ms);
}

export const DAY_MS = 24 * 60 * 60 * 1000;
export const HOUR_MS = 60 * 60 * 1000;
