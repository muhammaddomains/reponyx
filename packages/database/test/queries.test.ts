import test from "node:test";
import assert from "node:assert/strict";
import type { DbClient } from "../src/client";
import {
  adminStats,
  clearTotp,
  confirmTotpSecret,
  consumeBackupCode,
  countBackupCodes,
  countLinkedAccountsForUser,
  countRecentRegistrations,
  createPackage,
  createSession,
  createUser,
  deleteLinkedProvider,
  findPackageById,
  findLinkedProviderByProviderUserId,
  findUserByEmail,
  findValidVerificationToken,
  listLinkedProvidersForUser,
  listUsersAdmin,
  recordRegistrationAttempt,
  replaceBackupCodes,
  revokeSession,
  setTotpPendingSecret,
  setUserPackage,
  updatePackage,
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

test("TOTP enrollment: pending secret is stored, confirm moves it live", async () => {
  const db = new FakeDbClient();
  db.on("setTotpPendingSecret", () => []);
  await setTotpPendingSecret(db, "u1", "SECRET");
  assert.equal(db.calls[0].params[1], "SECRET");
  assert.match(db.calls[0].text, /totp_pending_secret = \$2/);

  db.on("confirmTotpSecret", () => [
    { id: "u1", totp_secret: "SECRET", totp_pending_secret: null },
  ]);
  const confirmed = await confirmTotpSecret(db, "u1");
  assert.equal(confirmed?.totp_secret, "SECRET");
  assert.match(db.calls[1].text, /totp_pending_secret IS NOT NULL/);

  db.on("confirmTotpSecret", () => []);
  assert.equal(await confirmTotpSecret(db, "u1"), null);
});

test("clearTotp deletes backup codes and clears both secrets", async () => {
  const db = new FakeDbClient();
  db.on("clearTotp", () => []);
  await clearTotp(db, "u1");
  assert.match(db.calls[0].text, /DELETE FROM totp_backup_codes/);
  assert.match(db.calls[1].text, /totp_secret = NULL/);
});

test("backup codes: replace stores hashes, consume spends once", async () => {
  const db = new FakeDbClient();
  db.on("replaceBackupCodes", () => []);
  await replaceBackupCodes(db, "u1", ["h1", "h2"]);
  assert.match(db.calls[0].text, /DELETE FROM totp_backup_codes/);
  assert.equal(db.calls[1].params[1], "h1");
  assert.equal(db.calls[2].params[1], "h2");

  db.on("countBackupCodes", () => [{ count: "2" }]);
  assert.equal(await countBackupCodes(db, "u1"), 2);

  db.on("consumeBackupCode", () => [{ id: "c1", consumed_at: "now" }]);
  assert.equal(await consumeBackupCode(db, "u1", "h1"), true);
  db.on("consumeBackupCode", () => []);
  assert.equal(await consumeBackupCode(db, "u1", "h1"), false);
});

test("admin: listUsersAdmin passes search, limit and offset", async () => {
  const db = new FakeDbClient();
  db.on("listUsersAdmin", () => []);
  await listUsersAdmin(db, { q: "zaheer", limit: 50, offset: 0 });
  assert.equal(db.calls[0].params[0], "%zaheer%");
  assert.equal(db.calls[0].params[1], 50);
  assert.match(db.calls[0].text, /ILIKE \$1/);
  assert.match(db.calls[0].text, /linked_count/);
  assert.match(db.calls[0].text, /device_count/);
});

test("admin: createPackage, updatePackage and setUserPackage", async () => {
  const db = new FakeDbClient();
  db.on("createPackage", (params) => [
    {
      id: "p1",
      name: params[0],
      max_linked_accounts: params[1],
      max_devices: params[2],
      price_cents: params[3],
    },
  ]);
  const created = await createPackage(db, {
    name: "Test",
    maxLinkedAccounts: 3,
    maxDevices: 3,
    priceCents: 0,
  });
  assert.equal(created.name, "Test");
  assert.match(db.calls[0].text, /INSERT INTO packages/);

  db.on("updatePackage", () => [
    { id: "p1", name: "Test", max_linked_accounts: 5 },
  ]);
  const updated = await updatePackage(db, "p1", { maxLinkedAccounts: 5 });
  assert.equal(updated?.max_linked_accounts, 5);
  assert.match(db.calls[1].text, /max_linked_accounts = \$2/);

  db.on("findPackageById", () => [{ id: "p1", name: "Test" }]);
  const unchanged = await updatePackage(db, "p1", {});
  assert.equal(unchanged?.id, "p1");

  db.on("setUserPackage", () => [
    { user_id: "u1", package_id: "p1", status: "active" },
  ]);
  const sub = await setUserPackage(db, "u1", "p1", null);
  assert.equal(sub.status, "active");
  assert.match(db.calls[3].text, /ON CONFLICT \(user_id\) DO UPDATE/);
});

test("adminStats returns parsed totals", async () => {
  const db = new FakeDbClient();
  db.on("adminStats", () => [
    {
      users: "2",
      linked_accounts: "6",
      active_devices: "1",
      two_factor_users: "1",
    },
  ]);
  assert.deepEqual(await adminStats(db), {
    users: 2,
    linkedAccounts: 6,
    activeDevices: 1,
    twoFactorUsers: 1,
  });
});
