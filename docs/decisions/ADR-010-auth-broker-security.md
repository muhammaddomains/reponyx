# ADR-010: Auth Broker Security

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

The GitHub App private key is the most sensitive secret in the
system: anyone holding it can mint installation tokens for every
installation of the app. It must never reach user machines.

## Decision

A dedicated **Auth Broker** service (Phase 5) is the sole holder of
the private key:

- Key lives in the broker environment / secret manager; never in the
  desktop binary, frontend, Tauri resources, config, logs, or repo.
- Broker mints short-lived installation tokens, validates installation
  ownership and permissions, authenticates desktop clients, supports
  secure one-time device pairing, rate-limits, and records audit events.
- CI secret-scans desktop artifacts; any private key or token found
  fails the build.

## Consequences

- Key compromise is contained to the broker's environment, but its
  blast radius is all installations — so the broker needs its own
  hardening, rotation plan, and incident procedure (Phase 5/10).
- Desktop installs cannot mint tokens offline; PAT auth covers
  broker-less use (ADR-004).
- "Encryption" is not the security argument: the argument is
  separation — the key is simply absent from the desktop.
