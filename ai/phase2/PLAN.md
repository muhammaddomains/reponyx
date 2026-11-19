# Phase 2 — Core Database and Domain

- Status: Not started
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

- [ ] Account repository (CRUD, validation, status transitions)
- [ ] Repository repository (upsert by provider id, filters, counts)
- [ ] Schedule repository (scope precedence: repo > account > global)
- [ ] Job + history repositories
- [ ] Settings + audit-log repositories
- [ ] Migration V2 if schema gaps are found (never edit V1)

## Dependencies

Phase 1 (schema, pool, counts).

## Acceptance criteria

- All repositories covered by tests against temp SQLite (migrations applied).
- No raw SQL in the desktop or UI layers.
- FK violations rejected; transactions roll back on failure.

## Testing

- Unit + integration tests per repository; concurrent-write sanity test.

## Known issues

None yet.

## Completion status

Not started.
