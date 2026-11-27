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
