-- RepoNyx Auth Broker V1 — durable broker state (Phase 10, F1).
--
-- Before this, installations, pairing codes, setup `state` values,
-- device tokens, and the audit log lived only in memory: a broker
-- restart lost every pairing and erased the audit history. These
-- tables are the write-through backing store for BrokerState; the
-- in-memory maps remain the read cache.
--
-- Security: only SHA-256 hashes of pairing codes, `state` values,
-- and device tokens are stored — never the raw values — so a dump
-- of this database yields nothing an attacker can present. The
-- GitHub App private key is NEVER stored here (it lives only in
-- the broker's environment). The minted installation-token cache
-- is deliberately NOT persisted: tokens are short-lived and are
-- re-minted on demand after a restart.

CREATE TABLE IF NOT EXISTS broker_installations (
    installation_id TEXT PRIMARY KEY,
    setup_action TEXT NOT NULL,
    account_login TEXT,
    installed_at TEXT NOT NULL,
    paired_device TEXT
);

CREATE TABLE IF NOT EXISTS broker_pairing_codes (
    code_hash TEXT PRIMARY KEY,
    installation_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    device_identifier TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_broker_pairing_codes_installation
    ON broker_pairing_codes (installation_id);

CREATE TABLE IF NOT EXISTS broker_setup_states (
    state_hash TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS broker_device_tokens (
    token_hash TEXT PRIMARY KEY,
    device_identifier TEXT NOT NULL,
    device_name TEXT NOT NULL,
    installation_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_broker_device_tokens_device
    ON broker_device_tokens (device_identifier);

CREATE TABLE IF NOT EXISTS broker_audit_events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL,
    detail TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_broker_audit_events_timestamp
    ON broker_audit_events (timestamp);
