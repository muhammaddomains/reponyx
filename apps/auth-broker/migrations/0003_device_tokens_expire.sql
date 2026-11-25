-- RepoNyx Auth Broker V3 — device tokens expire (audit A1).
--
-- Before this, a device token minted by pairing never expired: a
-- leaked token could mint installation tokens forever, and every
-- re-pair left another live token behind. Tokens now live 90 days
-- from creation (`device_token_ttl`); minting with an expired token
-- fails and the device must pair again.
--
-- Tokens minted before this migration get the same 90-day lifetime
-- counted from their creation time, so the migration itself extends
-- no token.
ALTER TABLE broker_device_tokens ADD COLUMN expires_at TEXT;
UPDATE broker_device_tokens
SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at, '+90 days')
WHERE expires_at IS NULL;
