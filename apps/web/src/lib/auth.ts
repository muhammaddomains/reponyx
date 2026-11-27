import { createHmac, timingSafeEqual } from "node:crypto";
import { cookies } from "next/headers";
import { DAY_MS, expiresIn, hashToken, randomToken } from "@reponyx/auth";
import {
  createSession,
  findSessionByHash,
  type DbClient,
  type SessionRow,
  type UserRow,
} from "@reponyx/database";
import { ApiError } from "./api-errors";

export const SESSION_COOKIE = "reponyx-session";
export const SESSION_TTL_MS = 30 * DAY_MS;

/**
 * Argon2id hash of the literal string "dummy-password-for-timing".
 * Used when the login email does not exist, so a wrong-email login
 * costs the same as a wrong-password login (no user enumeration by
 * timing). It is a hash of a dummy password — not a secret.
 */
export const DUMMY_PASSWORD_HASH =
  "$argon2id$v=19$m=65536,t=3,p=1$8StcA2VnqM1qbsw7T/dY2Q$Ygcr7knDEk2XzKF5HcQVIjtaxncMANosAyMw6T4X2v4";

export async function createSessionForUser(
  client: DbClient,
  user: UserRow,
  deviceId: string | null
): Promise<{ token: string; session: SessionRow }> {
  const token = randomToken();
  const session = await createSession(client, {
    userId: user.id,
    tokenHash: hashToken(token),
    deviceId,
    expiresAt: expiresIn(SESSION_TTL_MS),
  });
  return { token, session };
}

export function sessionCookie(token: string, expiresAt: string): string {
  const secure = process.env.NODE_ENV === "production" ? "; Secure" : "";
  return `${SESSION_COOKIE}=${token}; HttpOnly; Path=/; SameSite=Lax${secure}; Expires=${new Date(expiresAt).toUTCString()}`;
}

export function clearSessionCookie(): string {
  return `${SESSION_COOKIE}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0`;
}

export interface AuthedSession {
  user: UserRow;
  session: SessionRow;
  token: string;
}

export async function getSessionFromToken(
  client: DbClient,
  token: string | undefined | null
): Promise<AuthedSession | null> {
  if (!token) return null;
  const found = await findSessionByHash(client, hashToken(token));
  if (!found) return null;
  const { user, ...session } = found;
  return { user, session, token };
}

export async function getSessionFromRequest(
  client: DbClient,
  req: Request
): Promise<AuthedSession | null> {
  const cookieHeader = req.headers.get("cookie") ?? "";
  const match = cookieHeader.match(
    new RegExp(`(?:^|;\\s*)${SESSION_COOKIE}=([^;]+)`)
  );
  if (match) {
    const fromCookie = await getSessionFromToken(
      client,
      decodeURIComponent(match[1])
    );
    if (fromCookie) return fromCookie;
  }
  const auth = req.headers.get("authorization");
  if (auth?.startsWith("Bearer ")) {
    return getSessionFromToken(client, auth.slice("Bearer ".length));
  }
  return null;
}

export async function getSessionFromCookies(
  client: DbClient
): Promise<AuthedSession | null> {
  const cookieStore = await cookies();
  return getSessionFromToken(client, cookieStore.get(SESSION_COOKIE)?.value);
}

export async function requireSession(
  client: DbClient,
  req: Request
): Promise<AuthedSession> {
  const session = await getSessionFromRequest(client, req);
  if (!session) throw new ApiError("unauthorized", 401);
  return session;
}

// --- Pending login (2FA step) ----------------------------------------

const PENDING_LOGIN_TTL_MS = 10 * 60 * 1000;

function pendingLoginSecret(): string | null {
  return process.env.SESSION_SECRET ?? null;
}

function b64url(input: string | Buffer): string {
  return Buffer.from(input).toString("base64url");
}

/**
 * Signed token proving the password step passed (10-min TTL). The
 * login response returns it with `step: "2fa_required"` instead of
 * a session; `POST /api/auth/2fa/verify` returns it with the
 * authenticator code, and only then is the session created. The
 * purpose marker (`k: "login"`) keeps it from being accepted as any
 * other signed token (desktop auth code, pending registration).
 * Returns null when SESSION_SECRET is not set — fail closed.
 */
export function createPendingLoginToken(userId: string): string | null {
  const secret = pendingLoginSecret();
  if (!secret) return null;
  const payload = b64url(
    JSON.stringify({
      k: "login",
      u: userId,
      e: Date.now() + PENDING_LOGIN_TTL_MS,
    })
  );
  const sig = b64url(createHmac("sha256", secret).update(payload).digest());
  return `${payload}.${sig}`;
}

/** Verifies a pending-login token and returns the user id, or null. */
export function verifyPendingLoginToken(token: string): string | null {
  const secret = pendingLoginSecret();
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
    ) as { k?: unknown; u?: unknown; e?: unknown };
    if (data.k !== "login") return null;
    if (typeof data.e !== "number" || data.e <= Date.now()) return null;
    if (typeof data.u !== "string" || !data.u) return null;
    return data.u;
  } catch {
    return null;
  }
}
