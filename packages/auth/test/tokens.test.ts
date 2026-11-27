import test from "node:test";
import assert from "node:assert/strict";
import { DAY_MS, expiresIn, hashToken, isExpired, randomToken } from "../src/tokens";

test("randomToken is URL-safe and unique", () => {
  const a = randomToken();
  const b = randomToken();
  assert.notEqual(a, b);
  assert.match(a, /^[A-Za-z0-9_-]+$/);
  assert.equal(a.length, 43);
});

test("hashToken is deterministic SHA-256 hex", () => {
  assert.equal(hashToken("abc"), hashToken("abc"));
  assert.match(hashToken("abc"), /^[0-9a-f]{64}$/);
  assert.notEqual(hashToken("abc"), hashToken("abd"));
});

test("isExpired and expiresIn agree", () => {
  const now = new Date("2026-11-26T07:00:00Z");
  assert.equal(isExpired(expiresIn(DAY_MS, now), now), false);
  assert.equal(isExpired(expiresIn(-1, now), now), true);
});
