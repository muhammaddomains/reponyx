# Background Agent Architecture

Status: Accepted (Phase 1) · Code landed 2026-11-21, NOT verified
complete — see `ai/memory/PROJECT_STATUS.md` ("Phase 6 / Phase 7
verification"). The agent binary, IPC server, scheduler loop, and
queue startup exist in `apps/background-agent`, but the crate is not
a workspace member (cannot be built), the scheduler loop does not
compile (`materialize_due` called with one argument, needs two),
the agent is PAT-only, and nothing in the desktop launches or talks
to it yet.

## Purpose

The background agent keeps scheduled synchronization alive
independently of the desktop UI. The UI must never be responsible for
keeping sync alive.

## Responsibilities

- Load configuration; initialize the database.
- Load credentials through the secure credential abstraction.
- Initialize scheduler and queue; discover due jobs.
- Execute synchronization; persist results; emit progress events.
- Recover interrupted work; handle graceful shutdown.

## Windows behavior (Phase 7, first implementation)

- Separate background process, started at user login via Windows Task
  Scheduler; runs minimized without opening the UI.
- Continues after the desktop window closes; system-tray integration;
  graceful shutdown; health monitoring; automatic restart after
  unexpected termination; agent update compatibility.

## Limitations (documented honestly)

- A user-session process **does not continue after logout**. Task
  Scheduler "run whether user is logged on or not" and a true Windows
  Service have different credential-store and service-account
  implications: Windows Credential Manager credentials are tied to the
  user profile, so a service running as SYSTEM / another account cannot
  read the desktop user's stored PATs without a redesign (e.g.
  DPAPI-scoped storage or broker-issued tokens). This must be resolved
  before any native Windows Service implementation is claimed.
- The process lifecycle is abstracted so a native Windows Service can
  replace the Task Scheduler launcher later without touching the
  scheduler/queue code.

## Desktop ↔ agent communication

Secure local IPC — Windows named pipes on Windows (Phase 7):

- OS-level ACLs restrict which users/processes can connect.
- Messages are validated and versioned; disconnections and agent
  restarts are detected and handled.
- No network server is exposed. Arbitrary local processes must not be
  able to issue unrestricted sync commands (see threat model).

## Recovery

On restart the agent loads schedules, loads pending tasks, detects
interrupted tasks, recovers safely, avoids duplicate jobs, and
recalculates next execution times.
