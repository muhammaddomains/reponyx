import test from "node:test";
import assert from "node:assert/strict";
import { passwordResetEmail, verificationEmail } from "../src/templates";

test("verification email contains the link and the 24h expiry", () => {
  const url = "https://example.com/auth/verify?token=abc";
  const mail = verificationEmail(url);
  assert.match(mail.subject, /Verify/);
  assert.ok(mail.html.includes(url));
  assert.ok(mail.text.includes(url));
  assert.match(mail.html, /24 hours/);
});

test("password reset email contains the link and the 1h expiry", () => {
  const url = "https://example.com/auth/reset/confirm?token=abc";
  const mail = passwordResetEmail(url);
  assert.match(mail.subject, /Reset/);
  assert.ok(mail.html.includes(url));
  assert.match(mail.text, /1 hour/);
});
