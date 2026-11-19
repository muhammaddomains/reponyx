-- RepoNyx V1 — initial schema (spec §14).
-- All timestamps UTC ISO-8601 TEXT. No secrets in any table.
-- SQLite: foreign keys enforced by the connection (PRAGMA foreign_keys=ON).

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,                       -- 'github' (future: gitlab, bitbucket)
    account_type TEXT NOT NULL DEFAULT 'personal',-- 'personal' | 'organization'
    provider_account_id TEXT NOT NULL,
    username TEXT NOT NULL,
    display_name TEXT,
    label TEXT,
    authentication_method TEXT NOT NULL,          -- 'github_app' | 'fine_grained_pat'
    connection_status TEXT NOT NULL DEFAULT 'connected',
    last_validated_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (provider, provider_account_id)
);

CREATE TABLE IF NOT EXISTS installations (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    installation_id TEXT NOT NULL,                -- GitHub installation id
    installation_type TEXT NOT NULL DEFAULT 'user', -- 'user' | 'organization'
    installation_status TEXT NOT NULL DEFAULT 'active',
    permissions_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (account_id, installation_id)
);

CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    provider_repository_id TEXT NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',   -- 'public' | 'private' | 'internal'
    default_branch TEXT,
    clone_url TEXT NOT NULL,                      -- credential-free HTTPS URL
    archive_status TEXT NOT NULL DEFAULT 'pending',
    sync_enabled INTEGER NOT NULL DEFAULT 1,
    sync_mode TEXT NOT NULL DEFAULT 'mirror',     -- 'mirror' | 'working' | 'both'
    local_path TEXT,
    remote_updated_at TEXT,
    last_synced_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (account_id, provider_repository_id)
);

CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL,                          -- 'global' | 'account' | 'repository'
    account_id TEXT REFERENCES accounts (id) ON DELETE CASCADE,
    repository_id TEXT REFERENCES repositories (id) ON DELETE CASCADE,
    interval_type TEXT NOT NULL,                  -- 'interval_hours' | 'daily' | 'weekly' | 'custom'
    interval_value TEXT,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    enabled INTEGER NOT NULL DEFAULT 1,
    next_run_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (scope = 'global' AND account_id IS NULL AND repository_id IS NULL)
        OR (scope = 'account' AND account_id IS NOT NULL AND repository_id IS NULL)
        OR (scope = 'repository' AND repository_id IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS sync_jobs (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,                       -- 'mirror_clone' | 'working_clone' | 'fetch' | ...
    status TEXT NOT NULL DEFAULT 'pending',       -- 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'
    priority INTEGER NOT NULL DEFAULT 0,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    started_at TEXT,
    completed_at TEXT,
    error_code TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_history (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    job_id TEXT REFERENCES sync_jobs (id) ON DELETE SET NULL,
    operation TEXT NOT NULL,
    result TEXT NOT NULL,                         -- 'success' | 'failed' | 'cancelled'
    duration_ms INTEGER,
    bytes_received INTEGER,
    started_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS settings (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    device_name TEXT NOT NULL,
    device_identifier TEXT NOT NULL UNIQUE,
    pairing_status TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'paired' | 'revoked'
    created_at TEXT NOT NULL,
    last_seen_at TEXT
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

-- Indexes on actual query paths (spec §14).
CREATE INDEX IF NOT EXISTS idx_repositories_account_id ON repositories (account_id);
CREATE INDEX IF NOT EXISTS idx_repositories_archive_status ON repositories (archive_status);
CREATE INDEX IF NOT EXISTS idx_repositories_full_name ON repositories (full_name);
CREATE INDEX IF NOT EXISTS idx_schedules_next_run ON schedules (enabled, next_run_at);
CREATE INDEX IF NOT EXISTS idx_schedules_repository ON schedules (repository_id);
CREATE INDEX IF NOT EXISTS idx_sync_jobs_status ON sync_jobs (status, created_at);
CREATE INDEX IF NOT EXISTS idx_sync_jobs_repository ON sync_jobs (repository_id, created_at);
CREATE INDEX IF NOT EXISTS idx_sync_history_repository ON sync_history (repository_id, started_at);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs (created_at);

-- Default global settings (no secrets).
INSERT OR IGNORE INTO settings (id, key, value, updated_at)
VALUES
    ('setting-storage-root', 'storage.root', '', datetime('now')),
    ('setting-sync-mode', 'sync.default_mode', 'mirror', datetime('now')),
    ('setting-concurrency', 'sync.concurrency', '5', datetime('now')),
    ('setting-theme', 'ui.theme', 'system', datetime('now'));
