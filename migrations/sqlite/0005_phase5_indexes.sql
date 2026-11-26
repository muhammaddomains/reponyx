-- RepoNyx V5 — Phase 5 polish indexes + case-insensitive names
-- (audit DB-m1, DB-m3).
--
-- Indexes the audit found missing: schedules by account, history by
-- job, audit logs by actor / resource (the list queries filter on
-- those columns and order by created_at).
--
-- `local_path` gets a partial UNIQUE index: one archive belongs to
-- one repository. The layout already guarantees this (the path is
-- derived from account/owner/name), so the index only turns a
-- would-be silent corruption into a loud constraint failure.
--
-- GitHub names are case-insensitive, but the V2 unique index and
-- the rename clash check compared bytes (DB-m3): "Foo/Bar" and
-- "foo/bar" could coexist as two rows for the same repository.
-- The index is rebuilt with COLLATE NOCASE (ASCII folding is
-- enough — GitHub names are ASCII). GitHub always returns the
-- canonical casing for a repository, so no live database is
-- expected to hold case-variant duplicates; if one ever does, this
-- migration fails loudly (and the pre-migrate backup from Phase 2
-- is the recovery path) instead of silently keeping the duplicate.
CREATE INDEX IF NOT EXISTS idx_schedules_account_id ON schedules (account_id);

CREATE INDEX IF NOT EXISTS idx_sync_history_job_id
    ON sync_history (job_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_logs_actor_created
    ON audit_logs (actor, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_logs_resource_created
    ON audit_logs (resource_type, resource_id, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_repositories_local_path
    ON repositories (local_path) WHERE local_path IS NOT NULL;

DROP INDEX IF EXISTS idx_repositories_account_full_name;
CREATE UNIQUE INDEX IF NOT EXISTS idx_repositories_account_full_name
    ON repositories (account_id, full_name COLLATE NOCASE);
