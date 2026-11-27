import test from "node:test";
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PGlite } from "@electric-sql/pglite";
import { generateTotpCode, generateTotpSecret } from "@reponyx/auth";
import { findUserByEmail, migrate, type DbClient } from "@reponyx/database";
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
import { POST as loginPost } from "../src/app/api/auth/login/route";
import { POST as verify2faPost } from "../src/app/api/auth/2fa/verify/route";
import { POST as exchangePost } from "../src/app/api/auth/desktop/exchange/route";
import { POST as completePost } from "../src/app/api/auth/register/complete/route";
import { GET as limitsGet } from "../src/app/api/me/limits/route";
import { GET as linkedGet } from "../src/app/api/me/linked/route";
import { GET as devicesGet } from "../src/app/api/devices/route";
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
function googleStub(email: string, name?: string, sub = "google-sub-1") {
  return async (input: string): Promise<Response> => {
    if (input === "https://oauth2.googleapis.com/token") {
      return jsonResponse({ access_token: "google-access-token" });
    }
    if (input === "https://openidconnect.googleapis.com/v1/userinfo") {
      return jsonResponse({
        sub,
        email,
        name: name ?? null,
        email_verified: true,
      });
    }
    throw new Error(`unexpected fetch: ${input}`);
  };
}

/** Provider stub: GitHub happy path (hidden email → emails endpoint). */
function githubStub(email: string, id = 12345) {
  return async (input: string): Promise<Response> => {
    if (input === "https://github.com/login/oauth/access_token") {
      return jsonResponse({ access_token: "github-access-token" });
    }
    if (input === "https://api.github.com/user") {
      return jsonResponse({ id, login: "oauthuser", email: null });
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
  url: string,
  headers: Record<string, string> = {}
): Promise<{ status: number; location: string | null; cookie: string | null }> {
  const res = await handler(new Request(url, { headers }));
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
    {
      name: "OAuth User",
      email: EMAIL,
      password: "correct horse battery staple",
    },
    { "x-forwarded-for": "203.0.113.11" }
  );
  assert.equal(reg.status, 201);
  const token = tokenFromEmail(sent[0].html);
  const verify = await post(verifyPost, "/api/auth/verify", { token });
  assert.equal(verify.status, 200);
}

// A second user with TOTP on, for the 2FA + desktop-2FA tests. The
// seed user above stays TOTP-free so the plain login tests keep
// passing.
const TWOFA_EMAIL = "twofa-user@example.com";
const TWOFA_SECRET = generateTotpSecret();
{
  const reg = await post(
    registerPost,
    "/api/auth/register",
    {
      name: "Twofa User",
      email: TWOFA_EMAIL,
      password: "correct horse battery staple",
    },
    { "x-forwarded-for": "203.0.113.13" }
  );
  assert.equal(reg.status, 201);
  const token = tokenFromEmail(sent[1].html);
  const verify = await post(verifyPost, "/api/auth/verify", { token });
  assert.equal(verify.status, 200);
  await client.query("UPDATE users SET totp_secret = $1 WHERE email = $2", [
    TWOFA_SECRET,
    TWOFA_EMAIL,
  ]);
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
  assert.deepEqual(verifyOAuthState(state, "google"), {
    intent: "login",
    client: "web",
  });
  assert.equal(verifyOAuthState(state, "github"), null);
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
      "http://localhost/login?error=oauth_not_configured"
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
    // The login linked the provider: /api/me/linked lists Google.
    const linked = await linkedGet(
      new Request("http://localhost/api/me/linked", {
        headers: { cookie },
      })
    );
    assert.equal(linked.status, 200);
    const linkedBody = (await linked.json()) as {
      linked: { provider: string; email: string }[];
    };
    assert.deepEqual(
      linkedBody.linked.map((row) => row.provider),
      ["google"]
    );
  } finally {
    setFetchForTests(null);
  }
});

test("google callback refuses an unregistered email (no auto-create)", async () => {
  const state = createOAuthState("google");
  assert.ok(state);
  setFetchForTests(
    googleStub("stranger@example.com", undefined, "google-sub-stranger")
  );
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/login?error=oauth_no_account");
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
  assert.equal(res.location, "http://localhost/login?error=oauth_failed");
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
    assert.equal(res.location, "http://localhost/login?error=oauth_failed");
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
  assert.equal(verifyOAuthState(`${payload}.${sig}`, "google"), null);
});

test("google start with ?intent=register signs the register intent", async () => {
  const res = await get(
    googleStart,
    "http://localhost/api/auth/oauth/google?intent=register"
  );
  assert.equal(res.status, 307);
  assert.ok(res.location);
  const state = new URL(res.location).searchParams.get("state");
  assert.ok(state);
  assert.deepEqual(verifyOAuthState(state, "google"), {
    intent: "register",
    client: "web",
  });
});

test("google callback with register intent creates a verified user", async () => {
  const state = createOAuthState("google", "register");
  assert.ok(state);
  setFetchForTests(
    googleStub("oauth-new@example.com", "New User", "google-sub-new")
  );
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { "x-forwarded-for": "203.0.113.12" }
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard");
    assert.ok(res.cookie);
    const created = await findUserByEmail(client, "oauth-new@example.com");
    assert.ok(created);
    assert.equal(created.name, "New User");
    assert.equal(created.email_verified, true);
    // The session works: /api/me/limits answers for the new user.
    const cookie = (res.cookie as string).split(";")[0];
    const limits = await limitsGet(
      new Request("http://localhost/api/me/limits", {
        headers: { cookie },
      })
    );
    assert.equal(limits.status, 200);
    const body = (await limits.json()) as {
      user: { email: string; name: string | null };
    };
    assert.equal(body.user.email, "oauth-new@example.com");
    assert.equal(body.user.name, "New User");
  } finally {
    setFetchForTests(null);
  }
});

test("google callback with register intent refuses an existing email", async () => {
  const state = createOAuthState("google", "register");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL, "OAuth User", "google-sub-other"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { "x-forwarded-for": "203.0.113.13" }
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/register?error=email_taken");
    assert.equal(res.cookie, null);
  } finally {
    setFetchForTests(null);
  }
});

test("google callback with register intent enforces the registration limit", async () => {
  // 203.0.113.11 already registered once (the seed user above).
  const state = createOAuthState("google", "register");
  assert.ok(state);
  setFetchForTests(
    googleStub(
      "oauth-limited@example.com",
      "Limited User",
      "google-sub-limited"
    )
  );
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { "x-forwarded-for": "203.0.113.11" }
    );
    assert.equal(res.status, 307);
    assert.ok(res.location);
    assert.match(
      res.location,
      /^http:\/\/localhost\/register\?error=registration_rate_limited&retry_at=/
    );
    assert.equal(res.cookie, null);
    assert.equal(
      await findUserByEmail(client, "oauth-limited@example.com"),
      null
    );
  } finally {
    setFetchForTests(null);
  }
});

test("github callback with register intent creates via the emails endpoint", async () => {
  const state = createOAuthState("github", "register");
  assert.ok(state);
  setFetchForTests(githubStub("oauth-gh-new@example.com", 67890));
  try {
    const res = await get(
      githubCallback,
      `http://localhost/api/auth/callback/github?code=test-code&state=${encodeURIComponent(state)}`,
      { "x-forwarded-for": "203.0.113.14" }
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard");
    assert.ok(res.cookie);
    const created = await findUserByEmail(client, "oauth-gh-new@example.com");
    assert.ok(created);
    // GitHub's /user had no public name, so the login is the fallback.
    assert.equal(created.name, "oauthuser");
    assert.equal(created.email_verified, true);
  } finally {
    setFetchForTests(null);
  }
});

async function loginCookie(email: string): Promise<string> {
  const res = await loginPost(
    new Request("http://localhost/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email,
        password: "correct horse battery staple",
      }),
    })
  );
  assert.equal(res.status, 200);
  const cookie = res.headers.get("set-cookie") ?? "";
  assert.match(cookie, /reponyx-session=/);
  return cookie.split(";")[0];
}

test("google callback with link intent links the provider for a logged-in user", async () => {
  const cookie = await loginCookie(EMAIL);
  const state = createOAuthState("google", "link");
  assert.ok(state);
  // A different Google account (new sub) with the same email.
  setFetchForTests(googleStub(EMAIL, "OAuth User", "google-sub-2"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { cookie }
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/dashboard?linked=google");
    const linked = await linkedGet(
      new Request("http://localhost/api/me/linked", { headers: { cookie } })
    );
    assert.equal(linked.status, 200);
    const body = (await linked.json()) as {
      linked: { provider: string }[];
    };
    // The seed user also linked GitHub in an earlier test; the point
    // here is that the link flow added Google.
    assert.ok(body.linked.map((row) => row.provider).includes("google"));
  } finally {
    setFetchForTests(null);
  }
});

test("google callback with link intent refuses a different email", async () => {
  const cookie = await loginCookie(EMAIL);
  const state = createOAuthState("google", "link");
  assert.ok(state);
  setFetchForTests(googleStub("someone-else@example.com", "Someone Else"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { cookie }
    );
    assert.equal(res.status, 307);
    assert.equal(
      res.location,
      "http://localhost/dashboard?link_error=email_mismatch"
    );
  } finally {
    setFetchForTests(null);
  }
});

test("google callback with link intent without a session goes to login", async () => {
  const state = createOAuthState("google", "link");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL, "OAuth User"));
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "http://localhost/login?error=session_expired");
  } finally {
    setFetchForTests(null);
  }
});

test("google register without a provider name asks to complete the profile", async () => {
  const state = createOAuthState("google", "register");
  assert.ok(state);
  // The stub returns no name at all — the account must not be created yet.
  setFetchForTests(
    googleStub("oauth-noname@example.com", undefined, "google-sub-noname")
  );
  let pendingToken: string;
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { "x-forwarded-for": "203.0.113.15" }
    );
    assert.equal(res.status, 307);
    assert.ok(res.location);
    assert.match(
      res.location,
      /^http:\/\/localhost\/register\/complete\?token=/
    );
    assert.equal(res.cookie, null);
    assert.equal(
      await findUserByEmail(client, "oauth-noname@example.com"),
      null
    );
    pendingToken = new URL(res.location).searchParams.get("token") ?? "";
    assert.ok(pendingToken);
  } finally {
    setFetchForTests(null);
  }
  // Completing the profile creates the account and logs the user in.
  const done = await post(
    completePost,
    "/api/auth/register/complete",
    { token: pendingToken, name: "No Name User" },
    { "x-forwarded-for": "203.0.113.15" }
  );
  assert.equal(done.status, 201);
  const created = await findUserByEmail(client, "oauth-noname@example.com");
  assert.ok(created);
  assert.equal(created.name, "No Name User");
  assert.equal(created.email_verified, true);
  assert.equal(created.has_password, false);
});

test("complete registration refuses a tampered token and a missing name", async () => {
  const bad = await post(
    completePost,
    "/api/auth/register/complete",
    { token: "tampered.token", name: "Someone" },
    { "x-forwarded-for": "203.0.113.16" }
  );
  assert.equal(bad.status, 404);
  const state = createOAuthState("google", "register");
  assert.ok(state);
  setFetchForTests(
    googleStub("oauth-noname-2@example.com", undefined, "google-sub-noname-2")
  );
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`,
      { "x-forwarded-for": "203.0.113.16" }
    );
    const token = new URL(res.location ?? "").searchParams.get("token") ?? "";
    const noName = await post(
      completePost,
      "/api/auth/register/complete",
      { token, name: "   " },
      { "x-forwarded-for": "203.0.113.16" }
    );
    assert.equal(noName.status, 400);
    assert.equal(
      await findUserByEmail(client, "oauth-noname-2@example.com"),
      null
    );
  } finally {
    setFetchForTests(null);
  }
});

test("desktop start signs the desktop client (and forces the login intent)", async () => {
  const res = await get(
    googleStart,
    "http://localhost/api/auth/oauth/google?client=desktop"
  );
  assert.equal(res.status, 307);
  assert.ok(res.location);
  const state = new URL(res.location).searchParams.get("state");
  assert.ok(state);
  assert.deepEqual(verifyOAuthState(state, "google"), {
    intent: "login",
    client: "desktop",
  });
  // Even ?intent=register cannot turn a desktop start into a registration.
  const reg = await get(
    googleStart,
    "http://localhost/api/auth/oauth/google?client=desktop&intent=register"
  );
  const regState = new URL(reg.location ?? "").searchParams.get("state");
  assert.ok(regState);
  assert.deepEqual(verifyOAuthState(regState, "google"), {
    intent: "login",
    client: "desktop",
  });
});

test("desktop start refuses when the provider is not configured", async () => {
  delete process.env.GOOGLE_CLIENT_ID;
  try {
    const res = await get(
      googleStart,
      "http://localhost/api/auth/oauth/google?client=desktop"
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "reponyx://auth?error=oauth_not_configured");
  } finally {
    process.env.GOOGLE_CLIENT_ID = "google-test-client-id";
  }
});

test("desktop callback redirects to the deep link; exchange mints a session + device", async () => {
  const state = createOAuthState("google", "login", "desktop");
  assert.ok(state);
  setFetchForTests(googleStub(EMAIL, "OAuth User", "google-sub-desktop"));
  let code: string;
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.ok(res.location);
    assert.match(res.location, /^reponyx:\/\/auth\?code=/);
    assert.equal(res.cookie, null);
    code = new URL(res.location).searchParams.get("code") ?? "";
    assert.ok(code);
  } finally {
    setFetchForTests(null);
  }
  // The desktop exchanges the code for a session token (no cookie).
  const exchanged = await post(
    exchangePost,
    "/api/auth/desktop/exchange",
    {
      code,
      deviceKey: "desktop-device-1",
      machineName: "DESKTOP-TEST",
      platform: "Windows 11",
      appVersion: "0.1.0",
    },
    { "x-forwarded-for": "203.0.113.14" }
  );
  assert.equal(exchanged.status, 200);
  const session = exchanged.json.session as { token: string };
  assert.match(session.token, /^[A-Za-z0-9_-]+$/);
  assert.equal((exchanged.json.user as { email: string }).email, EMAIL);
  // The token works as a Bearer token, and the device was registered.
  const limits = await limitsGet(
    new Request("http://localhost/api/me/limits", {
      headers: { authorization: `Bearer ${session.token}` },
    })
  );
  assert.equal(limits.status, 200);
  const devices = await devicesGet(
    new Request("http://localhost/api/devices", {
      headers: { authorization: `Bearer ${session.token}` },
    })
  );
  assert.equal(devices.status, 200);
  const body = (await devices.json()) as {
    devices: { device_key: string; machine_name: string }[];
  };
  assert.deepEqual(
    body.devices.map((d) => d.device_key),
    ["desktop-device-1"]
  );
  assert.equal(body.devices[0].machine_name, "DESKTOP-TEST");
});

test("desktop exchange refuses a tampered code", async () => {
  const res = await post(exchangePost, "/api/auth/desktop/exchange", {
    code: "tampered.code",
  });
  assert.equal(res.status, 404);
  assert.equal((res.json.error as { code: string }).code, "invalid_token");
});

test("desktop callback refuses an unregistered email via the deep link", async () => {
  const state = createOAuthState("google", "login", "desktop");
  assert.ok(state);
  setFetchForTests(
    googleStub(
      "stranger-desktop@example.com",
      undefined,
      "google-sub-stranger-d"
    )
  );
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.equal(res.location, "reponyx://auth?error=oauth_no_account");
    assert.equal(res.cookie, null);
  } finally {
    setFetchForTests(null);
  }
});

test("desktop callback with a 2FA user returns a pending token; verify completes the login", async () => {
  const state = createOAuthState("google", "login", "desktop");
  assert.ok(state);
  setFetchForTests(googleStub(TWOFA_EMAIL, "Twofa User", "google-sub-twofa"));
  let pendingToken: string;
  try {
    const res = await get(
      googleCallback,
      `http://localhost/api/google/oauth/callback?code=test-code&state=${encodeURIComponent(state)}`
    );
    assert.equal(res.status, 307);
    assert.ok(res.location);
    const target = new URL(res.location);
    assert.equal(target.protocol, "reponyx:");
    assert.equal(target.searchParams.get("two_factor_required"), "1");
    pendingToken = target.searchParams.get("pending") ?? "";
    assert.ok(pendingToken);
    assert.equal(res.cookie, null);
  } finally {
    setFetchForTests(null);
  }
  // A wrong code is refused.
  const wrong = await post(
    verify2faPost,
    "/api/auth/2fa/verify",
    { token: pendingToken, code: "000000" },
    { "x-forwarded-for": "203.0.113.17" }
  );
  assert.equal(wrong.status, 401);
  assert.equal((wrong.json.error as { code: string }).code, "invalid_2fa_code");
  // The right code mints the session.
  const ok = await post(
    verify2faPost,
    "/api/auth/2fa/verify",
    {
      token: pendingToken,
      code: generateTotpCode(TWOFA_SECRET),
      deviceKey: "desktop-device-2fa",
      machineName: "DESKTOP-TEST",
      platform: "Windows 11",
      appVersion: "0.1.0",
    },
    { "x-forwarded-for": "203.0.113.17" }
  );
  assert.equal(ok.status, 200);
  const session = ok.json.session as { token: string };
  assert.match(session.token, /^[A-Za-z0-9_-]+$/);
  const limits = await limitsGet(
    new Request("http://localhost/api/me/limits", {
      headers: { authorization: `Bearer ${session.token}` },
    })
  );
  assert.equal(limits.status, 200);
  assert.equal(
    ((await limits.json()) as { user: { email: string } }).user.email,
    TWOFA_EMAIL
  );
});

test("2FA verify refuses a tampered pending token", async () => {
  const res = await post(verify2faPost, "/api/auth/2fa/verify", {
    token: "tampered.token",
    code: "123456",
  });
  assert.equal(res.status, 404);
  assert.equal((res.json.error as { code: string }).code, "invalid_token");
});
