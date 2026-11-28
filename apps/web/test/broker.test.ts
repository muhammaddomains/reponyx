import test from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import { linkInstallation, migrate, type DbClient } from "@reponyx/database";
import type { EmailMessage, SentEmail } from "@reponyx/email";
import { setDbClientForTests } from "../src/lib/db";
import { setEmailSenderForTests } from "../src/lib/email";
import { setFetchForTests } from "../src/lib/github-app";
import { POST as registerPost } from "../src/app/api/auth/register/route";
import { POST as verifyPost } from "../src/app/api/auth/verify/route";
import { POST as loginPost } from "../src/app/api/auth/login/route";
import { POST as mintPost } from "../src/app/api/broker/mint/route";
import { GET as verifyInstallationGet } from "../src/app/api/broker/installations/[id]/route";
import { POST as linkPost } from "../src/app/api/me/github-accounts/route";
import { DELETE as unlinkDelete } from "../src/app/api/me/github-accounts/[id]/route";
import { GET as limitsGet } from "../src/app/api/me/limits/route";
import { GET as devicesGet } from "../src/app/api/devices/route";
import { POST as devicesPost } from "../src/app/api/devices/route";
import { DELETE as revokeDeviceDelete } from "../src/app/api/devices/[id]/route";

/**
 * Broker ↔ web (W4) against real Postgres (PGlite) and the real
 * route handlers. The only fakes are the GitHub App HTTP calls (stub
 * fetch) and the email transport (captures mail). No real GitHub
 * request is ever made.
 */

process.env.SESSION_SECRET = "test-session-secret-32-chars-long";

const { privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
process.env.REPONYX_GITHUB_APP_ID = "5896712";
process.env.REPONYX_GITHUB_APP_PRIVATE_KEY = privateKey
  .export({ type: "pkcs8", format: "pem" })
  .toString();

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
await migrate(client, migrations);

setDbClientForTests(client);
setEmailSenderForTests({
  provider: "resend",
  async send(message: EmailMessage): Promise<SentEmail> {
    sent.push(message);
    return { messageId: "test-1", provider: "resend" };
  },
});

/** GitHub App installations the stub knows about: id → account. */
const knownInstallations = new Map<number, { id: number; login: string }>([
  [1001, { id: 5001, login: "octocat" }],
  [1002, { id: 5002, login: "hubot" }],
  [1003, { id: 5003, login: "defunkt" }],
  [2001, { id: 6001, login: "userb-one" }],
  [2002, { id: 6002, login: "userb-two" }],
  [2003, { id: 6003, login: "userb-three" }],
  [3001, { id: 7001, login: "rank-one" }],
  [3002, { id: 7002, login: "rank-two" }],
  [3003, { id: 7003, login: "rank-three" }],
]);

const mintCalls: number[] = [];

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

setFetchForTests(async (input: string, init?: RequestInit) => {
  const url = new URL(input);
  const match = url.pathname.match(/^\/app\/installations\/(\d+)$/);
  if (match && init?.method !== "POST") {
    const id = Number(match[1]);
    const account = knownInstallations.get(id);
    if (!account) return jsonResponse({ message: "Not Found" }, 404);
    return jsonResponse({ id, account });
  }
  const mintMatch = url.pathname.match(
    /^\/app\/installations\/(\d+)\/access_tokens$/
  );
  if (mintMatch && init?.method === "POST") {
    const id = Number(mintMatch[1]);
    if (!knownInstallations.has(id)) {
      return jsonResponse({ message: "Not Found" }, 404);
    }
    mintCalls.push(id);
    return jsonResponse({
      token: `ghs_test-token-${id}`,
      expires_at: new Date(Date.now() + 3600_000).toISOString(),
    });
  }
  throw new Error(`unexpected fetch: ${input}`);
});

async function post(
  handler: (req: Request) => Promise<Response>,
  path: string,
  body: unknown,
  headers: Record<string, string> = {}
): Promise<{ status: number; json: Record<string, unknown> }> {
  const res = await handler(
    new Request(`http://localhost${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...headers },
      body: JSON.stringify(body),
    })
  );
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, json };
}

async function getJson(
  handler: (req: Request) => Promise<Response>,
  url: string,
  headers: Record<string, string> = {}
): Promise<{ status: number; json: Record<string, unknown> }> {
  const res = await handler(new Request(url, { headers }));
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, json };
}

async function del(
  handler: (req: Request, props: never) => Promise<Response>,
  url: string,
  params: Record<string, string>,
  headers: Record<string, string> = {}
): Promise<{ status: number; json: Record<string, unknown> }> {
  const res = await handler(new Request(url, { method: "DELETE", headers }), {
    params: Promise.resolve(params),
  } as never);
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, json };
}

function tokenFromEmail(html: string): string {
  const match = html.match(/token=([A-Za-z0-9_-]+)/);
  assert.ok(match, "email contains a token link");
  return match[1];
}

async function registerAndLogin(
  email: string,
  ip: string
): Promise<{ token: string; userId: string }> {
  const before = sent.length;
  const reg = await post(
    registerPost,
    "/api/auth/register",
    { email, password: "correct-horse-battery-staple", name: "Test User" },
    { "x-forwarded-for": ip }
  );
  assert.equal(reg.status, 201, JSON.stringify(reg.json));
  const mail = sent[before];
  assert.ok(mail, "verification email sent");
  const token = tokenFromEmail(mail.html);
  const verified = await post(verifyPost, "/api/auth/verify", { token });
  assert.equal(verified.status, 200, JSON.stringify(verified.json));
  const login = await post(loginPost, "/api/auth/login", {
    email,
    password: "correct-horse-battery-staple",
  });
  assert.equal(login.status, 200, JSON.stringify(login.json));
  const session = login.json.session as { token: string };
  const user = login.json.user as { id: string };
  return { token: session.token, userId: user.id };
}

function bearer(token: string): Record<string, string> {
  return { authorization: `Bearer ${token}` };
}

const userA = await registerAndLogin("broker-a@example.com", "203.0.113.31");
const userB = await registerAndLogin("broker-b@example.com", "203.0.113.32");
const userC = await registerAndLogin("broker-c@example.com", "203.0.113.33");
const userD = await registerAndLogin("broker-d@example.com", "203.0.113.34");

test("link an App installation, then mint returns a web-minted token", async () => {
  const linked = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 1001 },
    bearer(userA.token)
  );
  assert.equal(linked.status, 200, JSON.stringify(linked.json));
  const account = linked.json.account as { id: string; login: string };
  assert.equal(account.login, "octocat");

  const limits = await getJson(
    limitsGet,
    "http://localhost/api/me/limits",
    bearer(userA.token)
  );
  assert.equal(limits.status, 200);
  assert.equal(
    (limits.json.usage as { linkedAccounts: number }).linkedAccounts,
    1
  );

  const minted = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1001 },
    bearer(userA.token)
  );
  assert.equal(minted.status, 200, JSON.stringify(minted.json));
  assert.equal(minted.json.token, "ghs_test-token-1001");
  assert.ok(typeof minted.json.expiresAt === "string");
  assert.deepEqual(mintCalls, [1001]);
});

test("mint without a link is 404 installation_not_linked", async () => {
  const minted = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1002 },
    bearer(userA.token)
  );
  assert.equal(minted.status, 404);
  assert.equal(
    (minted.json.error as { code: string }).code,
    "installation_not_linked"
  );
});

test("mint for another user's installation is 404", async () => {
  const minted = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1001 },
    bearer(userB.token)
  );
  assert.equal(minted.status, 404);
  assert.equal(
    (minted.json.error as { code: string }).code,
    "installation_not_linked"
  );
});

test("verify endpoint returns the account login for a known installation", async () => {
  const res = await getJson(
    (req) =>
      verifyInstallationGet(req, { params: Promise.resolve({ id: "1001" }) }),
    "http://localhost/api/broker/installations/1001",
    bearer(userA.token)
  );
  assert.equal(res.status, 200, JSON.stringify(res.json));
  assert.equal(res.json.accountLogin, "octocat");
  assert.equal(res.json.accountId, 5001);

  const missing = await getJson(
    (req) =>
      verifyInstallationGet(req, { params: Promise.resolve({ id: "9999" }) }),
    "http://localhost/api/broker/installations/9999",
    bearer(userA.token)
  );
  assert.equal(missing.status, 404);
  assert.equal(
    (missing.json.error as { code: string }).code,
    "installation_not_linked"
  );
});

test("mint after a GitHub-side uninstall is 404", async () => {
  const linked = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 1003 },
    bearer(userA.token)
  );
  assert.equal(linked.status, 200, JSON.stringify(linked.json));
  knownInstallations.delete(1003);
  try {
    const minted = await post(
      mintPost,
      "/api/broker/mint",
      { installationId: 1003 },
      bearer(userA.token)
    );
    assert.equal(minted.status, 404);
    assert.equal(
      (minted.json.error as { code: string }).code,
      "installation_not_linked"
    );
  } finally {
    knownInstallations.set(1003, { id: 5003, login: "defunkt" });
  }
});

test("mint with a revoked device is 403 device_revoked", async () => {
  const created = await post(
    devicesPost,
    "/api/devices",
    { deviceKey: "device-1", machineName: "DESKTOP-TEST" },
    bearer(userA.token)
  );
  assert.equal(created.status, 200, JSON.stringify(created.json));
  const device = created.json.device as { id: string };

  const minted = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1001, deviceIdentifier: "device-1" },
    bearer(userA.token)
  );
  assert.equal(minted.status, 200, JSON.stringify(minted.json));

  const revoked = await del(
    revokeDeviceDelete,
    "http://localhost/api/devices/" + device.id,
    { id: device.id },
    bearer(userA.token)
  );
  assert.equal(revoked.status, 200);
  assert.deepEqual(revoked.json, { ok: true });

  const devices = await getJson(
    devicesGet,
    "http://localhost/api/devices",
    bearer(userA.token)
  );
  assert.equal(devices.status, 200);
  const row = (
    devices.json.devices as { id: string; revoked_at: string | null }[]
  ).find((d) => d.id === device.id);
  assert.ok(row?.revoked_at, "device shows revoked_at");

  const blocked = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1001, deviceIdentifier: "device-1" },
    bearer(userA.token)
  );
  assert.equal(blocked.status, 403);
  assert.equal((blocked.json.error as { code: string }).code, "device_revoked");

  const unknown = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1001, deviceIdentifier: "never-registered" },
    bearer(userA.token)
  );
  assert.equal(unknown.status, 403);
  assert.equal((unknown.json.error as { code: string }).code, "device_revoked");
});

test("link over the Free limit is 403 limit_exceeded", async () => {
  const first = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 2001 },
    bearer(userB.token)
  );
  assert.equal(first.status, 200, JSON.stringify(first.json));
  const second = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 2002 },
    bearer(userB.token)
  );
  assert.equal(second.status, 200, JSON.stringify(second.json));
  const third = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 2003 },
    bearer(userB.token)
  );
  assert.equal(third.status, 403);
  assert.equal((third.json.error as { code: string }).code, "limit_exceeded");
  assert.equal((third.json.error as { data: { max: number } }).data.max, 2);
});

test("link is idempotent and does not double-count", async () => {
  const again = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 1001 },
    bearer(userA.token)
  );
  assert.equal(again.status, 200, JSON.stringify(again.json));
  const limits = await getJson(
    limitsGet,
    "http://localhost/api/me/limits",
    bearer(userA.token)
  );
  assert.equal(
    (limits.json.usage as { linkedAccounts: number }).linkedAccounts,
    2
  );
});

test("mint rank: oldest keeps minting over the limit, newest is refused", async () => {
  // Three links for user C, bypassing the link route's limit (the
  // mint endpoint is what enforces the oldest-first rule).
  for (const [installationId, accountId, login] of [
    [3001, "7001", "rank-one"],
    [3002, "7002", "rank-two"],
    [3003, "7003", "rank-three"],
  ] as const) {
    await linkInstallation(client, {
      userId: userC.userId,
      githubInstallationId: installationId,
      accountId,
      login,
    });
  }
  const oldest = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 3001 },
    bearer(userC.token)
  );
  assert.equal(oldest.status, 200, JSON.stringify(oldest.json));
  const newest = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 3003 },
    bearer(userC.token)
  );
  assert.equal(newest.status, 403);
  assert.equal((newest.json.error as { code: string }).code, "limit_exceeded");
});

test("link a PAT account, then unlink it", async () => {
  const linked = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "pat", login: "patuser" },
    bearer(userD.token)
  );
  assert.equal(linked.status, 200, JSON.stringify(linked.json));
  const account = linked.json.account as { id: string; login: string };
  assert.equal(account.login, "patuser");

  const limits = await getJson(
    limitsGet,
    "http://localhost/api/me/limits",
    bearer(userD.token)
  );
  assert.equal(
    (limits.json.usage as { linkedAccounts: number }).linkedAccounts,
    1
  );

  const unlinked = await del(
    unlinkDelete,
    "http://localhost/api/me/github-accounts/" + account.id,
    { id: account.id },
    bearer(userD.token)
  );
  assert.equal(unlinked.status, 200);
  assert.deepEqual(unlinked.json, { ok: true });

  const after = await getJson(
    limitsGet,
    "http://localhost/api/me/limits",
    bearer(userD.token)
  );
  assert.equal(
    (after.json.usage as { linkedAccounts: number }).linkedAccounts,
    0
  );
});

test("unlink an App account removes the installation link", async () => {
  const limits = await getJson(
    limitsGet,
    "http://localhost/api/me/limits",
    bearer(userA.token)
  );
  assert.equal(
    (limits.json.usage as { linkedAccounts: number }).linkedAccounts,
    2
  );
  // The App link for 1001 is the only app row left for user A after
  // the PAT unlink above (1003 was linked, then uninstalled on the
  // GitHub side — its row is still linked until unlinked).
  const linked = await post(
    linkPost,
    "/api/me/github-accounts",
    { authKind: "app", installationId: 1001 },
    bearer(userA.token)
  );
  assert.equal(linked.status, 200);
  const account = linked.json.account as { id: string };
  const unlinked = await del(
    unlinkDelete,
    "http://localhost/api/me/github-accounts/" + account.id,
    { id: account.id },
    bearer(userA.token)
  );
  assert.equal(unlinked.status, 200);
  const minted = await post(
    mintPost,
    "/api/broker/mint",
    { installationId: 1001 },
    bearer(userA.token)
  );
  assert.equal(minted.status, 404);
  assert.equal(
    (minted.json.error as { code: string }).code,
    "installation_not_linked"
  );
});

test("mint without App config is 503 not_configured", async () => {
  const savedId = process.env.REPONYX_GITHUB_APP_ID;
  const savedKey = process.env.REPONYX_GITHUB_APP_PRIVATE_KEY;
  delete process.env.REPONYX_GITHUB_APP_ID;
  delete process.env.REPONYX_GITHUB_APP_PRIVATE_KEY;
  try {
    const minted = await post(
      mintPost,
      "/api/broker/mint",
      { installationId: 1001 },
      bearer(userA.token)
    );
    assert.equal(minted.status, 503);
    assert.equal(
      (minted.json.error as { code: string }).code,
      "not_configured"
    );
  } finally {
    process.env.REPONYX_GITHUB_APP_ID = savedId;
    process.env.REPONYX_GITHUB_APP_PRIVATE_KEY = savedKey;
  }
});
