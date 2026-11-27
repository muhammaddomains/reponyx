import test from "node:test";
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import { migrate, type DbClient } from "@reponyx/database";
import type { EmailMessage, SentEmail } from "@reponyx/email";
import { setDbClientForTests } from "../src/lib/db";
import { setEmailSenderForTests } from "../src/lib/email";
import {
  createOAuthState,
  setFetchForTests,
  verifyOAuthState,
} from "../src/lib/oauth";
import { POST as registerPost } from "../src/app/api/auth/register/route";
import { POST as verifyPost } from "../src/app/api/auth/verify/route";
import { GET as limitsGet } from "../src/app/api/me/limits/route";
import { GET as googleStart } from "../src/app/api/auth/oauth/google/route";
import { GET as githubStart } from "../src/app/api/auth/oauth/github/route";
import { GET as googleCallback } from "../src/app/api/google/oauth/callback/route";
import { GET as githubCallback } from "../src/app/api/auth/callback/github/route";

/**
 * OAuth login against real Postgres (PGlite) and the real route handlers.
 * The only fakes are the provider HTTP calls (stub fetch) and the email
 * transport (captures mail). No real Google/GitHub request is ever made.
 */

process.env.SESSION_SECRET = "test-session-secret-32-chars-long";
process.env.GOOGLE_CLIENT_ID = "google-test-client-id";
process.env.GOOGLE_CLIENT_SECRET = "google-test-client-secret";
process.env.GITHUB_OAUTH_CLIENT_ID = "github-test-client-id";
process.env.GITHUB_OAUTH_CLIENT_SECRET = "github-test-client-secret";

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

/** Provider stub: Google happy path for the registered user. */
function googleStub(email: string) {
  return async (input: string): Promise<Response> => {
    if (input === "https://oauth2.googleapis.com/token") {
      return jsonResponse({ access_token: "google-access-token" });
    }
    if (input === "https://openidconnect.googleapis.com/v1/userinfo") {
      return jsonResponse({ email, email_verified: true });
    }
    throw new Error(`unexpected fetch: ${input}`);
  };
}

/** Provider stub: GitHub happy path (hidden email → emails endpoint). */
function githubStub(email: string) {
  return async (input: string): Promise<Response> => {
    if (input === "https://github.com/login/oauth/access_token") {
      return jsonResponse({ access_token: "github-access-token" });
    }
    if (input === "https://api.github.com/user") {
      return jsonResponse({ login: "oauthuser", email: null });
    }
    if (input === "https://api.github.com/user/emails") {
      return jsonResponse([
        { email, primary: true, verified: true },
        { email: "old@example.com", primary: false, verified: true },
      ]);
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
  url: string
): Promise<{ status: number; location: string | null; cookie: string | null }> {
  const res = await handler(new Request(url));
  return {
    status: res.status,
    location: res.headers.get("location"),
    cookie: res.headers.get("set-cookie"),
  };
}

function tokenFromEmail(html: string): string {
  const match = html.match(/token=([A-Za-z0-9_-]+)/);
  assert.ok(match, "email contains a token link");
  return match[1];
}

const EMAIL = "oauth-user@example.com";

// Register + verify the user the OAuth tests log in as.
{
  const reg = await post(
    registerPost,
    "/api/auth/register",
    { email: EMAIL, password: "correct horse battery staple" },
    { "x-forwarded-for": "203.0.113.11" }
  );
  assert.equal(reg.status, 201);
  const token = tokenFromEmail(sent[0].html);
  const verify = await post(verifyPost, "/api/auth/verify", { token });
  assert.equal(verify.status, 200);
}

test("google start redirects to the provider with a signed state", async () => {
  const res = await get(googleStart, "http://localhost/api/auth/oauth/google");
  assert.equal(res.status, 307);
  assert.ok(res.location);
  const url = new URL(res.location);
  assert.equal(url.origin, "https://accounts.google.com");
  assert.equal(url.pathname, "/o/oauth2/v2/auth");
  assert.equal(url.searchParams.get("client_id"), "google-test-client-id");
  assert.equal(
    url.searchParams.get("redirect_uri"),
    "http://localhost:3000/api/google/oauth/callback"
  );
  assert.equal(url.searchParams.get("response_type"), "code");
  assert.match(url.searchParams.get("scope") ?? "", /openid/);
  const state = url.searchParams.get("state");
  assert.ok(state);
  assert.equal(verifyOAuthState(state, "google"), true);
  assert.equal(verifyOAuthState(state, "github"), false);
});

test("github start redirects to the provider", async () => {
  const res = await get(githubStart, "http://localhost/api/auth/oauth/github");
  assert.equal(res.status, 307);
  assert.ok(res.location);
  const url = new URL(res.location);
  assert.equal(url.origin, "https://github.com");
  assert.equal(url.pathname, "/login/oauth/authorize");
  assert.equal(url.searchParams.get("client_id"), "github-test-client-id");
  assert.equal(
    url.searchParams.get("redirect_uri"),
    "http://localhost:3000/api/auth/callback/github"
  );
});

test("start refuses when the provider is not configured", async () => {
  delete process.env.GOOGLE_CLIENT_ID;
  try {
    const res = await get(
      googleStart,
      "http://localhost/api/auth/oauth/google"
    );
    assert.equal(res.status, 307);
    assert.equal(
      res.location,
      "http://localhost/auth/login?error=oauth_not_configured"
    );
  } finally {
    process.env.GOOGLE_CLIENT_ID = "google-test-client-id";
  }
});

test("google callback logs in a registered user and sets a session", async () => {
  const state = createOAuthState("google");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard");
    assert.ok(res.cookie);
    assert.match(res.cookie, /reponyx-session=/);
    // The session cookie works: /api/me/limits answers for this user.
    const cookie = res.cookie.split(";")[0];
    const limits = await limitsGet(
      new Request("http://localhost/api/me/limits", {
        headers: { cookie },
      })
    );
    assert.equal(limits.status, 200);
    const body = (await limits.json()) as {
      user: { email: string };
      package: { name: string };
    };
    assert.equal(body.user.email, EMAIL);
    assert.equal(body.package.name, "Free");
  } finally {
    setFetchForTests(null);
  }
});

test("google callback refuses an unregistered email (no auto-create)", async () => {
  const state = createOAuthState("google");
  assert.ok(state);
  setFetchForTests(googleStub("stranger@example.com"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(
      res.location,
      "http://localhost/auth/login?error=oauth_no_account"
    );
    assert.equal(res.cookie, null);
  } finally {
    setFetchForTests(null);
  }
});

test("google callback refuses a tampered state", async () => {
  const res = await get(
    googleCallback,
    "http://localhost/api/google/oauth/callback?code=test-code&state=tampered"
  );
  assert.equal(res.status, 307);
  assert.equal(res.location, "http://localhost/auth/login?error=oauth_failed");
});

test("github callback logs in via the emails endpoint", async () => {
  const state = createOAuthState("github");
  assert.ok(state);
  setFetchForTests(githubStub(EMAIL));
  try {
    const res = await get(
      githubCallback,
      `http://localhost/api/auth/callback/github?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard");
    assert.ok(res.cookie);
    assert.match(res.cookie, /reponyx-session=/);
  } finally {
    setFetchForTests(null);
  }
});

test("github callback fails cleanly when the provider errors", async () => {
  const state = createOAuthState("github");
  assert.ok(state);
  setFetchForTests(async () => jsonResponse({ error: "bad_code" }, 400));
  try {
    const res = await get(
      githubCallback,
      `http://localhost/api/auth/callback/github?code=bad-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(
      res.location,
      "http://localhost/auth/login?error=oauth_failed"
    );
  } finally {
    setFetchForTests(null);
  }
});

test("an expired state does not verify", async () => {
  const { createHmac } = await import("node:crypto");
  const payload = Buffer.from(
    JSON.stringify({ p: "google", n: "x", e: Date.now() - 1000 })
  ).toString("base64url");
  const sig = createHmac("sha256", "test-session-secret-32-chars-long")
    .update(payload)
    .digest()
    .toString("base64url");
  assert.equal(verifyOAuthState(`${payload}.${sig}`, "google"), false);
});
