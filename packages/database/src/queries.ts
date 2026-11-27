import type { DbClient } from "./client";
import type {
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
       last_seen_at = now()
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
