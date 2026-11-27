import { createHmac, timingSafeEqual } from "crypto";
import { NextResponse } from "next/server";
import { hashPassword, randomToken } from "@reponyx/auth";
import {
  createUser,
  findLinkedProviderByProviderUserId,
  findUserByEmail,
  findUserById,
  markUserVerified,
  recordAudit,
  recordRegistrationAttempt,
  upsertLinkedProvider,
  type DbClient,
  type UserRow,
} from "@reponyx/database";
import { ApiError } from "./api-errors";
import {
  createPendingLoginToken,
  createSessionForUser,
  getSessionFromRequest,
  sessionCookie,
} from "./auth";
import {
  checkAccountSwitchAllowed,
  checkRegistrationAllowed,
  clientIp,
  hashIp,
} from "./rate-limits";

export type OAuthProvider = "google" | "github";

/**
 * What the OAuth flow is for. "login" never creates an account — an
 * unregistered login is refused (plan §4). "register" is the register
 * page's flow: an unregistered user is created with the provider's
 * verified email and name, under the same 1-per-IP-per-24h limit as
 * the email form. "link" is the profile's flow: a logged-in user links
 * another provider to the same account, so any of them signs in.
 * The intent rides inside the signed state, so the callback can
 * trust it.
 */
export type OAuthIntent = "login" | "register" | "link";

/**
 * Which client started the OAuth flow. The web finishes in the
 * browser (dashboard + cookie); the desktop finishes through the
 * `reponyx://auth` deep link (short-lived code, exchanged for a
 * session token by the desktop app).
 */
export type OAuthClient = "web" | "desktop";

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
export function createOAuthState(
  provider: OAuthProvider,
  intent: OAuthIntent = "login",
  client: OAuthClient = "web"
): string | null {
  const secret = stateSecret();
  if (!secret) return null;
  const payload = b64url(
    JSON.stringify({
      p: provider,
      i: intent,
      c: client,
      n: randomToken(),
      e: Date.now() + STATE_TTL_MS,
    })
  );
  const sig = b64url(createHmac("sha256", secret).update(payload).digest());
  return `${payload}.${sig}`;
}

/**
 * Verifies the signed state and returns its intent and client, or
 * null when the signature is bad, the provider mismatches, or the
 * state expired.
 */
export function verifyOAuthState(
  token: string,
  provider: OAuthProvider
): { intent: OAuthIntent; client: OAuthClient } | null {
  const secret = stateSecret();
  if (!secret) return null;
  const [payload, sig] = token.split(".");
  if (!payload || !sig) return null;
  const expected = createHmac("sha256", secret).update(payload).digest();
  let given: Buffer;
  try {
    given = Buffer.from(sig, "base64url");
  } catch {
    return null;
  }
  if (given.length !== expected.length) return null;
  if (!timingSafeEqual(given, expected)) return null;
  try {
    const data = JSON.parse(
      Buffer.from(payload, "base64url").toString("utf8")
    ) as { p?: unknown; i?: unknown; c?: unknown; e?: unknown };
    if (data.p !== provider) return null;
    if (typeof data.e !== "number" || data.e <= Date.now()) return null;
    const intent: OAuthIntent =
      data.i === "register" ? "register" : data.i === "link" ? "link" : "login";
    const client: OAuthClient = data.c === "desktop" ? "desktop" : "web";
    return { intent, client };
  } catch {
    return null;
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
): Promise<{ email: string; name: string | null; providerUserId: string }> {
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
  if (typeof data.sub !== "string" || !data.sub) {
    throw new OAuthError("google userinfo returned no sub");
  }
  return {
    email: data.email,
    name: typeof data.name === "string" && data.name ? data.name : null,
    providerUserId: data.sub,
  };
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
): Promise<{ email: string; name: string | null; providerUserId: string }> {
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
  const providerUserId = typeof user.id === "number" ? String(user.id) : null;
  if (!providerUserId) {
    throw new OAuthError("github user fetch returned no id");
  }
  const name =
    (typeof user.name === "string" && user.name) ||
    (typeof user.login === "string" && user.login) ||
    null;
  if (typeof user.email === "string" && user.email) {
    return { email: user.email, name, providerUserId };
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
  return { email: pick.email, name, providerUserId };
}

// --- Shared login/register finish (same rules as the password flows) ---

export type OAuthLoginOutcome =
  | { outcome: "ok"; user: UserRow; token: string; expiresAt: string }
  | { outcome: "no_account" }
  | { outcome: "email_taken" }
  | { outcome: "two_factor_required"; pendingToken: string | null }
  | {
      outcome: "name_required";
      provider: OAuthProvider;
      providerUserId: string;
      email: string;
    };

async function loginExistingUser(
  client: DbClient,
  user: UserRow,
  ipHash: string | null
): Promise<OAuthLoginOutcome> {
  await checkAccountSwitchAllowed(client, {
    userId: user.id,
    deviceKey: null,
    ipHash,
  });
  // 2FA step (W2/W3): the session is never returned until the code verifies.
  // The pending token proves the provider verified the account; the
  // client returns it with the authenticator code to /api/auth/2fa/verify.
  if (user.totp_secret) {
    return {
      outcome: "two_factor_required",
      pendingToken: createPendingLoginToken(user.id),
    };
  }
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

export async function finishOAuthLogin(
  client: DbClient,
  args: {
    email: string;
    name: string | null;
    ipHash: string | null;
    intent: OAuthIntent;
    provider: OAuthProvider;
    providerUserId: string;
  }
): Promise<OAuthLoginOutcome> {
  const { email, name, ipHash, intent, provider, providerUserId } = args;
  // The provider identity is the strongest binding: a linked provider
  // always resolves to its owner, even if the provider's email changed
  // since. Otherwise the same verified email is one account.
  const linked = await findLinkedProviderByProviderUserId(
    client,
    provider,
    providerUserId
  );
  const linkedUser = linked ? await findUserById(client, linked.user_id) : null;
  const user = linkedUser ?? (await findUserByEmail(client, email));

  if (intent === "register") {
    // Already registered through this provider — just sign them in.
    if (linkedUser) return loginExistingUser(client, linkedUser, ipHash);
    // Registration via the provider: an existing email must log in
    // instead; a new one is created under the same 1-per-IP-per-24h
    // limit as the email form, with the provider-verified email and
    // the profile name. The stored password hash is of a random token
    // nobody knows — the account has no usable password until the
    // owner sets one via the reset flow.
    if (user) return { outcome: "email_taken" };
    // Name is mandatory: without a name from the provider the account
    // is not created yet — the user completes the profile first, and
    // only then is the account created and the login allowed.
    if (!name) {
      return { outcome: "name_required", provider, providerUserId, email };
    }
    if (ipHash) await checkRegistrationAllowed(client, ipHash);
    const passwordHash = await hashPassword(randomToken());
    const created = await createUser(client, {
      email,
      passwordHash,
      name,
      hasPassword: false,
    });
    await markUserVerified(client, created.id);
    await upsertLinkedProvider(client, {
      userId: created.id,
      provider,
      providerUserId,
      email,
    });
    if (ipHash) await recordRegistrationAttempt(client, ipHash);
    const { token, session } = await createSessionForUser(
      client,
      created,
      null
    );
    await recordAudit(client, {
      userId: created.id,
      action: "auth.oauth_register",
      ipHash,
    });
    return {
      outcome: "ok",
      user: created,
      token,
      expiresAt: session.expires_at,
    };
  }
  // OAuth NEVER auto-creates an account on login — an unregistered
  // login is refused with a clear message (plan §4).
  if (!user) return { outcome: "no_account" };
  // A login through a provider links it to the account, so the profile
  // shows it and the user can sign in with any of their providers.
  await upsertLinkedProvider(client, {
    userId: user.id,
    provider,
    providerUserId,
    email,
  });
  return loginExistingUser(client, user, ipHash);
}

// --- Link a provider to the signed-in account (profile flow) ---

export type OAuthLinkOutcome =
  | { outcome: "ok"; provider: OAuthProvider }
  | { outcome: "no_session" }
  | { outcome: "email_mismatch" }
  | { outcome: "already_linked" };

export async function finishOAuthLink(
  client: DbClient,
  args: {
    userId: string;
    provider: OAuthProvider;
    providerUserId: string;
    email: string;
    ipHash: string | null;
  }
): Promise<OAuthLinkOutcome> {
  const { userId, provider, providerUserId, email, ipHash } = args;
  const user = await findUserById(client, userId);
  if (!user) return { outcome: "no_session" };
  // A provider can only join the account whose email it verifies —
  // the same email is what makes Google, GitHub and password one account.
  if (email.toLowerCase() !== user.email) {
    return { outcome: "email_mismatch" };
  }
  const existing = await findLinkedProviderByProviderUserId(
    client,
    provider,
    providerUserId
  );
  if (existing && existing.user_id !== userId) {
    return { outcome: "already_linked" };
  }
  await upsertLinkedProvider(client, {
    userId,
    provider,
    providerUserId,
    email,
  });
  await recordAudit(client, {
    userId,
    action: "auth.oauth_link",
    ipHash,
  });
  return { outcome: "ok", provider };
}

// --- Pending OAuth registration (name required to complete the profile) ---

const PENDING_TTL_MS = 10 * 60 * 1000;

export interface PendingRegistration {
  provider: OAuthProvider;
  providerUserId: string;
  email: string;
}

/**
 * Signed token proving this provider verified this email (10-min TTL).
 * The complete-profile page returns it with the name the provider
 * did not supply, and the account is created only then.
 */
export function createPendingRegistrationToken(
  pending: PendingRegistration
): string | null {
  const secret = stateSecret();
  if (!secret) return null;
  const payload = b64url(
    JSON.stringify({ ...pending, e: Date.now() + PENDING_TTL_MS })
  );
  const sig = b64url(createHmac("sha256", secret).update(payload).digest());
  return `${payload}.${sig}`;
}

export function verifyPendingRegistrationToken(
  token: string
): PendingRegistration | null {
  const secret = stateSecret();
  if (!secret) return null;
  const [payload, sig] = token.split(".");
  if (!payload || !sig) return null;
  const expected = createHmac("sha256", secret).update(payload).digest();
  let given: Buffer;
  try {
    given = Buffer.from(sig, "base64url");
  } catch {
    return null;
  }
  if (given.length !== expected.length) return null;
  if (!timingSafeEqual(given, expected)) return null;
  try {
    const data = JSON.parse(
      Buffer.from(payload, "base64url").toString("utf8")
    ) as {
      provider?: unknown;
      providerUserId?: unknown;
      email?: unknown;
      e?: unknown;
    };
    if (typeof data.e !== "number" || data.e <= Date.now()) return null;
    if (data.provider !== "google" && data.provider !== "github") return null;
    if (typeof data.providerUserId !== "string" || !data.providerUserId) {
      return null;
    }
    if (typeof data.email !== "string" || !data.email) return null;
    return {
      provider: data.provider,
      providerUserId: data.providerUserId,
      email: data.email,
    };
  } catch {
    return null;
  }
}

// --- Desktop auth code (deep-link finish) -----------------------------

const DESKTOP_CODE_TTL_MS = 10 * 60 * 1000;

/**
 * Signed code proving the provider verified this user (10-min TTL).
 * The OAuth callback redirects the desktop flow to
 * `reponyx://auth?code=…` with it; the desktop app exchanges it at
 * `POST /api/auth/desktop/exchange` for a session token. The raw
 * session token never travels in the deep-link URL — only this
 * short-lived code. The purpose marker (`k: "desktop"`) keeps it
 * from being accepted as any other signed token (pending login,
 * pending registration).
 */
export function createDesktopAuthCode(userId: string): string | null {
  const secret = stateSecret();
  if (!secret) return null;
  const payload = b64url(
    JSON.stringify({
      k: "desktop",
      u: userId,
      e: Date.now() + DESKTOP_CODE_TTL_MS,
    })
  );
  const sig = b64url(createHmac("sha256", secret).update(payload).digest());
  return `${payload}.${sig}`;
}

/** Verifies a desktop auth code and returns the user id, or null. */
export function verifyDesktopAuthCode(code: string): string | null {
  const secret = stateSecret();
  if (!secret) return null;
  const [payload, sig] = code.split(".");
  if (!payload || !sig) return null;
  const expected = createHmac("sha256", secret).update(payload).digest();
  let given: Buffer;
  try {
    given = Buffer.from(sig, "base64url");
  } catch {
    return null;
  }
  if (given.length !== expected.length) return null;
  if (!timingSafeEqual(given, expected)) return null;
  try {
    const data = JSON.parse(
      Buffer.from(payload, "base64url").toString("utf8")
    ) as { k?: unknown; u?: unknown; e?: unknown };
    if (data.k !== "desktop") return null;
    if (typeof data.e !== "number" || data.e <= Date.now()) return null;
    if (typeof data.u !== "string" || !data.u) return null;
    return data.u;
  } catch {
    return null;
  }
}

// --- Shared callback handler (both providers, different paths) ---

function oauthFail(req: Request, page: string, error: string): NextResponse {
  return NextResponse.redirect(new URL(`${page}?error=${error}`, req.url));
}

/**
 * The desktop flow finishes through the `reponyx://auth` deep link
 * instead of a web page: the desktop app picks the URL up and shows
 * the login screen (or the 2FA field) itself.
 */
function desktopFail(error: string): NextResponse {
  return NextResponse.redirect(
    new URL(`reponyx://auth?error=${encodeURIComponent(error)}`)
  );
}

function dashboardLinkError(req: Request, error: string): NextResponse {
  const target = new URL("/dashboard", req.url);
  target.searchParams.set("link_error", error);
  return NextResponse.redirect(target);
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
  const verified = state ? verifyOAuthState(state, provider) : null;
  // A bad state carries no trustworthy intent — fail closed to login.
  if (!config || !code || !state || !verified) {
    return oauthFail(req, "/login", "oauth_failed");
  }
  const { intent, client: oauthClient } = verified;
  const isDesktop = oauthClient === "desktop";
  // The desktop only logs in — linking happens on the web dashboard,
  // and a desktop start route never signs a link intent.
  if (isDesktop && intent === "link") {
    return desktopFail("oauth_failed");
  }
  const failPage = intent === "register" ? "/register" : "/login";
  try {
    const { accessToken } =
      provider === "google"
        ? await exchangeGoogleCode(config, code)
        : await exchangeGitHubCode(config, code);
    const { email, name, providerUserId } =
      provider === "google"
        ? await fetchGoogleProfile(accessToken)
        : await fetchGitHubProfile(accessToken);
    const client = getDb();
    const ip = clientIp(req);
    const ipHash = ip ? hashIp(ip) : null;
    if (intent === "link") {
      const session = await getSessionFromRequest(client, req);
      if (!session) {
        return oauthFail(req, "/login", "session_expired");
      }
      const link = await finishOAuthLink(client, {
        userId: session.user.id,
        provider,
        providerUserId,
        email,
        ipHash,
      });
      if (link.outcome === "email_mismatch") {
        return dashboardLinkError(req, "email_mismatch");
      }
      if (link.outcome === "already_linked") {
        return dashboardLinkError(req, "already_linked");
      }
      if (link.outcome === "no_session") {
        return oauthFail(req, "/login", "session_expired");
      }
      const target = new URL("/dashboard", req.url);
      target.searchParams.set("linked", provider);
      return NextResponse.redirect(target);
    }
    const result = await finishOAuthLogin(client, {
      email,
      name,
      ipHash,
      intent,
      provider,
      providerUserId,
    });
    if (result.outcome === "no_account") {
      return isDesktop
        ? desktopFail("oauth_no_account")
        : oauthFail(req, "/login", "oauth_no_account");
    }
    if (result.outcome === "email_taken") {
      return isDesktop
        ? desktopFail("email_taken")
        : oauthFail(req, "/register", "email_taken");
    }
    if (result.outcome === "two_factor_required") {
      if (isDesktop) {
        if (!result.pendingToken) return desktopFail("oauth_failed");
        const target = new URL("reponyx://auth");
        target.searchParams.set("two_factor_required", "1");
        target.searchParams.set("pending", result.pendingToken);
        return NextResponse.redirect(target);
      }
      return oauthFail(req, "/login", "two_factor_required");
    }
    if (result.outcome === "name_required") {
      const pending = createPendingRegistrationToken({
        provider: result.provider,
        providerUserId: result.providerUserId,
        email: result.email,
      });
      if (!pending) {
        return isDesktop
          ? desktopFail("oauth_failed")
          : oauthFail(req, "/register", "oauth_failed");
      }
      if (isDesktop) {
        const target = new URL("reponyx://auth");
        target.searchParams.set("register_complete", pending);
        return NextResponse.redirect(target);
      }
      const target = new URL("/register/complete", req.url);
      target.searchParams.set("token", pending);
      return NextResponse.redirect(target);
    }
    if (isDesktop) {
      const desktopCode = createDesktopAuthCode(result.user.id);
      if (!desktopCode) return desktopFail("oauth_failed");
      const target = new URL("reponyx://auth");
      target.searchParams.set("code", desktopCode);
      return NextResponse.redirect(target);
    }
    const res = NextResponse.redirect(new URL("/dashboard", req.url));
    res.headers.set(
      "Set-Cookie",
      sessionCookie(result.token, result.expiresAt)
    );
    return res;
  } catch (err) {
    if (
      err instanceof ApiError &&
      (err.code === "account_switch_blocked" ||
        err.code === "registration_rate_limited")
    ) {
      const retryAt = (err.data as { retry_at?: string } | undefined)?.retry_at;
      if (isDesktop) {
        const target = new URL("reponyx://auth");
        target.searchParams.set("error", err.code);
        if (retryAt) target.searchParams.set("retry_at", retryAt);
        return NextResponse.redirect(target);
      }
      const target = new URL(
        err.code === "registration_rate_limited" ? "/register" : "/login",
        req.url
      );
      target.searchParams.set("error", err.code);
      if (retryAt) target.searchParams.set("retry_at", retryAt);
      return NextResponse.redirect(target);
    }
    console.error(`${provider} oauth callback failed`, err);
    return isDesktop
      ? desktopFail("oauth_failed")
      : oauthFail(req, failPage, "oauth_failed");
  }
}
