import test from "node:test";
import assert from "node:assert/strict";
import { hashPassword, verifyPassword } from "../src/password";

test("hashPassword verifies the same password", async () => {
  const hash = await hashPassword("correct horse battery staple");
  assert.equal(await verifyPassword("correct horse battery staple", hash), true);
});

test("hashPassword rejects a wrong password", async () => {
  const hash = await hashPassword("correct horse battery staple");
  assert.equal(await verifyPassword("wrong password", hash), false);
});

test("hashPassword rejects a malformed stored hash", async () => {
  assert.equal(await verifyPassword("anything", "not-a-hash"), false);
});

test("hashPassword uses Argon2id with a fresh salt each time", async () => {
  const a = await hashPassword("same-password");
  const b = await hashPassword("same-password");
  assert.notEqual(a, b);
  assert.match(a, /^\$argon2id\$v=19\$/);
});
