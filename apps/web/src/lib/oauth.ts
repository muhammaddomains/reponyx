import { createHmac, timingSafeEqual } from "crypto";
import { NextResponse } from "next/server";
import { randomToken } from "@reponyx/auth";
import {
  findUserByEmail,
  markUserVerified,
  recordAudit,
  type DbClient,
  type UserRow,
} from "@reponyx/database";
import { ApiError } from "./api-errors";
import { createSessionForUser, sessionCookie } from "./auth";
import { checkAccountSwitchAllowed, clientIp, hashIp } from "./rate-limits";

export type OAuthProvider = "google" | "github";

export interface OAuthProviderConfig {
  provider: OAuthProvider;
  clientId: string;
  clientSecret: string;
  redirectUri: string;
  authorizationUrl: string;
  scope: string;
}

export function getAppUrl(): string {
  return process.env.NEXT_PUBLIC_APP_URL ?? "http://localhost:3000";
}

/**
 * Provider credentials live in the Vercel env. Returns null when the
 * provider is not configured — the UI then hides its button and the
 * start route refuses instead of sending the user to a dead end.
 */
export function getOAuthProviderConfig(
  provider: OAuthProvider
): OAuthProviderConfig | null {
  const appUrl = getAppUrl();
  if (provider === "google") {
    const clientId = process.env.GOOGLE_CLIENT_ID;
    const clientSecret = process.env.GOOGLE_CLIENT_SECRET;
    if (!clientId || !clientSecret) return null;
    return {
      provider,
      clientId,
      clientSecret,
      // Exact callback URL the user registered (plan §12 W2).
      redirectUri: `${appUrl}/api/google/oauth/callback`,
      authorizationUrl: "https://accounts.google.com/o/oauth2/v2/auth",
      scope: "openid email profile",
    };
  }
  const clientId = process.env.GITHUB_OAUTH_CLIENT_ID;
  const clientSecret = process.env.GITHUB_OAUTH_CLIENT_SECRET;
  if (!clientId || !clientSecret) return null;
  return {
    provider,
    clientId,
    clientSecret,
    // Exact callback URL the user registered (plan §12 W2).
    redirectUri: `${appUrl}/api/auth/callback/github`,
    authorizationUrl: "https://github.com/login/oauth/authorize",
    scope: "user:email",
  };
}

export function buildAuthorizationUrl(
  config: OAuthProviderConfig,
  state: string
): string {
  const url = new URL(config.authorizationUrl);
  url.searchParams.set("client_id", config.clientId);
  url.searchParams.set("redirect_uri", config.redirectUri);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", config.scope);
  url.searchParams.set("state", state);
  return url.toString();
}

// --- OAuth state (CSRF protection), HMAC-signed with SESSION_SECRET ---

const STATE_TTL_MS = 10 * 60 * 1000;

function stateSecret(): string | null {
  return process.env.SESSION_SECRET ?? null;
}

function b64url(input: string | Buffer): string {
  return Buffer.from(input).toString("base64url");
}

/** Returns null when SESSION_SECRET is not set — fail closed. */
export function createOAuthState(provider: OAuthProvider): string | null {
  const secret = stateSecret();
  if (!secret) return null;
  const payload = b64url(
    JSON.stringify({
      p: provider,
      n: randomToken(),
      e: Date.now() + STATE_TTL_MS,
    })
  );
  const sig = b64url(createHmac("sha256", secret).update(payload).digest());
  return `${payload}.${sig}`;
}

export function verifyOAuthState(
  token: string,
  provider: OAuthProvider
): boolean {
  const secret = stateSecret();
  if (!secret) return false;
  const [payload, sig] = token.split(".");
  if (!payload || !sig) return false;
  const expected = createHmac("sha256", secret).update(payload).digest();
  let given: Buffer;
  try {
    given = Buffer.from(sig, "base64url");
  } catch {
    return false;
  }
  if (given.length !== expected.length) return false;
  if (!timingSafeEqual(given, expected)) return false;
  try {
    const data = JSON.parse(
      Buffer.from(payload, "base64url").toString("utf8")
    ) as { p?: unknown; e?: unknown };
    return (
      data.p === provider && typeof data.e === "number" && data.e > Date.now()
    );
  } catch {
    return false;
  }
}

// --- Provider token exchange + profile (fetch is injectable for tests) ---

export class OAuthError extends Error {}

type Fetcher = (input: string, init?: RequestInit) => Promise<Response>;

let fetchOverride: Fetcher | null = null;

/** Test-only override for the provider HTTP calls. */
export function setFetchForTests(fetcher: Fetcher | null): void {
  fetchOverride = fetcher;
}

function httpFetch(input: string, init?: RequestInit): Promise<Response> {
  const fetcher = fetchOverride ?? fetch;
  return fetcher(input, init);
}

const FETCH_TIMEOUT_MS = 15_000;

async function readJson(res: Response): Promise<Record<string, unknown>> {
  return (await res.json().catch(() => ({}))) as Record<string, unknown>;
}

export async function exchangeGoogleCode(
  config: OAuthProviderConfig,
  code: string
): Promise<{ accessToken: string }> {
  const res = await httpFetch("https://oauth2.googleapis.com/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      code,
      client_id: config.clientId,
      client_secret: config.clientSecret,
      redirect_uri: config.redirectUri,
      grant_type: "authorization_code",
    }),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) throw new OAuthError("google token exchange failed");
  const data = await readJson(res);
  if (typeof data.access_token !== "string" || !data.access_token) {
    throw new OAuthError("google token exchange returned no token");
  }
  return { accessToken: data.access_token };
}

export async function fetchGoogleProfile(
  accessToken: string
): Promise<{ email: string }> {
  const res = await httpFetch(
    "https://openidconnect.googleapis.com/v1/userinfo",
    {
      headers: { Authorization: `Bearer ${accessToken}` },
      signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
    }
  );
  if (!res.ok) throw new OAuthError("google userinfo failed");
  const data = await readJson(res);
  if (typeof data.email !== "string" || !data.email) {
    throw new OAuthError("google userinfo returned no email");
  }
  return { email: data.email };
}

export async function exchangeGitHubCode(
  config: OAuthProviderConfig,
  code: string
): Promise<{ accessToken: string }> {
  const res = await httpFetch("https://github.com/login/oauth/access_token", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({
      client_id: config.clientId,
      client_secret: config.clientSecret,
      code,
      redirect_uri: config.redirectUri,
    }),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) throw new OAuthError("github token exchange failed");
  const data = await readJson(res);
  if (typeof data.access_token !== "string" || !data.access_token) {
    throw new OAuthError("github token exchange returned no token");
  }
  return { accessToken: data.access_token };
}

export async function fetchGitHubProfile(
  accessToken: string
): Promise<{ email: string }> {
  const headers = {
    Authorization: `Bearer ${accessToken}`,
    Accept: "application/vnd.github+json",
  };
  const userRes = await httpFetch("https://api.github.com/user", {
    headers,
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!userRes.ok) throw new OAuthError("github user fetch failed");
  const user = await readJson(userRes);
  if (typeof user.email === "string" && user.email) {
    return { email: user.email };
  }
  // GitHub hides the email on the user object unless it is public —
  // the dedicated emails endpoint lists the verified ones.
  const emailsRes = await httpFetch("https://api.github.com/user/emails", {
    headers,
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!emailsRes.ok) throw new OAuthError("github emails fetch failed");
  const emails = (await emailsRes.json().catch(() => [])) as {
    email: string;
    primary?: boolean;
    verified?: boolean;
  }[];
  const pick =
    emails.find((e) => e.primary && e.verified) ??
    emails.find((e) => e.verified);
  if (!pick) throw new OAuthError("github returned no verified email");
  return { email: pick.email };
}

// --- Shared login finish (same rules as the password login) ---

export type OAuthLoginOutcome =
  | { outcome: "ok"; user: UserRow; token: string; expiresAt: string }
  | { outcome: "no_account" }
  | { outcome: "two_factor_required" };

export async function finishOAuthLogin(
  client: DbClient,
  email: string,
  ipHash: string | null
): Promise<OAuthLoginOutcome> {
  const user = await findUserByEmail(client, email);
  // OAuth NEVER auto-creates an account — an unregistered login is
  // refused with a clear message (plan §4).
  if (!user) return { outcome: "no_account" };
  await checkAccountSwitchAllowed(client, {
    userId: user.id,
    deviceKey: null,
    ipHash,
  });
  // 2FA step (W2/W3): the session is never returned until the code verifies.
  if (user.totp_secret) return { outcome: "two_factor_required" };
  // The provider verified this email address, so it satisfies the
  // verification requirement for an already-registered account.
  if (!user.email_verified) await markUserVerified(client, user.id);
  const { token, session } = await createSessionForUser(client, user, null);
  await recordAudit(client, {
    userId: user.id,
    action: "auth.oauth_login",
    ipHash,
  });
  return { outcome: "ok", user, token, expiresAt: session.expires_at };
}

// --- Shared callback handler (both providers, different paths) ---

function loginFail(req: Request, error: string): NextResponse {
  return NextResponse.redirect(new URL(`/auth/login?error=${error}`, req.url));
}

export async function handleOAuthCallback(
  req: Request,
  provider: OAuthProvider,
  getDb: () => DbClient
): Promise<NextResponse> {
  const url = new URL(req.url);
  const code = url.searchParams.get("code");
  const state = url.searchParams.get("state");
  const config = getOAuthProviderConfig(provider);
  if (!config || !code || !state || !verifyOAuthState(state, provider)) {
    return loginFail(req, "oauth_failed");
  }
  try {
    const { accessToken } =
      provider === "google"
        ? await exchangeGoogleCode(config, code)
        : await exchangeGitHubCode(config, code);
    const { email } =
      provider === "google"
        ? await fetchGoogleProfile(accessToken)
        : await fetchGitHubProfile(accessToken);
    const client = getDb();
    const ip = clientIp(req);
    const result = await finishOAuthLogin(
      client,
      email,
      ip ? hashIp(ip) : null
    );
    if (result.outcome === "no_account") {
      return loginFail(req, "oauth_no_account");
    }
    if (result.outcome === "two_factor_required") {
      return loginFail(req, "two_factor_required");
    }
    const res = NextResponse.redirect(new URL("/dashboard", req.url));
    res.headers.set(
      "Set-Cookie",
      sessionCookie(result.token, result.expiresAt)
    );
    return res;
  } catch (err) {
    if (err instanceof ApiError && err.code === "account_switch_blocked") {
      const retryAt = (err.data as { retry_at?: string } | undefined)?.retry_at;
      const target = new URL("/auth/login", req.url);
      target.searchParams.set("error", "account_switch_blocked");
      if (retryAt) target.searchParams.set("retry_at", retryAt);
      return NextResponse.redirect(target);
    }
    console.error(`${provider} oauth callback failed`, err);
    return loginFail(req, "oauth_failed");
  }
}
