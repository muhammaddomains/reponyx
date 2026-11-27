-- 0004_linked_providers — OAuth identities linked to one account.
-- One RepoNyx account per email; Google/GitHub are sign-in methods
-- linked to it, shown in the profile, linkable/unlinkable there.
-- Never edit an applied migration; schema changes go in a new file.

ALTER TABLE users ADD COLUMN IF NOT EXISTS has_password boolean NOT NULL DEFAULT true;

CREATE TABLE IF NOT EXISTS linked_providers (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  provider text NOT NULL CHECK (provider IN ('google', 'github')),
  provider_user_id text NOT NULL,
  email text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, provider),
  UNIQUE (provider, provider_user_id)
);
CREATE INDEX IF NOT EXISTS idx_linked_providers_user
  ON linked_providers (user_id);
