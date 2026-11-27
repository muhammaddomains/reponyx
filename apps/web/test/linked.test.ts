import test from "node:test";
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import { findUserByEmail, migrate, type DbClient } from "@reponyx/database";
import type { EmailMessage, SentEmail } from "@reponyx/email";
import { setDbClientForTests } from "../src/lib/db";
import { setEmailSenderForTests } from "../src/lib/email";
import { createOAuthState, setFetchForTests } from "../src/lib/oauth";
import { POST as registerPost } from "../src/app/api/auth/register/route";
import { POST as verifyPost } from "../src/app/api/auth/verify/route";
import { POST as loginPost } from "../src/app/api/auth/login/route";
import { POST as resetPost } from "../src/app/api/auth/reset/route";
import { POST as resetConfirmPost } from "../src/app/api/auth/reset/confirm/route";
import { GET as linkedGet } from "../src/app/api/me/linked/route";
import { DELETE as unlinkDelete } from "../src/app/api/me/linked/[provider]/route";
import { GET as googleCallback } from "../src/app/api/google/oauth/callback/route";

/**
 * Linked providers API against real Postgres (PGlite) and the real
 * route handlers. The only fakes are the provider HTTP calls (stub
 * fetch) and the email transport (captures mail).
 */

process.env.SESSION_SECRET = "test-session-secret-32-chars-long";
process.env.GOOGLE_CLIENT_ID = "google-test-client-id";
process.env.GOOGLE_CLIENT_SECRET = "google-test-client-secret";

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

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function googleStub(email: string, name: string | null, sub: string) {
  return async (input: string): Promise<Response> => {
    if (input === "https://oauth2.googleapis.com/token") {
      return jsonResponse({ access_token: "google-access-token" });
    }
    if (input === "https://openidconnect.googleapis.com/v1/userinfo") {
      return jsonResponse({ sub, email, name, email_verified: true });
    }
    throw new Error(`unexpected fetch: ${input}`);
  };
}

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

async function get(
  handler: (req: Request) => Promise<Response>,
  url: string,
  headers: Record<string, string> = {}
): Promise<{
  status: number;
  location: string | null;
  json: Record<string, unknown>;
}> {
  const res = await handler(new Request(url, { headers }));
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, location: res.headers.get("location"), json };
}

async function del(
  handler: (
    req: Request,
    props: { params: Promise<{ provider: string }> }
  ) => Promise<Response>,
  provider: string,
  headers: Record<string, string> = {}
): Promise<{ status: number; json: Record<string, unknown> }> {
  const res = await handler(
    new Request(`http://localhost/api/me/linked/${provider}`, {
      method: "DELETE",
      headers,
    }),
    { params: Promise.resolve({ provider }) }
  );
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { status: res.status, json };
}

function tokenFromEmail(html: string): string {
  const match = html.match(/token=([A-Za-z0-9_-]+)/);
  assert.ok(match, "email contains a token link");
  return match[1];
}

function errorCode(result: { json: Record<string, unknown> }): string {
  return (result.json.error as { code: string }).code;
}

const EMAIL_A = "user-a@example.com";
const EMAIL_B = "user-b@example.com";

// User A: email + password, verified. User B: created via Google
// register (no password, Google linked).
let cookieA = "";
let cookieB = "";
{
  const reg = await post(
    registerPost,
    "/api/auth/register",
    {
      name: "User A",
      email: EMAIL_A,
      password: "correct horse battery staple",
    },
    { "x-forwarded-for": "203.0.113.21" }
  );
  assert.equal(reg.status, 201);
  const verify = await post(verifyPost, "/api/auth/verify", {
    token: tokenFromEmail(sent[0].html),
  });
  assert.equal(verify.status, 200);
  const login = await loginPost(
    new Request("http://localhost/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email: EMAIL_A,
        password: "correct horse battery staple",
      }),
    })
  );
  assert.equal(login.status, 200);
  cookieA = (login.headers.get("set-cookie") ?? "").split(";")[0];

  const state = createOAuthState("google", "register");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL_B, "User B", "google-sub-b"));
  try {
    const res = await googleCallback(
      new Request(
        `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
        { headers: { "x-forwarded-for": "203.0.113.22" } }
      )
    );
    assert.equal(res.status, 307);
    assert.equal(res.headers.get("location"), "http://localhost/dashboard");
    cookieB = (res.headers.get("set-cookie") ?? "").split(";")[0];
    assert.match(cookieB, /reponyx-session=/);
  } finally {
    setFetchForTests(null);
  }
  const userB = await findUserByEmail(client, EMAIL_B);
  assert.ok(userB);
  assert.equal(userB.has_password, false);
}

test("linked providers start empty, link via the profile flow, unlink works", async () => {
  const empty = await get(linkedGet, "http://localhost/api/me/linked", {
    cookie: cookieA,
  });
  assert.equal(empty.status, 200);
  assert.deepEqual(empty.json.linked, []);

  const state = createOAuthState("google", "link");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL_A, "User A", "google-sub-a"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { cookie: cookieA }
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard?linked=google");
  } finally {
    setFetchForTests(null);
  }

  const listed = await get(linkedGet, "http://localhost/api/me/linked", {
    cookie: cookieA,
  });
  assert.equal(listed.status, 200);
  const rows = listed.json.linked as { provider: string; email: string }[];
  assert.equal(rows.length, 1);
  assert.equal(rows[0].provider, "google");
  assert.equal(rows[0].email, EMAIL_A);

  const removed = await del(unlinkDelete, "google", { cookie: cookieA });
  assert.equal(removed.status, 200);
  assert.deepEqual(removed.json, { ok: true });
  const after = await get(linkedGet, "http://localhost/api/me/linked", {
    cookie: cookieA,
  });
  assert.deepEqual(after.json.linked, []);
  // Unlinking twice is idempotent.
  const again = await del(unlinkDelete, "google", { cookie: cookieA });
  assert.equal(again.status, 200);
});

test("unlinking the last method is refused without a password, allowed after setting one", async () => {
  const listed = await get(linkedGet, "http://localhost/api/me/linked", {
    cookie: cookieB,
  });
  assert.equal(
    (listed.json.linked as { provider: string }[])
      .map((r) => r.provider)
      .join(),
    "google"
  );
  const refused = await del(unlinkDelete, "google", { cookie: cookieB });
  assert.equal(refused.status, 409);
  assert.equal(errorCode(refused), "last_login_method");

  const before = sent.length;
  const req = await post(resetPost, "/api/auth/reset", { email: EMAIL_B });
  assert.equal(req.status, 200);
  const confirm = await post(resetConfirmPost, "/api/auth/reset/confirm", {
    token: tokenFromEmail(sent[before].html),
    password: "new horse battery staple",
  });
  assert.equal(confirm.status, 200);
  const userB = await findUserByEmail(client, EMAIL_B);
  assert.equal(userB?.has_password, true);
  // The reset revoked every session, so log in again for the final unlink.
  const login = await loginPost(
    new Request("http://localhost/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email: EMAIL_B,
        password: "new horse battery staple",
      }),
    })
  );
  assert.equal(login.status, 200);
  cookieB = (login.headers.get("set-cookie") ?? "").split(";")[0];

  const allowed = await del(unlinkDelete, "google", { cookie: cookieB });
  assert.equal(allowed.status, 200);
});

test("linking a provider that belongs to another account is refused", async () => {
  // B re-links Google first (test 2 unlinked it after setting a password).
  const relink = createOAuthState("google", "link");
  assert.ok(relink);
  setFetchForTests(googleStub(EMAIL_B, "User B", "google-sub-b"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(relink)}`,
      { cookie: cookieB }
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard?linked=google");
  } finally {
    setFetchForTests(null);
  }
  // A presents the same Google sub with A's email.
  const state = createOAuthState("google", "link");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL_A, "User A", "google-sub-b"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { cookie: cookieA }
    );
    assert.equal(res.status, 307);
    assert.equal(
      res.location,
      "http://localhost/dashboard?link_error=already_linked"
    );
  } finally {
    setFetchForTests(null);
  }
});

test("linked list requires a session", async () => {
  const res = await get(linkedGet, "http://localhost/api/me/linked");
  assert.equal(res.status, 401);
});
