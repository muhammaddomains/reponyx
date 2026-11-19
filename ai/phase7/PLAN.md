# Phase 7 — Scheduler and Background Agent

- Status: Not started
- Depends on: Phase 6

## Objectives

Automatic synchronization independent of the UI: persistent scheduler
+ background agent + Windows startup integration + secure IPC.

## Requirements

- `crates/scheduler`: persistent schedules (SQLite); intervals
  1/2/4/6/8/12 h, daily, custom; specific time, weekdays, time windows;
  global / account / repository precedence; timezone-aware (UTC
  storage, local display, DST-safe).
- `apps/background-agent`: loads config/DB/credentials, runs scheduler
  + queue, recovers interrupted work, emits progress, graceful shutdown.
- Windows: Task Scheduler at login, minimized, tray integration,
  health monitoring, auto-restart. Service-account/credential-store
  limits documented before any native service work.
- IPC: named pipes (Windows), versioned + authenticated messages,
  ACLs, disconnect/restart detection.

## Implementation tasks

- [ ] Scheduler crate + next-run calculation (incl. DST)
- [ ] Background agent binary + lifecycle
- [ ] Task Scheduler registration + tray
- [ ] IPC crate (protocol v1) + desktop client
- [ ] Agent health + restart detection in UI

## Dependencies

Phase 6 queue; Phase 5 credentials.

## Acceptance criteria

- Schedules survive restart; due jobs run with the UI closed; agent
  restarts after termination; IPC rejects unauthenticated messages.

## Testing

- Scheduler math tests (incl. DST transitions); agent lifecycle
  tests; IPC auth/validation tests; restart recovery tests.

## Known issues

- User-session agent does not survive logout; Windows Service needs
  credential-store redesign (see background-agent doc).

## Completion status

Not started.
