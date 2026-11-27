import * as OTPAuth from "otpauth";

/**
 * TOTP (authenticator-app codes) for the web 2FA step.
 *
 * Codes are 6 digits, 30-second steps, SHA-1 — the defaults every
 * authenticator app speaks. Verification allows one step of clock
 * skew either way. The secret is the base32 string stored in
 * `users.totp_secret` (set at enrollment; verification only reads
 * it, never logs it).
 */
export function verifyTotpCode(secret: string, code: string): boolean {
  const normalized = code.replace(/\s+/g, "");
  if (!/^\d{6}$/.test(normalized)) return false;
  let totp: OTPAuth.TOTP;
  try {
    totp = new OTPAuth.TOTP({
      secret: OTPAuth.Secret.fromBase32(secret),
    });
  } catch {
    return false;
  }
  return totp.validate({ token: normalized, window: 1 }) !== null;
}

/** A fresh base32 secret for TOTP enrollment (20 random bytes). */
export function generateTotpSecret(): string {
  return new OTPAuth.Secret({ size: 20 }).base32;
}

/** The current code for a secret (test helper; also what an enrollment check uses). */
export function generateTotpCode(secret: string): string {
  return new OTPAuth.TOTP({
    secret: OTPAuth.Secret.fromBase32(secret),
  }).generate();
}
