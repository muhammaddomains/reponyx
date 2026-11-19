# ADR-005: Local SQLite Database

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

RepoNyx needs durable, queryable state for accounts, repositories,
schedules, jobs, and history on a single-user desktop, without
requiring the user to run a database server.

## Decision

**SQLite via SQLx**, single file (`data/reponyx.sqlite`), WAL mode,
foreign keys on, versioned migrations in `migrations/sqlite/`,
embedded and applied on open.

## Consequences

- Zero external services; backup is a file copy; transactions and
  indexes give the needed integrity and query performance for
  1,000–10,000 repositories.
- Single-writer semantics: the desktop and background agent must
  coordinate (WAL + one writer at a time; Phase 7 defines ownership).
- No secrets in any table — credentials live in the OS store.
- Compile-time-checked queries are avoided where they would require a
  live DB at build time; runtime queries are used instead.
