-- RepoNyx Auth Broker V2 — pairing codes no longer expire.
--
-- Phase 10 fix: the 10-minute pairing-code TTL expired while users
-- were mid-flow. Codes stay safe without it: single-use, bound to
-- one device, confirm requires the setup token, and wrong-device
-- attempts revoke the code after `max_pairing_attempts`. New codes
-- store NULL in `expires_at`; the column becomes nullable.
ALTER TABLE broker_pairing_codes ALTER COLUMN expires_at DROP NOT NULL;
