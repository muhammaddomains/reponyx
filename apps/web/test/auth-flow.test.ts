import test from "node:test";
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import { generateTotpCode, generateTotpSecret } from "@reponyx/auth";
import { migrate, type DbClient } from "@reponyx/database";
import type { EmailMessage, SentEmail } from "@reponyx/email";
import { setDbClientForTests } from "../src/lib/db";
import { setEmailSenderForTests } from "../src/lib/email";
import { POST as registerPost } from "../src/app/api/auth/register/route";
import { POST as verifyPost } from "../src/app/api/auth/verify/route";
import { POST as loginPost } from "../src/app/api/auth/login/route";
import { POST as verify2faPost } from "../src/app/api/auth/2fa/verify/route";
import { POST as logoutPost } from "../src/app/api/auth/logout/route";
import { POST as resetPost } from "../src/app/api/auth/reset/route";
import { POST as resetConfirmPost } from "../src/app/api/auth/reset/confirm/route";
import { GET as limitsGet } from "../src/app/api/me/limits/route";
import {
  GET as devicesGet,
  POST as devicesPost,
} from "../src/app/api/devices/route";

/**
 * End-to-end auth flow against real Postgres (PGlite, the WASM build)
 * and the real route handlers. The only fakes are the email transport
 * (it captures mail instead of sending it) and the database client
 * (a PGlite adapter — the SQL, the migrations and the hashing are all
 * the production ones).
 */

process.env.SESSION_SECRET = "test-session-secret-32-chars-long";

const sent: EmailMessage[] = [];
const db = new PGlite();
const client: DbClient = {
  async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
    // Multi-statement migration files go through exec(); parameterized
    // statements through query().
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
  handler: (req: Request) => Promise<Response>,
  path: string,
  init: { method?: string; body?: unknown; headers?: Record<string, string> }
): Promise<CallResult> {
  const res = await handler(
    new Request(`http://localhost${path}`, {
      method: init.method ?? "POST",
      headers: { "Content-Type": "application/json", ...(init.headers ?? {}) },
      body: init.body === undefined ? undefined : JSON.stringify(init.body),
    })
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

test("full flow: register → verify → login → limits → devices → logout", async () => {
  const ip = { "x-forwarded-for": "203.0.113.10" };
  const reg = await call(registerPost, "/api/auth/register", {
    body: {
      name: "Flow User",
      email: "a@example.com",
      password: "correct horse battery staple",
    },
    headers: ip,
  });
  assert.equal(reg.status, 201);
  assert.equal(sent.length, 1);
  assert.match(sent[0].html, /Verify my email/);

  const early = await call(loginPost, "/api/auth/login", {
    body: { email: "a@example.com", password: "correct horse battery staple" },
  });
  assert.equal(early.status, 403);
  assert.equal(errorCode(early), "email_not_verified");

  const token = tokenFromEmail(sent[0].html);
  const ver = await call(verifyPost, "/api/auth/verify", { body: { token } });
  assert.equal(ver.status, 200);
  const again = await call(verifyPost, "/api/auth/verify", { body: { token } });
  assert.equal(again.status, 404);
  assert.equal(errorCode(again), "invalid_token");

  const wrong = await call(loginPost, "/api/auth/login", {
    body: { email: "a@example.com", password: "nope" },
  });
  assert.equal(wrong.status, 401);
  assert.equal(errorCode(wrong), "invalid_credentials");

  const login = await call(loginPost, "/api/auth/login", {
    body: { email: "a@example.com", password: "correct horse battery staple" },
  });
  assert.equal(login.status, 200);
  const sessionToken = (login.json.session as { token: string }).token;
  assert.match(sessionToken, /^[A-Za-z0-9_-]+$/);
  const cookie = login.headers.get("set-cookie") ?? "";
  assert.match(cookie, /reponyx-session=/);

  const limits = await call(limitsGet, "/api/me/limits", {
    method: "GET",
    headers: { cookie },
  });
  assert.equal(limits.status, 200);
  assert.equal((limits.json.package as { name: string }).name, "Free");
  assert.equal((limits.json.user as { name: string | null }).name, "Flow User");
  assert.deepEqual(limits.json.usage, { linkedAccounts: 0, devices: 0 });

  const dev = await call(devicesPost, "/api/devices", {
    body: {
      deviceKey: "dev-1",
      machineName: "DESKTOP-TEST",
      platform: "win32",
      appVersion: "0.1.0",
    },
    headers: { authorization: `Bearer ${sessionToken}` },
  });
  assert.equal(dev.status, 200);
  const list = await call(devicesGet, "/api/devices", {
    method: "GET",
    headers: { cookie },
  });
  assert.equal(list.status, 200);
  assert.equal((list.json.devices as unknown[]).length, 1);
  const limits2 = await call(limitsGet, "/api/me/limits", {
    method: "GET",
    headers: { cookie },
  });
  assert.deepEqual(limits2.json.usage, { linkedAccounts: 0, devices: 1 });

  const out = await call(logoutPost, "/api/auth/logout", {
    headers: { cookie },
  });
  assert.equal(out.status, 200);
  const after = await call(limitsGet, "/api/me/limits", {
    method: "GET",
    headers: { cookie },
  });
  assert.equal(after.status, 401);
  assert.equal(errorCode(after), "unauthorized");
});

test("registration limit: 1 per IP per 24 hours", async () => {
  const blocked = await call(registerPost, "/api/auth/register", {
    body: {
      name: "Blocked User",
      email: "b@example.com",
      password: "correct horse battery staple",
    },
    headers: { "x-forwarded-for": "203.0.113.10" },
  });
  assert.equal(blocked.status, 429);
  assert.equal(errorCode(blocked), "registration_rate_limited");
  const retryAt = (blocked.json.error as { data: { retry_at: string } }).data
    .retry_at;
  assert.ok(retryAt);

  const ok = await call(registerPost, "/api/auth/register", {
    body: {
      name: "Second User",
      email: "b@example.com",
      password: "correct horse battery staple",
    },
    headers: { "x-forwarded-for": "203.0.113.20" },
  });
  assert.equal(ok.status, 201);
});

test("account-switch limit: same account can return, a different one waits 1 hour", async () => {
  const ipB = { "x-forwarded-for": "203.0.113.20" };
  const tokenB = tokenFromEmail(sent[1].html);
  const ver = await call(verifyPost, "/api/auth/verify", {
    body: { token: tokenB },
  });
  assert.equal(ver.status, 200);

  const loginB = await call(loginPost, "/api/auth/login", {
    body: {
      email: "b@example.com",
      password: "correct horse battery staple",
      deviceKey: "shared-device",
    },
    headers: ipB,
  });
  assert.equal(loginB.status, 200);
  const cookieB = loginB.headers.get("set-cookie") ?? "";

  await call(logoutPost, "/api/auth/logout", {
    headers: { cookie: cookieB, ...ipB },
  });

  const blocked = await call(loginPost, "/api/auth/login", {
    body: {
      email: "a@example.com",
      password: "correct horse battery staple",
      deviceKey: "shared-device",
    },
    headers: ipB,
  });
  assert.equal(blocked.status, 403);
  assert.equal(errorCode(blocked), "account_switch_blocked");

  const back = await call(loginPost, "/api/auth/login", {
    body: {
      email: "b@example.com",
      password: "correct horse battery staple",
      deviceKey: "shared-device",
    },
    headers: ipB,
  });
  assert.equal(back.status, 200);
});

test("password reset: request → confirm → new password works, old one does not", async () => {
  const before = sent.length;
  const req = await call(resetPost, "/api/auth/reset", {
    body: { email: "a@example.com" },
  });
  assert.equal(req.status, 200);
  assert.equal(sent.length, before + 1);
  const token = tokenFromEmail(sent[before].html);
  const confirm = await call(resetConfirmPost, "/api/auth/reset/confirm", {
    body: { token, password: "new horse battery staple" },
  });
  assert.equal(confirm.status, 200);

  // The link is single-use: a second confirm with the same token fails.
  const replay = await call(resetConfirmPost, "/api/auth/reset/confirm", {
    body: { token, password: "another horse battery staple" },
  });
  assert.equal(replay.status, 404);
  assert.equal(errorCode(replay), "invalid_token");

  const oldLogin = await call(loginPost, "/api/auth/login", {
    body: { email: "a@example.com", password: "correct horse battery staple" },
  });
  assert.equal(oldLogin.status, 401);
  const newLogin = await call(loginPost, "/api/auth/login", {
    body: { email: "a@example.com", password: "new horse battery staple" },
  });
  assert.equal(newLogin.status, 200);
});

test("registering an existing email is refused", async () => {
  const res = await call(registerPost, "/api/auth/register", {
    body: {
      name: "Again User",
      email: "a@example.com",
      password: "correct horse battery staple",
    },
    headers: { "x-forwarded-for": "203.0.113.30" },
  });
  assert.equal(res.status, 409);
  assert.equal(errorCode(res), "email_taken");
});

test("2FA: password login stops at the 2FA step; verify with the code completes it", async () => {
  const ip = { "x-forwarded-for": "203.0.113.40" };
  const reg = await call(registerPost, "/api/auth/register", {
    body: {
      name: "Twofa Flow",
      email: "twofa-flow@example.com",
      password: "correct horse battery staple",
    },
    headers: ip,
  });
  assert.equal(reg.status, 201);
  const token = tokenFromEmail(sent[sent.length - 1].html);
  const ver = await call(verifyPost, "/api/auth/verify", { body: { token } });
  assert.equal(ver.status, 200);
  const secret = generateTotpSecret();
  await client.query("UPDATE users SET totp_secret = $1 WHERE email = $2", [
    secret,
    "twofa-flow@example.com",
  ]);

  // The password is right, but no session comes back yet.
  const login = await call(loginPost, "/api/auth/login", {
    body: {
      email: "twofa-flow@example.com",
      password: "correct horse battery staple",
    },
  });
  assert.equal(login.status, 200);
  assert.equal(login.json.step, "2fa_required");
  assert.equal(login.json.session, undefined);
  const pendingToken = login.json.pendingToken as string;
  assert.match(pendingToken, /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/);

  // A wrong code is refused, and a tampered token is refused.
  const wrong = await call(verify2faPost, "/api/auth/2fa/verify", {
    body: { token: pendingToken, code: "000000" },
  });
  assert.equal(wrong.status, 401);
  assert.equal(errorCode(wrong), "invalid_2fa_code");
  const tampered = await call(verify2faPost, "/api/auth/2fa/verify", {
    body: { token: "tampered.token", code: "123456" },
  });
  assert.equal(tampered.status, 404);
  assert.equal(errorCode(tampered), "invalid_token");

  // The right code mints the session and registers the device.
  const ok = await call(verify2faPost, "/api/auth/2fa/verify", {
    body: {
      token: pendingToken,
      code: generateTotpCode(secret),
      deviceKey: "flow-device-2fa",
      machineName: "DESKTOP-TEST",
      platform: "Windows 11",
      appVersion: "0.1.0",
    },
  });
  assert.equal(ok.status, 200);
  const sessionToken = (ok.json.session as { token: string }).token;
  assert.match(sessionToken, /^[A-Za-z0-9_-]+$/);
  const limits = await call(limitsGet, "/api/me/limits", {
    method: "GET",
    headers: { authorization: `Bearer ${sessionToken}` },
  });
  assert.equal(limits.status, 200);
  assert.deepEqual(limits.json.usage, { linkedAccounts: 0, devices: 1 });
});
