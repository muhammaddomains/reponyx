# ADR-006: Background Agent

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

Scheduled sync must survive UI closure and machine restart. The
desktop UI process cannot be relied on to stay alive.

## Decision

A **separate background agent** binary (Phase 7):

- First implementation: user-session process started at login via
  Windows Task Scheduler, running minimized with tray integration.
- Owns the scheduler + queue; the desktop is a client over IPC.
- Process lifecycle is abstracted so a native Windows Service can
  replace the Task Scheduler launcher later.

## Consequences

- Sync continues after the UI closes; survives UI crashes.
- A user-session process does **not** survive logout; that limitation
  is documented, not hidden. A true service requires resolving
  credential-store scoping (Credential Manager is per-user) first.
- Two processes share one SQLite file: ownership and locking rules
  are a Phase 7 deliverable.
