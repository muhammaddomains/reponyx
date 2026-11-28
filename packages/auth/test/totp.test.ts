import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as OTPAuth from "otpauth";
import {
  generateTotpCode,
  generateTotpSecret,
  verifyTotpCode,
} from "../src/totp.ts";

describe("totp", () => {
  it("verifies a code generated from the same secret", () => {
    const secret = generateTotpSecret();
    const code = new OTPAuth.TOTP({
      secret: OTPAuth.Secret.fromBase32(secret),
    }).generate();
    assert.equal(verifyTotpCode(secret, code), true);
  });

  it("accepts spaces around the code", () => {
    const secret = generateTotpSecret();
    const code = new OTPAuth.TOTP({
      secret: OTPAuth.Secret.fromBase32(secret),
    }).generate();
    assert.equal(verifyTotpCode(secret, ` ${code} `), true);
  });

  it("rejects a wrong code", () => {
    const secret = generateTotpSecret();
    const code = new OTPAuth.TOTP({
      secret: OTPAuth.Secret.fromBase32(secret),
    }).generate();
    const wrong = code === "000000" ? "000001" : "000000";
    assert.equal(verifyTotpCode(secret, wrong), false);
  });

  it("rejects non-6-digit codes", () => {
    const secret = generateTotpSecret();
    assert.equal(verifyTotpCode(secret, "12345"), false);
    assert.equal(verifyTotpCode(secret, "1234567"), false);
    assert.equal(verifyTotpCode(secret, "abcdef"), false);
    assert.equal(verifyTotpCode(secret, ""), false);
  });

  it("rejects an invalid base32 secret", () => {
    assert.equal(verifyTotpCode("not!base32", "123456"), false);
  });

  it("generates distinct base32 secrets", () => {
    const a = generateTotpSecret();
    const b = generateTotpSecret();
    assert.notEqual(a, b);
    assert.match(a, /^[A-Z2-7]+$/);
  });

  it("generateTotpCode produces a code verifyTotpCode accepts", () => {
    const secret = generateTotpSecret();
    assert.equal(verifyTotpCode(secret, generateTotpCode(secret)), true);
  });
});

describe("totpSetupUrl", () => {
  it("builds a scannable otpauth URL for the user's email", async () => {
    const { totpSetupUrl } = await import("../src/totp.ts");
    const secret = generateTotpSecret();
    const url = totpSetupUrl(secret, "user@example.com");
    assert.match(url, /^otpauth:\/\/totp\/RepoNyx:user%40example\.com\?/);
    assert.match(url, new RegExp(`secret=${secret}`));
    assert.match(url, /issuer=RepoNyx/);
  });
});
