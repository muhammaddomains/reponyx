-- 0005_totp_backup_codes — TOTP enrollment (W5): pending secret + backup codes.
-- Never edit an applied migration; schema changes go in a new file.

-- The secret generated at enrollment, held server-side until the user
-- confirms it with a code from their authenticator app. Only then does
-- it move to users.totp_secret. Keeping it in the database (rather than
-- echoing it to the client and back) means the client never holds an
-- unconfirmed secret.
ALTER TABLE users ADD COLUMN IF NOT EXISTS totp_pending_secret text NULL;

-- One-time backup codes for 2FA login when the authenticator app is
-- unavailable. Only SHA-256 hashes are stored; the plaintext codes are
-- shown exactly once, at enrollment or regeneration. A code is spent by
-- setting consumed_at.
CREATE TABLE IF NOT EXISTS totp_backup_codes (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  code_hash text NOT NULL,
  consumed_at timestamptz NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, code_hash)
);
CREATE INDEX IF NOT EXISTS idx_totp_backup_codes_user
  ON totp_backup_codes (user_id);
