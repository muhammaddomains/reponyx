import test from "node:test";
import assert from "node:assert/strict";
import type { DbClient } from "../src/client";
import {
  countLinkedAccountsForUser,
  countRecentRegistrations,
  createSession,
  createUser,
  deleteLinkedProvider,
  findLinkedProviderByProviderUserId,
  findUserByEmail,
  findValidVerificationToken,
  listLinkedProvidersForUser,
  recordRegistrationAttempt,
  revokeSession,
  upsertDevice,
  upsertLinkedProvider,
} from "../src/queries";

/** Test double: dispatches on the marker comment in each statement. */
class FakeDbClient implements DbClient {
  calls: { text: string; params: unknown[] }[] = [];
  handlers = new Map<string, (params: unknown[]) => unknown[]>();

  on(marker: string, fn: (params: unknown[]) => unknown[]): void {
    this.handlers.set(marker, fn);
  }

  async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
    this.calls.push({ text, params });
    const marker = text.match(/\/\*\s*(\w+)\s*\*\//)?.[1];
    const handler = marker ? this.handlers.get(marker) : undefined;
    if (!handler)
      throw new Error(`no fake handler for query: ${text.slice(0, 80)}`);
    return handler(params) as T[];
  }
}

test("createUser lowercases the email and returns the row", async () => {
  const db = new FakeDbClient();
  db.on("createUser", (params) => [
    {
      id: "u1",
      email: params[0],
      password_hash: params[1],
      name: params[2],
      email_verified: false,
    },
  ]);
  const row = await createUser(db, {
    email: "USER@Example.com",
    passwordHash: "hash",
    name: "Test User",
  });
  assert.equal(row.email, "user@example.com");
  assert.equal(row.name, "Test User");
  assert.match(db.calls[0].text, /INSERT INTO users/);
});

test("findUserByEmail returns null when absent, the row when present", async () => {
  const db = new FakeDbClient();
  db.on("findUserByEmail", () => []);
  assert.equal(await findUserByEmail(db, "a@example.com"), null);
  db.on("findUserByEmail", () => [{ id: "u1", email: "a@example.com" }]);
  assert.equal((await findUserByEmail(db, "A@Example.com"))?.id, "u1");
  assert.equal(db.calls[1].params[0], "a@example.com");
});

test("findValidVerificationToken passes the token hash through", async () => {
  const db = new FakeDbClient();
  db.on("findValidVerificationToken", (params) => [
    { id: "t1", user_id: "u1", token_hash: params[0] },
  ]);
  const row = await findValidVerificationToken(db, "deadbeef");
  assert.equal(row?.token_hash, "deadbeef");
  assert.match(db.calls[0].text, /consumed_at IS NULL/);
});

test("createSession then revokeSession issue the right statements", async () => {
  const db = new FakeDbClient();
  db.on("createSession", () => [{ id: "s1" }]);
  db.on("revokeSession", () => []);
  await createSession(db, {
    userId: "u1",
    tokenHash: "tok",
    deviceId: null,
    expiresAt: new Date("2026-12-26T00:00:00Z"),
  });
  await revokeSession(db, "tok");
  assert.match(db.calls[0].text, /INSERT INTO sessions/);
  assert.match(db.calls[1].text, /DELETE FROM sessions/);
  assert.equal(db.calls[1].params[0], "tok");
});

test("countLinkedAccountsForUser parses the count", async () => {
  const db = new FakeDbClient();
  db.on("countLinkedAccountsForUser", () => [{ count: "3" }]);
  assert.equal(await countLinkedAccountsForUser(db, "u1"), 3);
});

test("registration limit counts recent attempts for the IP hash", async () => {
  const db = new FakeDbClient();
  db.on("recordRegistrationAttempt", () => []);
  db.on("countRecentRegistrations", () => [{ count: "1" }]);
  await recordRegistrationAttempt(db, "iphash");
  assert.equal(
    await countRecentRegistrations(db, { ipHash: "iphash", since: new Date() }),
    1
  );
  assert.match(db.calls[1].text, /registration_attempts/);
});

test("linked providers round-trip through upsert, find, list, delete", async () => {
  const db = new FakeDbClient();
  const stored: Record<string, unknown>[] = [];
  db.on("upsertLinkedProvider", (params) => {
    const row = {
      id: "lp1",
      user_id: params[0],
      provider: params[1],
      provider_user_id: params[2],
      email: params[3],
    };
    stored.push(row);
    return [row];
  });
  db.on("findLinkedProviderByProviderUserId", (params) => {
    return stored.filter(
      (r) => r.provider === params[0] && r.provider_user_id === params[1]
    );
  });
  db.on("listLinkedProvidersForUser", (params) => {
    return stored.filter((r) => r.user_id === params[0]);
  });
  db.on("deleteLinkedProvider", (params) => {
    const i = stored.findIndex(
      (r) => r.user_id === params[0] && r.provider === params[1]
    );
    if (i >= 0) stored.splice(i, 1);
    return [];
  });

  const row = await upsertLinkedProvider(db, {
    userId: "u1",
    provider: "google",
    providerUserId: "google-sub-1",
    email: "USER@Example.com",
  });
  assert.equal(row.email, "user@example.com");
  assert.match(db.calls[0].text, /ON CONFLICT \(user_id, provider\)/);

  const found = await findLinkedProviderByProviderUserId(
    db,
    "google",
    "google-sub-1"
  );
  assert.equal(found?.user_id, "u1");
  assert.equal(
    await findLinkedProviderByProviderUserId(db, "google", "other-sub"),
    null
  );

  const listed = await listLinkedProvidersForUser(db, "u1");
  assert.equal(listed.length, 1);

  await deleteLinkedProvider(db, "u1", "google");
  assert.equal((await listLinkedProvidersForUser(db, "u1")).length, 0);
});

test("upsertDevice clears revoked_at so a running desktop re-activates", async () => {
  const db = new FakeDbClient();
  db.on("upsertDevice", () => [
    {
      id: "d1",
      user_id: "u1",
      device_key: "dev-1",
      machine_name: "DESKTOP-TEST",
      platform: "Windows 11",
      app_version: "0.1.0",
      last_ip: null,
      last_seen_at: "2026-11-28T14:00:00.000Z",
      revoked_at: null,
      created_at: "2026-11-28T10:00:00.000Z",
    },
  ]);
  const row = await upsertDevice(db, {
    userId: "u1",
    deviceKey: "dev-1",
    machineName: "DESKTOP-TEST",
    platform: "Windows 11",
    appVersion: "0.1.0",
    lastIp: null,
  });
  assert.equal(row.revoked_at, null);
  assert.match(db.calls[0].text, /ON CONFLICT \(user_id, device_key\)/);
  assert.match(db.calls[0].text, /revoked_at = NULL/);
});
