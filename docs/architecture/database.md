# Database Design

Status: Accepted (Phase 1) · Migration: `migrations/sqlite/0001_init.sql`

## Engine

SQLite via SQLx. WAL mode, foreign keys enforced on every connection,
UTC timestamps (ISO-8601 TEXT), versioned migrations embedded at
compile time and applied on open. **No authentication secrets in any
table** — credentials live in the OS credential store.

## Tables (V1)

| Table | Purpose | Key columns |
| --- | --- | --- |
| `accounts` | Connected GitHub accounts | provider, provider_account_id (unique per provider), username, authentication_method, connection_status, last_validated_at |
| `installations` | GitHub App installations per account | installation_id, installation_type, installation_status, permissions_json |
| `repositories` | Discovered repos | provider_repository_id (unique per account), owner, name, full_name, visibility, default_branch, clone_url (credential-free), archive_status, sync_enabled, sync_mode, local_path, remote_updated_at, last_synced_at |
| `schedules` | Global / account / repository schedules | scope, interval_type, interval_value, timezone, enabled, next_run_at; CHECK enforces scope↔FK consistency |
| `sync_jobs` | Queue + execution records | job_type, status, priority, attempt_count, error_code/message |
| `sync_history` | Per-operation results | operation, result, duration_ms, bytes_received |
| `settings` | Key/value settings | key (unique), value |
| `devices` | Paired devices | device_identifier (unique), pairing_status |
| `audit_logs` | Security audit events | actor, action, resource_type, resource_id, metadata |

## Indexes

On actual query paths: repositories by account/status/full_name;
schedules by (enabled, next_run_at) and repository; sync_jobs by
(status, created_at) and repository; sync_history by repository;
audit_logs by created_at.

## Identity rules

- Repositories are identified by provider repository id + account,
  never by display name alone. Renames/ownership changes are handled
  by updating name/owner while the stable id (and local path mapping)
  is preserved; collisions never silently overwrite local data.
- All writes that span tables use transactions.

## Backups

`data/database-backups/` holds SQLite backups (Phase 9 tooling).
Restore must re-run migrations to the current version.
