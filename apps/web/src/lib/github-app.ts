import { importPKCS8, SignJWT } from "jose";
import { ApiError } from "./api-errors";

/**
 * GitHub App client for the web (W4): the web is the single holder of
 * the App private key, and mints installation tokens with it. The key
 * never leaves the server — the local broker asks the web to mint
 * instead of signing locally. Mirrors the broker's old `tokens.rs`
 * semantics (RS256 App JWT, 9-minute lifetime, installation-token
 * mint) and the `setFetchForTests` seam from `oauth.ts`.
 */

export class GitHubAppError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
  }
}

export interface GitHubAppConfig {
  appId: string;
  privateKeyPem: string;
}

/**
 * Reads the App credentials from the environment on every call (no
 * caching — a rotated key takes effect on the next request).
 * Returns null when either value is missing: callers fail closed
 * with `not_configured`. A PEM pasted with literal `\n` sequences
 * (single-line env values) is normalized to real newlines — base64
 * never contains a backslash, so this cannot corrupt a real key.
 */
export function getGitHubAppConfig(): GitHubAppConfig | null {
  const appId = process.env.REPONYX_GITHUB_APP_ID;
  let privateKeyPem = process.env.REPONYX_GITHUB_APP_PRIVATE_KEY;
  if (!appId || !privateKeyPem) return null;
  if (!privateKeyPem.includes("\n") && privateKeyPem.includes("\\n")) {
    privateKeyPem = privateKeyPem.replace(/\\n/g, "\n");
  }
  return { appId, privateKeyPem };
}

export interface AppInstallation {
  id: number;
  account: { id: number; login: string };
}

export interface InstallationToken {
  token: string;
  expiresAt: string;
}

type Fetcher = (input: string, init?: RequestInit) => Promise<Response>;

let fetchOverride: Fetcher | null = null;

/** Test-only override for the GitHub API calls. */
export function setFetchForTests(fetcher: Fetcher | null): void {
  fetchOverride = fetcher;
}

function httpFetch(input: string, init?: RequestInit): Promise<Response> {
  const fetcher = fetchOverride ?? fetch;
  return fetcher(input, init);
}

const GITHUB_API_BASE = "https://api.github.com";

/** Sign a GitHub App JWT (RS256, 9-minute lifetime, iat backdated 60 s for clock skew). */
async function appJwt(config: GitHubAppConfig): Promise<string> {
  let key;
  try {
    key = await importPKCS8(config.privateKeyPem, "RS256");
  } catch {
    throw new GitHubAppError(
      500,
      "GitHub App private key is not a valid PEM key"
    );
  }
  const now = Math.floor(Date.now() / 1000);
  return new SignJWT({})
    .setProtectedHeader({ alg: "RS256" })
    .setIssuer(config.appId)
    .setIssuedAt(now - 60)
    .setExpirationTime(now + 540)
    .sign(key);
}

function githubHeaders(jwt: string): Record<string, string> {
  return {
    Authorization: `Bearer ${jwt}`,
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
}

async function readErrorMessage(res: Response): Promise<string> {
  const body = (await res.json().catch(() => ({}))) as {
    message?: unknown;
  };
  return typeof body.message === "string"
    ? body.message
    : `GitHub API error ${res.status}`;
}

/** Get one installation of this App by id, or null when GitHub says it does not exist. */
export async function getAppInstallation(
  config: GitHubAppConfig,
  installationId: number
): Promise<AppInstallation | null> {
  const jwt = await appJwt(config);
  const res = await httpFetch(
    `${GITHUB_API_BASE}/app/installations/${installationId}`,
    { headers: githubHeaders(jwt) }
  );
  if (res.status === 404) return null;
  if (!res.ok) {
    throw new GitHubAppError(res.status, await readErrorMessage(res));
  }
  const body = (await res.json().catch(() => null)) as {
    id?: unknown;
    account?: { id?: unknown; login?: unknown };
  } | null;
  if (
    !body ||
    typeof body.id !== "number" ||
    typeof body.account?.id !== "number" ||
    typeof body.account?.login !== "string"
  ) {
    throw new GitHubAppError(
      500,
      "GitHub API returned an installation without an account"
    );
  }
  return {
    id: body.id,
    account: { id: body.account.id, login: body.account.login },
  };
}

/** Mint an installation access token from GitHub. The token is returned, never logged or stored. */
export async function mintInstallationToken(
  config: GitHubAppConfig,
  installationId: number
): Promise<InstallationToken> {
  const jwt = await appJwt(config);
  const res = await httpFetch(
    `${GITHUB_API_BASE}/app/installations/${installationId}/access_tokens`,
    { method: "POST", headers: githubHeaders(jwt) }
  );
  if (!res.ok) {
    throw new GitHubAppError(res.status, await readErrorMessage(res));
  }
  const body = (await res.json().catch(() => ({}))) as {
    token?: unknown;
    expires_at?: unknown;
  };
  if (typeof body.token !== "string" || typeof body.expires_at !== "string") {
    throw new GitHubAppError(
      500,
      "GitHub API returned an unparsable token response"
    );
  }
  return { token: body.token, expiresAt: body.expires_at };
}

/** Maps a GitHub App failure onto the structured error contract: the client sees `internal_error`, never GitHub's text. */
export function githubErrorResponse(err: unknown): ApiError {
  if (err instanceof GitHubAppError) {
    return new ApiError("internal_error", 502);
  }
  throw err;
}
