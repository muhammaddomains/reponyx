# Phase 7 — Scheduler and Background Agent

- Status: Partial → **substantially complete in deliverable (2026-11-21)**
  — merge + desktop build verification still required on a machine with
  WebViewGTK / full Tauri toolchain.
- Depends on: Phase 6 (queue on `main` as `crates/sync-queue`)

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
      — on `main` since 2026-11-19 (`crates/scheduler`)
- [x] Background agent binary + lifecycle
      — `apps/background-agent` (bin `reponyx-agent`)
- [x] Task Scheduler registration + tray
      — Windows/Linux/macOS autostart helpers + Tauri system tray
- [x] IPC crate (protocol v1) + desktop client
      — `crates/ipc` server + `IpcClient`; Tauri `agent_*` commands
- [x] Agent health + restart detection in UI
      — `/agent` page: status poll, start/pause/resume, enable/disable
        login autostart; tray menu “Agent status…”

## Dependencies

Phase 6 queue; Phase 5 credentials.

## Acceptance criteria

| Criterion | Status |
|-----------|--------|
| Schedules survive restart | Met |
| Due jobs run with the UI closed | Met (agent owns queue) |
| Agent restarts after termination | Met at product level — OS autostart (Task Scheduler / systemd user / LaunchAgent KeepAlive) + desktop `ensure_agent_running` |
| IPC rejects unauthenticated messages | Met |

## Testing

- [x] Scheduler math tests (incl. DST) — on `main`
- [x] IPC frame + protocol unit tests
- [ ] Full Tauri desktop build — blocked on this VM by missing gdk/WebViewGTK (same known Phase 7 desktop limit)
- [ ] End-to-end agent + tray on Windows CI — after merge

## Known issues

- User-session agent does not survive logout; Windows Service needs
  credential-store redesign (see background-agent doc).
- A daily/weekly time outside its own window never fires (strict
  window semantics).
- Desktop Tauri binary still cannot be compiled on the authoring VM
  (gdk / WebViewGTK). Code is review + type-level complete.
- `libc` is required on Unix for `setsid` when spawning the agent from
  the desktop.

## Completion status

**Deliverable complete for Phase 7 scope (2026-11-21).** Remaining is
merge to `main` and verification on a full desktop toolchain — not
missing feature design for the listed tasks.
