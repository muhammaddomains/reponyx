import type { DbClient } from "./client";
import type {
  AdminUserRow,
  AuditLogRow,
  DeviceRow,
  EmailVerificationTokenRow,
  InstallationRow,
  LinkedGithubAccountRow,
  LinkedProviderRow,
  PackageRow,
  PasswordResetTokenRow,
  SessionRow,
  SubscriptionRow,
  TotpBackupCodeRow,
  UserRow,
} from "./types";

/**
 * Repository functions over `DbClient`. Every statement carries a
 * `/* name *\/` marker as its first line so tests can dispatch on it
 * (see test/queries.test.ts) — the marker is a SQL comment, invisible
 * to Postgres.
 */

// --- users ---

export async function createUser(
  client: DbClient,
  args: {
    email: string;
    passwordHash: string;
    name?: string | null;
    hasPassword?: boolean;
  }
): Promise<UserRow> {
  const rows = await client.query<UserRow>(
    `/* createUser */
     INSERT INTO users (email, password_hash, name, has_password)
     VALUES ($1, $2, $3, $4)
     RETURNING *`,
    [
      args.email.toLowerCase(),
      args.passwordHash,
      args.name ?? null,
      args.hasPassword ?? true,
    ]
  );
  const row = rows[0];
  if (!row) throw new Error("createUser returned no row");
  return row;
}

export async function findUserByEmail(
  client: DbClient,
  email: string
): Promise<UserRow | null> {
  const rows = await client.query<UserRow>(
    `/* findUserByEmail */ SELECT * FROM users WHERE email = $1`,
    [email.toLowerCase()]
  );
  return rows[0] ?? null;
}

export async function findUserById(
  client: DbClient,
  id: string
): Promise<UserRow | null> {
  const rows = await client.query<UserRow>(
    `/* findUserById */ SELECT * FROM users WHERE id = $1`,
    [id]
  );
  return rows[0] ?? null;
}

export async function markUserVerified(
  client: DbClient,
  id: string
): Promise<UserRow> {
  const rows = await client.query<UserRow>(
    `/* markUserVerified */
     UPDATE users SET email_verified = true, updated_at = now()
     WHERE id = $1 RETURNING *`,
    [id]
  );
  const row = rows[0];
  if (!row) throw new Error("markUserVerified returned no row");
  return row;
}

export async function setUserPassword(
  client: DbClient,
  args: { userId: string; passwordHash: string }
): Promise<void> {
  await client.query(
    `/* setUserPassword */
     UPDATE users SET password_hash = $2, has_password = true, updated_at = now() WHERE id = $1`,
    [args.userId, args.passwordHash]
  );
}

export async function setUserHasPassword(
  client: DbClient,
  args: { userId: string; hasPassword: boolean }
): Promise<void> {
  await client.query(
    `/* setUserHasPassword */
     UPDATE users SET has_password = $2, updated_at = now() WHERE id = $1`,
    [args.userId, args.hasPassword]
  );
}

export async function updateUserName(
  client: DbClient,
  args: { userId: string; name: string }
): Promise<UserRow> {
  const rows = await client.query<UserRow>(
    `/* updateUserName */
     UPDATE users SET name = $2, updated_at = now() WHERE id = $1 RETURNING *`,
    [args.userId, args.name]
  );
  const row = rows[0];
  if (!row) throw new Error("updateUserName returned no row");
  return row;
}

export async function deleteUser(client: DbClient, id: string): Promise<void> {
  await client.query(`/* deleteUser */ DELETE FROM users WHERE id = $1`, [id]);
}

// --- linked providers (one account, many sign-in methods) ---

export async function upsertLinkedProvider(
  client: DbClient,
  args: {
    userId: string;
    provider: string;
    providerUserId: string;
    email: string;
  }
): Promise<LinkedProviderRow> {
  const rows = await client.query<LinkedProviderRow>(
    `/* upsertLinkedProvider */
     INSERT INTO linked_providers (user_id, provider, provider_user_id, email)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (user_id, provider) DO UPDATE
       SET provider_user_id = EXCLUDED.provider_user_id,
           email = EXCLUDED.email
     RETURNING *`,
    [args.userId, args.provider, args.providerUserId, args.email.toLowerCase()]
  );
  const row = rows[0];
  if (!row) throw new Error("upsertLinkedProvider returned no row");
  return row;
}

export async function findLinkedProviderByProviderUserId(
  client: DbClient,
  provider: string,
  providerUserId: string
): Promise<LinkedProviderRow | null> {
  const rows = await client.query<LinkedProviderRow>(
    `/* findLinkedProviderByProviderUserId */
     SELECT * FROM linked_providers
     WHERE provider = $1 AND provider_user_id = $2`,
    [provider, providerUserId]
  );
  return rows[0] ?? null;
}

export async function listLinkedProvidersForUser(
  client: DbClient,
  userId: string
): Promise<LinkedProviderRow[]> {
  return client.query<LinkedProviderRow>(
    `/* listLinkedProvidersForUser */
     SELECT * FROM linked_providers WHERE user_id = $1 ORDER BY provider`,
    [userId]
  );
}

export async function deleteLinkedProvider(
  client: DbClient,
  userId: string,
  provider: string
): Promise<void> {
  await client.query(
    `/* deleteLinkedProvider */
     DELETE FROM linked_providers WHERE user_id = $1 AND provider = $2`,
    [userId, provider]
  );
}

// --- email verification tokens ---

export async function createEmailVerificationToken(
  client: DbClient,
  args: { userId: string; tokenHash: string; expiresAt: Date }
): Promise<EmailVerificationTokenRow> {
  const rows = await client.query<EmailVerificationTokenRow>(
    `/* createEmailVerificationToken */
     INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
     VALUES ($1, $2, $3) RETURNING *`,
    [args.userId, args.tokenHash, args.expiresAt.toISOString()]
  );
  const row = rows[0];
  if (!row) throw new Error("createEmailVerificationToken returned no row");
  return row;
}

export async function findValidVerificationToken(
  client: DbClient,
  tokenHash: string
): Promise<EmailVerificationTokenRow | null> {
  const rows = await client.query<EmailVerificationTokenRow>(
    `/* findValidVerificationToken */
     SELECT * FROM email_verification_tokens
     WHERE token_hash = $1 AND consumed_at IS NULL AND expires_at > now()`,
    [tokenHash]
  );
  return rows[0] ?? null;
}

export async function consumeVerificationToken(
  client: DbClient,
  id: string
): Promise<void> {
  await client.query(
    `/* consumeVerificationToken */
     UPDATE email_verification_tokens SET consumed_at = now() WHERE id = $1`,
    [id]
  );
}

// --- password reset tokens ---

export async function createPasswordResetToken(
  client: DbClient,
  args: { userId: string; tokenHash: string; expiresAt: Date }
): Promise<PasswordResetTokenRow> {
  const rows = await client.query<PasswordResetTokenRow>(
    `/* createPasswordResetToken */
     INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
     VALUES ($1, $2, $3) RETURNING *`,
    [args.userId, args.tokenHash, args.expiresAt.toISOString()]
  );
  const row = rows[0];
  if (!row) throw new Error("createPasswordResetToken returned no row");
  return row;
}

export async function findValidResetToken(
  client: DbClient,
  tokenHash: string
): Promise<PasswordResetTokenRow | null> {
  const rows = await client.query<PasswordResetTokenRow>(
    `/* findValidResetToken */
     SELECT * FROM password_reset_tokens
     WHERE token_hash = $1 AND consumed_at IS NULL AND expires_at > now()`,
    [tokenHash]
  );
  return rows[0] ?? null;
}

/**
 * Claims a reset token for use: marks it consumed only if it is
 * still unconsumed, and reports whether this call claimed it.
 * The `consumed_at IS NULL` guard makes the claim atomic — two
 * requests racing the same link cannot both use it.
 */
export async function consumeResetToken(
  client: DbClient,
  id: string
): Promise<boolean> {
  const rows = await client.query<{ id: string }>(
    `/* consumeResetToken */
     UPDATE password_reset_tokens SET consumed_at = now()
     WHERE id = $1 AND consumed_at IS NULL
     RETURNING id`,
    [id]
  );
  return rows.length > 0;
}

// --- sessions ---

export async function createSession(
  client: DbClient,
  args: {
    userId: string;
    tokenHash: string;
    deviceId: string | null;
    expiresAt: Date;
  }
): Promise<SessionRow> {
  const rows = await client.query<SessionRow>(
    `/* createSession */
     INSERT INTO sessions (user_id, token_hash, device_id, expires_at)
     VALUES ($1, $2, $3, $4) RETURNING *`,
    [args.userId, args.tokenHash, args.deviceId, args.expiresAt.toISOString()]
  );
  const row = rows[0];
  if (!row) throw new Error("createSession returned no row");
  return row;
}

export async function findSessionByHash(
  client: DbClient,
  tokenHash: string
): Promise<(SessionRow & { user: UserRow }) | null> {
  const rows = await client.query<SessionRow & { user: UserRow }>(
    `/* findSessionByHash */
     SELECT s.*, to_jsonb(u.*) AS user FROM sessions s
     JOIN users u ON u.id = s.user_id
     WHERE s.token_hash = $1 AND s.expires_at > now()`,
    [tokenHash]
  );
  return rows[0] ?? null;
}

export async function revokeSession(
  client: DbClient,
  tokenHash: string
): Promise<void> {
  await client.query(
    `/* revokeSession */ DELETE FROM sessions WHERE token_hash = $1`,
    [tokenHash]
  );
}

export async function revokeAllSessionsForUser(
  client: DbClient,
  userId: string
): Promise<void> {
  await client.query(
    `/* revokeAllSessionsForUser */ DELETE FROM sessions WHERE user_id = $1`,
    [userId]
  );
}

// --- devices ---
//
// Presence rule (user decision 2026-11-28): the dashboard Devices list is
// a presence record, not a revocation record. Whenever the desktop runs it
// heartbeats (login + session-status check) and re-registers the device —
// and that re-registration clears a previous revoke, so the device shows
// again with a fresh last-seen and counts toward the package limit. The
// web Revoke button therefore only removes the device until it next runs;
// the strong cut-off for a device is revoking its pairing in the desktop
// (the broker then refuses to mint for it).

export async function upsertDevice(
  client: DbClient,
  args: {
    userId: string;
    deviceKey: string;
    machineName: string | null;
    platform: string | null;
    appVersion: string | null;
    lastIp: string | null;
  }
): Promise<DeviceRow> {
  const rows = await client.query<DeviceRow>(
    `/* upsertDevice */
     INSERT INTO devices (user_id, device_key, machine_name, platform, app_version, last_ip, last_seen_at)
     VALUES ($1, $2, $3, $4, $5, $6, now())
     ON CONFLICT (user_id, device_key) DO UPDATE SET
       machine_name = EXCLUDED.machine_name,
       platform = EXCLUDED.platform,
       app_version = EXCLUDED.app_version,
       last_ip = EXCLUDED.last_ip,
       last_seen_at = now(),
       revoked_at = NULL
     RETURNING *`,
    [
      args.userId,
      args.deviceKey,
      args.machineName,
      args.platform,
      args.appVersion,
      args.lastIp,
    ]
  );
  const row = rows[0];
  if (!row) throw new Error("upsertDevice returned no row");
  return row;
}

export async function listDevicesForUser(
  client: DbClient,
  userId: string
): Promise<DeviceRow[]> {
  return client.query<DeviceRow>(
    `/* listDevicesForUser */
     SELECT * FROM devices WHERE user_id = $1 ORDER BY last_seen_at DESC`,
    [userId]
  );
}

export async function findDeviceById(
  client: DbClient,
  id: string
): Promise<DeviceRow | null> {
  const rows = await client.query<DeviceRow>(
    `/* findDeviceById */ SELECT * FROM devices WHERE id = $1`,
    [id]
  );
  return rows[0] ?? null;
}

export async function revokeDevice(
  client: DbClient,
  id: string
): Promise<void> {
  await client.query(
    `/* revokeDevice */ UPDATE devices SET revoked_at = now() WHERE id = $1`,
    [id]
  );
}

// --- linked GitHub accounts + installations (limits) ---

export async function countLinkedAccountsForUser(
  client: DbClient,
  userId: string
): Promise<number> {
  const rows = await client.query<{ count: string }>(
    `/* countLinkedAccountsForUser */
     SELECT count(*)::text AS count FROM linked_github_accounts WHERE user_id = $1`,
    [userId]
  );
  return Number(rows[0]?.count ?? 0);
}

export async function countDevicesForUser(
  client: DbClient,
  userId: string
): Promise<number> {
  const rows = await client.query<{ count: string }>(
    `/* countDevicesForUser */
     SELECT count(*)::text AS count FROM devices WHERE user_id = $1 AND revoked_at IS NULL`,
    [userId]
  );
  return Number(rows[0]?.count ?? 0);
}

export async function listInstallationsForUser(
  client: DbClient,
  userId: string
): Promise<InstallationRow[]> {
  return client.query<InstallationRow>(
    `/* listInstallationsForUser */
     SELECT * FROM installations WHERE user_id = $1 ORDER BY created_at DESC`,
    [userId]
  );
}

export async function listLinkedAccountsForUser(
  client: DbClient,
  userId: string
): Promise<LinkedGithubAccountRow[]> {
  return client.query<LinkedGithubAccountRow>(
    `/* listLinkedAccountsForUser */
     SELECT * FROM linked_github_accounts WHERE user_id = $1 ORDER BY created_at DESC`,
    [userId]
  );
}

export async function findInstallationByGithubId(
  client: DbClient,
  githubInstallationId: number
): Promise<InstallationRow | null> {
  const rows = await client.query<InstallationRow>(
    `/* findInstallationByGithubId */
     SELECT * FROM installations WHERE github_installation_id = $1`,
    [githubInstallationId]
  );
  return rows[0] ?? null;
}

export async function findLinkedAccountById(
  client: DbClient,
  userId: string,
  id: string
): Promise<LinkedGithubAccountRow | null> {
  const rows = await client.query<LinkedGithubAccountRow>(
    `/* findLinkedAccountById */
     SELECT * FROM linked_github_accounts WHERE id = $1 AND user_id = $2`,
    [id, userId]
  );
  return rows[0] ?? null;
}

export async function findLinkedAccountByProviderAccount(
  client: DbClient,
  userId: string,
  providerAccountId: string
): Promise<LinkedGithubAccountRow | null> {
  const rows = await client.query<LinkedGithubAccountRow>(
    `/* findLinkedAccountByProviderAccount */
     SELECT * FROM linked_github_accounts WHERE user_id = $1 AND provider_account_id = $2`,
    [userId, providerAccountId]
  );
  return rows[0] ?? null;
}

/**
 * Link a GitHub App installation to a user: upserts the
 * `installations` row (by GitHub installation id) and the
 * `linked_github_accounts` row (`auth_kind='app'`). The provider
 * account id is the GitHub *account* id (stable across reinstalls —
 * a reinstall mints a new installation id for the same account, so
 * keying the linked row on the installation id would double-count).
 * Idempotent: re-linking returns the existing rows.
 */
export async function linkInstallation(
  client: DbClient,
  args: {
    userId: string;
    githubInstallationId: number;
    accountId: string;
    login: string;
  }
): Promise<{ installation: InstallationRow; account: LinkedGithubAccountRow }> {
  const installations = await client.query<InstallationRow>(
    `/* linkInstallation */
     INSERT INTO installations (user_id, github_installation_id, account_login)
     VALUES ($1, $2, $3)
     ON CONFLICT (github_installation_id) DO UPDATE
       SET user_id = EXCLUDED.user_id, account_login = EXCLUDED.account_login
     RETURNING *`,
    [args.userId, args.githubInstallationId, args.login]
  );
  const installation = installations[0];
  if (!installation) throw new Error("linkInstallation returned no row");
  const accounts = await client.query<LinkedGithubAccountRow>(
    `/* linkInstallationAccount */
     INSERT INTO linked_github_accounts (user_id, provider, provider_account_id, login, auth_kind)
     VALUES ($1, 'github', $2, $3, 'app')
     ON CONFLICT (user_id, provider, provider_account_id) DO UPDATE
       SET login = EXCLUDED.login
     RETURNING *`,
    [args.userId, args.accountId, args.login]
  );
  const account = accounts[0];
  if (!account) throw new Error("linkInstallationAccount returned no row");
  return { installation, account };
}

/**
 * Link a PAT GitHub account (login only — the PAT itself is never
 * sent to the web). The provider account id is the login: the
 * desktop never learns the GitHub account id for a PAT account, and
 * a login that is all digits (which would collide with an App
 * account id) is vanishingly unlikely.
 */
export async function linkPatAccount(
  client: DbClient,
  args: { userId: string; login: string }
): Promise<LinkedGithubAccountRow> {
  const rows = await client.query<LinkedGithubAccountRow>(
    `/* linkPatAccount */
     INSERT INTO linked_github_accounts (user_id, provider, provider_account_id, login, auth_kind)
     VALUES ($1, 'github', $2, $2, 'pat')
     ON CONFLICT (user_id, provider, provider_account_id) DO UPDATE
       SET login = EXCLUDED.login
     RETURNING *`,
    [args.userId, args.login]
  );
  const row = rows[0];
  if (!row) throw new Error("linkPatAccount returned no row");
  return row;
}

/**
 * Unlink a GitHub account: deletes the linked row, and for `app`
 * rows the `installations` row with the same login (one installation
 * per GitHub account, so the login identifies it). The `auth_kind`
 * guard matters: a user can hold the same login via PAT *and* via
 * the App, and unlinking the PAT row must not drop the App link.
 * Returns the deleted row, or null when it did not exist.
 */
export async function unlinkLinkedAccount(
  client: DbClient,
  args: { userId: string; id: string }
): Promise<LinkedGithubAccountRow | null> {
  const row = await findLinkedAccountById(client, args.userId, args.id);
  if (!row) return null;
  if (row.auth_kind === "app") {
    await client.query(
      `/* unlinkLinkedAccountInstallations */
       DELETE FROM installations WHERE user_id = $1 AND account_login = $2`,
      [args.userId, row.login]
    );
  }
  await client.query(
    `/* unlinkLinkedAccount */
     DELETE FROM linked_github_accounts WHERE id = $1 AND user_id = $2`,
    [args.id, args.userId]
  );
  return row;
}

export async function findDeviceByKey(
  client: DbClient,
  userId: string,
  deviceKey: string
): Promise<DeviceRow | null> {
  const rows = await client.query<DeviceRow>(
    `/* findDeviceByKey */
     SELECT * FROM devices WHERE user_id = $1 AND device_key = $2`,
    [userId, deviceKey]
  );
  return rows[0] ?? null;
}

/**
 * 1-based rank of a linked account among the user's linked accounts,
 * oldest first (created_at, then id as the tiebreaker). The mint
 * endpoint allows minting only while the rank is within the package
 * limit — so when a paid package expires back to Free, the oldest
 * accounts keep working and only the excess ones pause.
 */
export async function linkedAccountRank(
  client: DbClient,
  userId: string,
  linkedAccountId: string
): Promise<number> {
  const target = await findLinkedAccountById(client, userId, linkedAccountId);
  if (!target) return 0;
  const rows = await client.query<{ rank: string }>(
    `/* linkedAccountRank */
     SELECT count(*)::text AS rank FROM linked_github_accounts
     WHERE user_id = $1
       AND (created_at < $2 OR (created_at = $2 AND id <= $3))`,
    [userId, target.created_at, target.id]
  );
  return Number(rows[0]?.rank ?? 0);
}

// --- packages + subscriptions ---

export async function listPackages(client: DbClient): Promise<PackageRow[]> {
  return client.query<PackageRow>(
    `/* listPackages */ SELECT * FROM packages WHERE is_active = true ORDER BY max_linked_accounts ASC`
  );
}

export async function findPackageByName(
  client: DbClient,
  name: string
): Promise<PackageRow | null> {
  const rows = await client.query<PackageRow>(
    `/* findPackageByName */ SELECT * FROM packages WHERE name = $1`,
    [name]
  );
  return rows[0] ?? null;
}

export async function findActiveSubscriptionForUser(
  client: DbClient,
  userId: string
): Promise<(SubscriptionRow & { package: PackageRow }) | null> {
  const rows = await client.query<SubscriptionRow & { package: PackageRow }>(
    `/* findActiveSubscriptionForUser */
     SELECT s.*, to_jsonb(p.*) AS package FROM subscriptions s
     JOIN packages p ON p.id = s.package_id
     WHERE s.user_id = $1 AND s.status = 'active'
       AND (s.expires_at IS NULL OR s.expires_at > now())`,
    [userId]
  );
  return rows[0] ?? null;
}

// --- abuse limits (§7) ---

export async function recordRegistrationAttempt(
  client: DbClient,
  ipHash: string
): Promise<void> {
  await client.query(
    `/* recordRegistrationAttempt */
     INSERT INTO registration_attempts (ip_hash) VALUES ($1)`,
    [ipHash]
  );
}

export async function countRecentRegistrations(
  client: DbClient,
  args: { ipHash: string; since: Date }
): Promise<number> {
  const rows = await client.query<{ count: string }>(
    `/* countRecentRegistrations */
     SELECT count(*)::text AS count FROM registration_attempts
     WHERE ip_hash = $1 AND created_at > $2`,
    [args.ipHash, args.since.toISOString()]
  );
  return Number(rows[0]?.count ?? 0);
}

export async function findOldestRecentRegistration(
  client: DbClient,
  args: { ipHash: string; since: Date }
): Promise<{ created_at: string } | null> {
  const rows = await client.query<{ created_at: string }>(
    `/* findOldestRecentRegistration */
     SELECT created_at FROM registration_attempts
     WHERE ip_hash = $1 AND created_at > $2 ORDER BY created_at ASC LIMIT 1`,
    [args.ipHash, args.since.toISOString()]
  );
  return rows[0] ?? null;
}

export async function recordLogout(
  client: DbClient,
  args: { userId: string; deviceKey: string | null; ipHash: string | null }
): Promise<void> {
  await client.query(
    `/* recordLogout */
     INSERT INTO logout_events (user_id, device_key, ip_hash) VALUES ($1, $2, $3)`,
    [args.userId, args.deviceKey, args.ipHash]
  );
}

export async function findRecentLogoutForDevice(
  client: DbClient,
  args: { deviceKey: string; since: Date }
): Promise<{ user_id: string; created_at: string } | null> {
  const rows = await client.query<{ user_id: string; created_at: string }>(
    `/* findRecentLogoutForDevice */
     SELECT user_id, created_at FROM logout_events
     WHERE device_key = $1 AND created_at > $2 ORDER BY created_at DESC LIMIT 1`,
    [args.deviceKey, args.since.toISOString()]
  );
  return rows[0] ?? null;
}

export async function findRecentLogoutForIp(
  client: DbClient,
  args: { ipHash: string; since: Date }
): Promise<{ user_id: string; created_at: string } | null> {
  const rows = await client.query<{ user_id: string; created_at: string }>(
    `/* findRecentLogoutForIp */
     SELECT user_id, created_at FROM logout_events
     WHERE ip_hash = $1 AND created_at > $2 ORDER BY created_at DESC LIMIT 1`,
    [args.ipHash, args.since.toISOString()]
  );
  return rows[0] ?? null;
}

// --- audit log ---

export async function recordAudit(
  client: DbClient,
  args: {
    userId: string | null;
    action: string;
    details?: Record<string, unknown>;
    ipHash?: string | null;
  }
): Promise<AuditLogRow> {
  const rows = await client.query<AuditLogRow>(
    `/* recordAudit */
     INSERT INTO audit_log (user_id, action, details, ip_hash)
     VALUES ($1, $2, $3, $4) RETURNING *`,
    [
      args.userId,
      args.action,
      JSON.stringify(args.details ?? {}),
      args.ipHash ?? null,
    ]
  );
  const row = rows[0];
  if (!row) throw new Error("recordAudit returned no row");
  return row;
}

// --- TOTP enrollment (W5) ---

/** Store the secret generated at enrollment; it becomes live only on confirm. */
export async function setTotpPendingSecret(
  client: DbClient,
  userId: string,
  secret: string
): Promise<void> {
  await client.query(
    `/* setTotpPendingSecret */
     UPDATE users SET totp_pending_secret = $2, updated_at = now() WHERE id = $1`,
    [userId, secret]
  );
}

/**
 * Confirm enrollment: the pending secret becomes the live secret and
 * the pending slot is cleared. Callers verify the code first.
 */
export async function confirmTotpSecret(
  client: DbClient,
  userId: string
): Promise<UserRow | null> {
  const rows = await client.query<UserRow>(
    `/* confirmTotpSecret */
     UPDATE users
     SET totp_secret = totp_pending_secret, totp_pending_secret = NULL,
         updated_at = now()
     WHERE id = $1 AND totp_pending_secret IS NOT NULL
     RETURNING *`,
    [userId]
  );
  return rows[0] ?? null;
}

/** Disable 2FA: clears the live secret, the pending secret, and all backup codes. */
export async function clearTotp(
  client: DbClient,
  userId: string
): Promise<void> {
  await client.query(
    `/* clearTotp */ DELETE FROM totp_backup_codes WHERE user_id = $1`,
    [userId]
  );
  await client.query(
    `/* clearTotp */ UPDATE users
     SET totp_secret = NULL, totp_pending_secret = NULL, updated_at = now()
     WHERE id = $1`,
    [userId]
  );
}

/** Replace all backup codes for a user with a fresh set of hashes. */
export async function replaceBackupCodes(
  client: DbClient,
  userId: string,
  codeHashes: string[]
): Promise<void> {
  await client.query(
    `/* replaceBackupCodes */ DELETE FROM totp_backup_codes WHERE user_id = $1`,
    [userId]
  );
  for (const codeHash of codeHashes) {
    await client.query(
      `/* replaceBackupCodes */
       INSERT INTO totp_backup_codes (user_id, code_hash) VALUES ($1, $2)`,
      [userId, codeHash]
    );
  }
}

/** Count the unspent backup codes a user has left. */
export async function countBackupCodes(
  client: DbClient,
  userId: string
): Promise<number> {
  const rows = await client.query<{ count: string }>(
    `/* countBackupCodes */
     SELECT COUNT(*)::text AS count FROM totp_backup_codes
     WHERE user_id = $1 AND consumed_at IS NULL`,
    [userId]
  );
  return Number(rows[0]?.count ?? 0);
}

/**
 * Spend a backup code: finds the unconsumed code by its hash and marks
 * it consumed. Returns true only when a code was actually spent.
 */
export async function consumeBackupCode(
  client: DbClient,
  userId: string,
  codeHash: string
): Promise<boolean> {
  const rows = await client.query<TotpBackupCodeRow>(
    `/* consumeBackupCode */
     UPDATE totp_backup_codes SET consumed_at = now()
     WHERE user_id = $1 AND code_hash = $2 AND consumed_at IS NULL
     RETURNING *`,
    [userId, codeHash]
  );
  return rows.length > 0;
}

// --- admin (W5) ---

/**
 * Every user for the admin dashboard, with their package and usage
 * counts. Optional case-insensitive search over email and name.
 * Newest first; the admin UI pages through it with limit/offset.
 */
export async function listUsersAdmin(
  client: DbClient,
  args: { q?: string; limit: number; offset: number }
): Promise<AdminUserRow[]> {
  const pattern = args.q ? `%${args.q}%` : null;
  return client.query<AdminUserRow>(
    `/* listUsersAdmin */
     SELECT u.*,
       p.name AS package_name,
       s.expires_at AS subscription_expires_at,
       (SELECT COUNT(*) FROM linked_github_accounts l WHERE l.user_id = u.id) AS linked_count,
       (SELECT COUNT(*) FROM devices d WHERE d.user_id = u.id AND d.revoked_at IS NULL) AS device_count
     FROM users u
     LEFT JOIN subscriptions s
       ON s.user_id = u.id AND s.status = 'active'
       AND (s.expires_at IS NULL OR s.expires_at > now())
     LEFT JOIN packages p ON p.id = s.package_id
     WHERE ($1::text IS NULL OR u.email ILIKE $1 OR u.name ILIKE $1)
     ORDER BY u.created_at DESC
     LIMIT $2 OFFSET $3`,
    [pattern, args.limit, args.offset]
  );
}

export async function countUsersAdmin(
  client: DbClient,
  q?: string
): Promise<number> {
  const pattern = q ? `%${q}%` : null;
  const rows = await client.query<{ count: string }>(
    `/* countUsersAdmin */
     SELECT COUNT(*)::text AS count FROM users u
     WHERE ($1::text IS NULL OR u.email ILIKE $1 OR u.name ILIKE $1)`,
    [pattern]
  );
  return Number(rows[0]?.count ?? 0);
}

/** Totals for the admin home: users, linked accounts, active devices, 2FA users. */
export async function adminStats(client: DbClient): Promise<{
  users: number;
  linkedAccounts: number;
  activeDevices: number;
  twoFactorUsers: number;
}> {
  const rows = await client.query<{
    users: string;
    linked_accounts: string;
    active_devices: string;
    two_factor_users: string;
  }>(
    `/* adminStats */
     SELECT
       (SELECT COUNT(*) FROM users) AS users,
       (SELECT COUNT(*) FROM linked_github_accounts) AS linked_accounts,
       (SELECT COUNT(*) FROM devices WHERE revoked_at IS NULL) AS active_devices,
       (SELECT COUNT(*) FROM users WHERE totp_secret IS NOT NULL) AS two_factor_users`
  );
  const row = rows[0];
  return {
    users: Number(row?.users ?? 0),
    linkedAccounts: Number(row?.linked_accounts ?? 0),
    activeDevices: Number(row?.active_devices ?? 0),
    twoFactorUsers: Number(row?.two_factor_users ?? 0),
  };
}

/** All packages including inactive ones — the admin manages the whole set. */
export async function listAllPackages(client: DbClient): Promise<PackageRow[]> {
  return client.query<PackageRow>(
    `/* listAllPackages */ SELECT * FROM packages ORDER BY max_linked_accounts ASC`
  );
}

export async function findPackageById(
  client: DbClient,
  id: string
): Promise<PackageRow | null> {
  const rows = await client.query<PackageRow>(
    `/* findPackageById */ SELECT * FROM packages WHERE id = $1`,
    [id]
  );
  return rows[0] ?? null;
}

export async function createPackage(
  client: DbClient,
  args: {
    name: string;
    maxLinkedAccounts: number;
    maxDevices: number;
    priceCents: number;
  }
): Promise<PackageRow> {
  const rows = await client.query<PackageRow>(
    `/* createPackage */
     INSERT INTO packages (name, max_linked_accounts, max_devices, price_cents)
     VALUES ($1, $2, $3, $4) RETURNING *`,
    [args.name, args.maxLinkedAccounts, args.maxDevices, args.priceCents]
  );
  const row = rows[0];
  if (!row) throw new Error("createPackage returned no row");
  return row;
}

/**
 * Edit a package. Only the fields given are changed; `isActive: false`
 * retires a package instead of deleting it (subscriptions reference it).
 */
export async function updatePackage(
  client: DbClient,
  id: string,
  args: {
    name?: string;
    maxLinkedAccounts?: number;
    maxDevices?: number;
    priceCents?: number;
    isActive?: boolean;
  }
): Promise<PackageRow | null> {
  const sets: string[] = [];
  const params: unknown[] = [id];
  const push = (sql: string, value: unknown) => {
    params.push(value);
    sets.push(`${sql} = $${params.length}`);
  };
  if (args.name !== undefined) push("name", args.name);
  if (args.maxLinkedAccounts !== undefined)
    push("max_linked_accounts", args.maxLinkedAccounts);
  if (args.maxDevices !== undefined) push("max_devices", args.maxDevices);
  if (args.priceCents !== undefined) push("price_cents", args.priceCents);
  if (args.isActive !== undefined) push("is_active", args.isActive);
  if (sets.length === 0) return findPackageById(client, id);
  sets.push("updated_at = now()");
  const rows = await client.query<PackageRow>(
    `/* updatePackage */ UPDATE packages SET ${sets.join(", ")} WHERE id = $1 RETURNING *`,
    params
  );
  return rows[0] ?? null;
}

/**
 * Assign a package to a user: upserts their (single) subscription as
 * active. `expiresAt` null means no expiry. Re-assigning the same
 * package refreshes the subscription rather than duplicating it.
 */
export async function setUserPackage(
  client: DbClient,
  userId: string,
  packageId: string,
  expiresAt: string | null
): Promise<SubscriptionRow> {
  const rows = await client.query<SubscriptionRow>(
    `/* setUserPackage */
     INSERT INTO subscriptions (user_id, package_id, status, expires_at)
     VALUES ($1, $2, 'active', $3)
     ON CONFLICT (user_id) DO UPDATE
       SET package_id = EXCLUDED.package_id, status = 'active',
           expires_at = EXCLUDED.expires_at, updated_at = now()
     RETURNING *`,
    [userId, packageId, expiresAt]
  );
  const row = rows[0];
  if (!row) throw new Error("setUserPackage returned no row");
  return row;
}
