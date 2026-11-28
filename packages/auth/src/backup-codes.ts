import { randomBytes } from "crypto";

/**
 * One-time backup codes for TOTP login when the authenticator app is
 * unavailable. A code is 10 characters from an unambiguous alphabet
 * (no 0/O, 1/I/L), shown as `XXXXX-XXXXX`. Only SHA-256 hashes are
 * stored (see `hashToken`); the plaintext codes are shown exactly
 * once, at enrollment or regeneration.
 */

const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

export const BACKUP_CODE_COUNT = 10;

function oneCode(): string {
  const bytes = randomBytes(10);
  let raw = "";
  for (const byte of bytes) raw += ALPHABET[byte % ALPHABET.length];
  return `${raw.slice(0, 5)}-${raw.slice(5)}`;
}

/** A fresh set of backup codes (plaintext — hash before storing). */
export function generateBackupCodes(count = BACKUP_CODE_COUNT): string[] {
  const codes = new Set<string>();
  while (codes.size < count) codes.add(oneCode());
  return [...codes];
}

/** What the user types, normalized for comparison: no dashes/spaces, upper-case. */
export function normalizeBackupCode(code: string): string {
  return code.replace(/[\s-]+/g, "").toUpperCase();
}
