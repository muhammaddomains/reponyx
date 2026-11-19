-- RepoNyx V3 — PAT expiry for accounts (Phase 5).
--
-- A fine-grained PAT may carry a user-supplied expiry date; the
-- desktop warns before it lapses. GitHub App accounts never use
-- this column (installation tokens are short-lived and minted on
-- demand, never stored). The value is metadata, not a secret.
-- V1 and V2 are untouched.
ALTER TABLE accounts ADD COLUMN pat_expires_at TEXT;
