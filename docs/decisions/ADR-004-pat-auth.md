# ADR-004: Fine-Grained PAT Authentication (Alternative)

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

Not every user can (or wants to) install a GitHub App — e.g. single
users, restricted orgs, or offline setup. A manual token path keeps
RepoNyx usable without the broker.

## Decision

Fine-grained PATs are a **first-class alternative** (Phase 5):

- Per-account: label, username, PAT, optional repo selection and sync
  settings. Multiple PAT accounts supported; mixed auth with GitHub
  App accounts on the same installation.
- Required scopes: Contents: Read, Metadata: Read.
- PATs stored only in the OS credential store (ADR-008), never in
  SQLite/logs/config. Expiration is detected and surfaced with a
  reconnect workflow.

## Consequences

- No broker dependency for PAT-only setups.
- PATs are long-lived bearer tokens: theft impact is higher than
  1-hour installation tokens, so storage and handling rules are strict.
- The sync engine is indifferent to the method (common provider trait).
