-- Phase 6 queue fields (2026-11-21): retry scheduling, worker leases,
-- cancellation, and DB-level dedup. All columns are nullable, so
-- existing rows are unaffected; V1-V3 are untouched.
--
-- available_at: a retry (or deferred) job is not claimable before this
--   UTC timestamp. NULL means immediately claimable.
-- worker_id / lease_expires_at / heartbeat_at: lease ownership for a
--   running job. Recovery treats a job as abandoned only when its
--   lease has expired (see reponyx-sync-queue recovery).
-- cancel_requested_at: cooperative cancellation requested while the
--   job was running; the worker checks it via its heartbeat loop.
-- dedupe_key: deterministic `repo:<id>:<job_type>`; the partial unique
--   index below enforces at most one active job per (repository, type).
ALTER TABLE sync_jobs ADD COLUMN available_at TEXT;
ALTER TABLE sync_jobs ADD COLUMN worker_id TEXT;
ALTER TABLE sync_jobs ADD COLUMN lease_expires_at TEXT;
ALTER TABLE sync_jobs ADD COLUMN heartbeat_at TEXT;
ALTER TABLE sync_jobs ADD COLUMN cancel_requested_at TEXT;
ALTER TABLE sync_jobs ADD COLUMN dedupe_key TEXT;

CREATE UNIQUE INDEX idx_sync_jobs_dedupe_active
    ON sync_jobs(dedupe_key)
    WHERE dedupe_key IS NOT NULL AND status IN ('pending', 'running');

CREATE INDEX idx_sync_jobs_available_at ON sync_jobs(available_at);

CREATE INDEX idx_sync_jobs_lease_running
    ON sync_jobs(lease_expires_at) WHERE status = 'running';
