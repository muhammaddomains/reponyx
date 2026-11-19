# Phase 7 — Scheduler and Background Agent

- Status: Partial — scheduler crate only (2026-11-19). The
  background agent, IPC, and Task Scheduler work are blocked on
  Phase 6 (no sync queue to run).
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

- [x] Scheduler crate + next-run calculation (incl. DST)
- [ ] Background agent binary + lifecycle — blocked on Phase 6
  (the agent runs scheduler + queue; there is no queue yet)
- [ ] Task Scheduler registration + tray — blocked on Phase 6
- [ ] IPC crate (protocol v1) + desktop client — blocked on Phase 6
- [ ] Agent health + restart detection in UI — blocked on Phase 6

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
- A daily/weekly time outside its own window never fires (strict
  window semantics — the run time is fixed, so the window cannot
  shift it; the next-run preview shows no runs for such a schedule).
  Interval schedules filter their candidates by the window instead.

## Completion status

Partial (2026-11-19): the scheduler crate is done — next-run
calculation (incl. DST), precedence via Phase 2, due detection,
job creation for due schedules, Tauri commands + TS types, docs.
The rest of the phase (background agent, Task Scheduler/tray, IPC,
agent health UI) is blocked on Phase 6: the agent runs scheduler +
queue, and there is no queue yet. The scheduler creates job rows
only; nothing runs them until Phase 6 lands.
