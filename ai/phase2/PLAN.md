# Phase 2 — Core Database and Domain

- Status: Complete (2026-11-18)
- Depends on: Phase 1

## Objectives

Complete the domain layer and database repositories on top of the V1
schema: account registry, repository registry, schedule registry, job
registry, sync history, settings, repository identity management.

## Requirements

- Repository-pattern data access in `crates/database` (accounts,
  repositories, schedules, sync_jobs, sync_history, settings, devices,
  audit_logs) with transactions.
- Identity: provider repository id + account as stable key; rename /
  ownership-change handling; collision detection.
- Domain services in `crates/core` as needed; no business logic in the UI.

## Implementation tasks

- [x] Account repository (CRUD, validation, status transitions)
- [x] Repository repository (upsert by provider id, filters, counts)
- [x] Schedule repository (scope precedence: repo > account > global)
- [x] Job + history repositories
- [x] Settings + audit-log repositories
- [x] Migration V2 if schema gaps are found (never edit V1)

## Dependencies

Phase 1 (schema, pool, counts).

## Acceptance criteria

- All repositories covered by tests against temp SQLite (migrations applied).
- No raw SQL in the desktop or UI layers.
- FK violations rejected; transactions roll back on failure.

## Testing

- Unit + integration tests per repository; concurrent-write sanity test.
- Verified 2026-11-18: `cargo test` **91 passed, 0 failed** (core 11,
  database 54, git-engine 22, logging 4, provider-abstractions 0);
  `cargo fmt --check` pass; clippy (5 buildable crates) pass,
  0 warnings; `pnpm -r typecheck` pass; desktop build pass.
  `cargo test --workspace` / `cargo clippy --workspace` still blocked
  on this VM by the missing `gdk-3.0` system library (desktop crate
  only) — not claimed passed.

## Known issues

Two real bugs found and fixed while building (both caught by the new
tests, not by review):

1. `transition_job` / `complete_job_with_history` were missing the
   final `.bind(id)` — sqlx does not check bind counts for `query()`,
   so the `WHERE id = ?` matched NULL and the UPDATE silently
   affected 0 rows. Fixed; the functions now assert `rows_affected`
   in debug builds.
2. `claim_next_job` used select-then-update inside a transaction,
   which deadlocks under contention (both claimers hold a shared read
   lock; neither can upgrade to write → `database is locked`).
   Rewritten as a single atomic `UPDATE ... RETURNING` statement.

## Completion status

Complete (2026-11-18). Migration V2 was needed: `0002_unique_full_name.sql`
adds a UNIQUE index on `repositories (account_id, full_name)` — the
repository layer does collision detection in code, but SQLite
transactions are deferred by default, so concurrent upserts/renames
could race past the check; the index makes the invariant hold at the
storage layer and the repository maps the constraint error to
`InvalidInput`. V1 was not edited. One dependency added:
`serde_json` to `reponyx-database` (already in the lockfile at
1.0.154, latest stable; used for audit-log metadata JSON).
