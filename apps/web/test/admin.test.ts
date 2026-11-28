import test from "node:test";
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import { generateTotpCode } from "@reponyx/auth";
import { migrate, type DbClient } from "@reponyx/database";
import type { EmailMessage, SentEmail } from "@reponyx/email";
import { setDbClientForTests } from "../src/lib/db";
import { setEmailSenderForTests } from "../src/lib/email";
import { POST as registerPost } from "../src/app/api/auth/register/route";
import { POST as verifyPost } from "../src/app/api/auth/verify/route";
import { POST as loginPost } from "../src/app/api/auth/login/route";
import { POST as logoutPost } from "../src/app/api/auth/logout/route";
import { POST as setupPost } from "../src/app/api/auth/2fa/setup/route";
import { POST as confirmPost } from "../src/app/api/auth/2fa/confirm/route";
import { POST as disablePost } from "../src/app/api/auth/2fa/disable/route";
import { POST as backupCodesPost } from "../src/app/api/auth/2fa/backup-codes/route";
import { POST as verify2faPost } from "../src/app/api/auth/2fa/verify/route";
import { GET as statsGet } from "../src/app/api/admin/stats/route";
import { GET as usersGet } from "../src/app/api/admin/users/route";
import { GET as userGet } from "../src/app/api/admin/users/[id]/route";
import { POST as assignPost } from "../src/app/api/admin/users/[id]/package/route";
import {
  GET as packagesGet,
  POST as packagesPost,
} from "../src/app/api/admin/packages/route";
import { PATCH as packagePatch } from "../src/app/api/admin/packages/[id]/route";

/**
 * W5: TOTP enrollment + admin dashboard against real Postgres
 * (PGlite) and the real route handlers. The only fakes are the email
 * transport and the database client (a PGlite adapter).
 */

process.env.SESSION_SECRET = "test-session-secret-for-w5-admin-tests";

const sent: EmailMessage[] = [];
const db = new PGlite();
const client: DbClient = {
  async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
    if (
      params.length === 0 &&
      text.split(";").filter((s) => s.trim()).length > 1
    ) {
      await db.exec(text);
      return [];
    }
    const result = await db.query(text, params as never[]);
    return result.rows as T[];
  },
};

const migrationsDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
  "packages",
  "database",
  "migrations"
);
const files = (await readdir(migrationsDir))
  .filter((f) => f.endsWith(".sql"))
  .sort();
const migrations = [];
for (const file of files) {
  migrations.push({
    version: file.replace(/\.sql$/, ""),
    sql: await readFile(path.join(migrationsDir, file), "utf8"),
  });
}
const applied = await migrate(client, migrations);
assert.deepEqual(applied, [
  "0001_init",
  "0002_seed_packages",
  "0003_add_user_name",
  "0004_linked_providers",
  "0005_totp_backup_codes",
]);

setDbClientForTests(client);
setEmailSenderForTests({
  provider: "resend",
  async send(message: EmailMessage): Promise<SentEmail> {
    sent.push(message);
    return { messageId: "test-1", provider: "resend" };
  },
});

interface CallResult {
  status: number;
  json: Record<string, unknown>;
  headers: Headers;
}

async function call(
  handler: (
    req: Request,
    ctx: { params: Promise<{ id: string }> }
  ) => Promise<Response>,
  path: string,
  init: {
    method?: string;
    body?: unknown;
    headers?: Record<string, string>;
    params?: { id: string };
  }
): Promise<CallResult> {
  const res = await handler(
    new Request(`http://localhost${path}`, {
      method: init.method ?? "POST",
      headers: { "Content-Type": "application/json", ...(init.headers ?? {}) },
      body: init.body === undefined ? undefined : JSON.stringify(init.body),
    }),
    { params: Promise.resolve(init.params ?? { id: "" }) }
  );
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, json, headers: res.headers };
}

function tokenFromEmail(html: string): string {
  const match = html.match(/token=([A-Za-z0-9_-]+)/);
  assert.ok(match, "email contains a token link");
  return match[1];
}

function errorCode(result: CallResult): string {
  return (result.json.error as { code: string }).code;
}

async function registerVerified(
  name: string,
  email: string,
  ip: string
): Promise<{ id: string; cookie: string }> {
  const reg = await call(registerPost, "/api/auth/register", {
    body: { name, email, password: "correct horse battery staple" },
    headers: { "x-forwarded-for": ip },
  });
  assert.equal(reg.status, 201);
  const token = tokenFromEmail(sent[sent.length - 1].html);
  const ver = await call(verifyPost, "/api/auth/verify", { body: { token } });
  assert.equal(ver.status, 200);
  const login = await call(loginPost, "/api/auth/login", {
    body: {
      email,
      password: "correct horse battery staple",
      deviceKey: `device-${email}`,
    },
    headers: { "x-forwarded-for": ip },
  });
  assert.equal(login.status, 200);
  return {
    id: (login.json.user as { id: string }).id,
    cookie: login.headers.get("set-cookie") ?? "",
  };
}

test("W5: admin without 2FA is refused, with 2FA the dashboard works", async () => {
  const admin = await registerVerified(
    "Admin User",
    "admin@example.com",
    "203.0.113.30"
  );
  const plain = await registerVerified(
    "Plain User",
    "plain@example.com",
    "203.0.113.31"
  );
  await client.query("UPDATE users SET is_admin = true WHERE email = $1", [
    "admin@example.com",
  ]);

  // Non-admin: forbidden on every admin route.
  const forbidden = await call(statsGet, "/api/admin/stats", {
    method: "GET",
    headers: { cookie: plain.cookie },
  });
  assert.equal(forbidden.status, 403);
  assert.equal(errorCode(forbidden), "forbidden");

  // Admin without 2FA: refused until they enroll.
  const no2fa = await call(statsGet, "/api/admin/stats", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal(no2fa.status, 403);
  assert.equal(errorCode(no2fa), "admin_2fa_required");

  // Enroll: setup returns a QR + secret; confirm enables 2FA.
  const setup = await call(setupPost, "/api/auth/2fa/setup", {
    headers: { cookie: admin.cookie },
  });
  assert.equal(setup.status, 200);
  const secret = setup.json.secret as string;
  assert.match(setup.json.otpauthUrl as string, /^otpauth:\/\/totp\//);
  assert.match(setup.json.qrDataUrl as string, /^data:image\/png;base64,/);
  const confirm = await call(confirmPost, "/api/auth/2fa/confirm", {
    body: { code: generateTotpCode(secret) },
    headers: { cookie: admin.cookie },
  });
  assert.equal(confirm.status, 200);
  const backupCodes = confirm.json.backupCodes as string[];
  assert.equal(backupCodes.length, 10);
  assert.match(backupCodes[0], /^[A-HJ-NP-Z2-9]{5}-[A-HJ-NP-Z2-9]{5}$/);

  // Setup again is refused once 2FA is on.
  const again = await call(setupPost, "/api/auth/2fa/setup", {
    headers: { cookie: admin.cookie },
  });
  assert.equal(again.status, 409);
  assert.equal(errorCode(again), "two_factor_already_enabled");

  // Admin routes now work.
  const stats = await call(statsGet, "/api/admin/stats", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal(stats.status, 200);
  assert.deepEqual(stats.json.stats, {
    users: 2,
    linkedAccounts: 0,
    activeDevices: 2,
    twoFactorUsers: 1,
  });

  const users = await call(usersGet, "/api/admin/users", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal(users.status, 200);
  assert.equal(users.json.total, 2);
  const rows = users.json.users as { email: string; packageName: string }[];
  assert.equal(rows[0].packageName, "Free");

  const search = await call(usersGet, "/api/admin/users?q=plain", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal(search.json.total, 1);

  // Packages: create, edit, assign.
  const created = await call(packagesPost, "/api/admin/packages", {
    body: {
      name: "Test",
      maxLinkedAccounts: 3,
      maxDevices: 3,
      priceCents: 0,
    },
    headers: { cookie: admin.cookie },
  });
  assert.equal(created.status, 201);
  const packageId = (created.json.package as { id: string }).id;

  const duplicate = await call(packagesPost, "/api/admin/packages", {
    body: {
      name: "Test",
      maxLinkedAccounts: 3,
      maxDevices: 3,
      priceCents: 0,
    },
    headers: { cookie: admin.cookie },
  });
  assert.equal(duplicate.status, 400);

  const patched = await call(packagePatch, `/api/admin/packages/${packageId}`, {
    method: "PATCH",
    body: { maxLinkedAccounts: 5 },
    headers: { cookie: admin.cookie },
    params: { id: packageId },
  });
  assert.equal(patched.status, 200);
  assert.equal(
    (patched.json.package as { max_linked_accounts: number })
      .max_linked_accounts,
    5
  );

  const list = await call(packagesGet, "/api/admin/packages", {
    method: "GET",
    headers: { cookie: admin.cookie },
  });
  assert.equal((list.json.packages as unknown[]).length, 6);

  const assigned = await call(
    assignPost,
    `/api/admin/users/${plain.id}/package`,
    {
      body: { packageId, expiresAt: null },
      headers: { cookie: admin.cookie },
      params: { id: plain.id },
    }
  );
  assert.equal(assigned.status, 200);

  const detail = await call(userGet, `/api/admin/users/${plain.id}`, {
    method: "GET",
    headers: { cookie: admin.cookie },
    params: { id: plain.id },
  });
  assert.equal(detail.status, 200);
  assert.equal(
    (detail.json.subscription as { packageName: string }).packageName,
    "Test"
  );

  // Admins cannot disable their own 2FA.
  const disable = await call(disablePost, "/api/auth/2fa/disable", {
    body: { password: "correct horse battery staple" },
    headers: { cookie: admin.cookie },
  });
  assert.equal(disable.status, 403);
  assert.equal(errorCode(disable), "admin_2fa_required");

  // Backup codes: regenerate needs a TOTP code; the new set works once.
  const regen = await call(backupCodesPost, "/api/auth/2fa/backup-codes", {
    body: { code: generateTotpCode(secret) },
    headers: { cookie: admin.cookie },
  });
  assert.equal(regen.status, 200);
  const freshCodes = regen.json.backupCodes as string[];
  assert.equal(freshCodes.length, 10);

  // Log in with a backup code: it works once, then it is spent.
  await call(logoutPost, "/api/auth/logout", {
    headers: { cookie: admin.cookie },
  });
  const login = await call(loginPost, "/api/auth/login", {
    body: {
      email: "admin@example.com",
      password: "correct horse battery staple",
      deviceKey: "device-admin@example.com",
    },
    headers: { "x-forwarded-for": "203.0.113.30" },
  });
  assert.equal(login.json.step, "2fa_required");
  const pendingToken = login.json.pendingToken as string;
  const ok = await call(verify2faPost, "/api/auth/2fa/verify", {
    body: {
      token: pendingToken,
      code: freshCodes[0],
      deviceKey: "device-admin@example.com",
    },
  });
  assert.equal(ok.status, 200);
  const spent = await call(verify2faPost, "/api/auth/2fa/verify", {
    body: {
      token: pendingToken,
      code: freshCodes[0],
      deviceKey: "device-admin@example.com",
    },
  });
  assert.equal(spent.status, 401);
  assert.equal(errorCode(spent), "invalid_2fa_code");
});
