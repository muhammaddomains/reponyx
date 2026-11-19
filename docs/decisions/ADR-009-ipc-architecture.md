# ADR-009: IPC Architecture

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

The desktop UI and background agent are separate processes that must
coordinate: enqueue syncs, report progress, pause/resume, and report
agent health — without exposing a network service.

## Decision

**Secure local IPC** (Phase 7): Windows named pipes on Windows, Unix
domain sockets elsewhere. Versioned, validated messages; OS-level
ACLs; desktop-to-agent authentication; no TCP listener.

## Consequences

- No network attack surface; localhost-only by construction.
- A malicious local process is still a threat (see threat model §1,
  §9): ACLs + authentication + strict message validation are required,
  not optional.
- Protocol versioning lets the agent and desktop evolve independently;
  mismatches are reported, never silently tolerated.
