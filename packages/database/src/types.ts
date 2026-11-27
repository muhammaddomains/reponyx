/** Row shapes for the web database (see migrations/0001_init.sql). */

export interface UserRow {
  id: string;
  email: string;
  password_hash: string;
  name: string | null;
  has_password: boolean;
  email_verified: boolean;
  is_admin: boolean;
  totp_secret: string | null;
  locale: string;
  created_at: string;
  updated_at: string;
}

/** One OAuth identity linked to a RepoNyx account (migration 0004). */
export interface LinkedProviderRow {
  id: string;
  user_id: string;
  provider: string;
  provider_user_id: string;
  email: string;
  created_at: string;
}

export interface EmailVerificationTokenRow {
  id: string;
  user_id: string;
  token_hash: string;
  expires_at: string;
  consumed_at: string | null;
  created_at: string;
}

export interface PasswordResetTokenRow {
  id: string;
  user_id: string;
  token_hash: string;
  expires_at: string;
  consumed_at: string | null;
  created_at: string;
}

export interface SessionRow {
  id: string;
  user_id: string;
  token_hash: string;
  device_id: string | null;
  expires_at: string;
  created_at: string;
  last_seen_at: string;
}

export interface DeviceRow {
  id: string;
  user_id: string;
  device_key: string;
  machine_name: string | null;
  platform: string | null;
  app_version: string | null;
  last_ip: string | null;
  last_seen_at: string;
  revoked_at: string | null;
  created_at: string;
}

export interface LinkedGithubAccountRow {
  id: string;
  user_id: string;
  provider: string;
  provider_account_id: string;
  login: string;
  auth_kind: "app" | "pat";
  created_at: string;
}

export interface InstallationRow {
  id: string;
  user_id: string;
  github_installation_id: number;
  account_login: string;
  created_at: string;
}

export interface PackageRow {
  id: string;
  name: string;
  max_linked_accounts: number;
  max_devices: number;
  price_cents: number;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface SubscriptionRow {
  id: string;
  user_id: string;
  package_id: string;
  status: "active" | "expired" | "cancelled";
  expires_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface AuditLogRow {
  id: string;
  user_id: string | null;
  action: string;
  details: Record<string, unknown> | null;
  ip_hash: string | null;
  created_at: string;
}
