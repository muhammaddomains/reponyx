# Authentication Architecture

Status: Accepted (Phase 1) · Implemented: Phase 5

## Methods

1. **GitHub App (primary).** One public GitHub App, installable by
   personal accounts and organizations. Minimum permissions:
   Metadata: Read, Contents: Read. No write/admin access for sync.
2. **Fine-grained PAT (alternative).** Per-account manual entry;
   Contents: Read, Metadata: Read. Multiple PATs supported.

Both flow through the same `GitProvider` trait
(`crates/provider-abstractions`): the sync engine receives a
short-lived credential and never knows which method supplied it.

## GitHub App flow

1. Admin creates the GitHub App once; App ID, Client ID, private key,
   and callback live in the Auth Broker environment only.
2. Team installation link is published.
3. Member authenticates with GitHub, installs the app on their account
   or organization, selects repositories.
4. GitHub redirects to the broker's setup/callback URL.
5. Broker validates the installation, stores metadata, and the member
   completes a secure one-time pairing with the central desktop.
6. Desktop discovers repositories via the installation; sync starts
   per policy.

## Auth Broker (Phase 5)

A separate service that:

- Holds the GitHub App **private key exclusively** — never in the
  desktop binary, frontend, Tauri resources, config, logs, or repo.
- Generates installation access tokens (~1 h lifetime), fresh per use.
- Validates installation ownership and requested permissions.
- Authenticates desktop clients; supports secure device pairing.
- Rate-limits requests; records security audit events; never logs secrets.

## Device pairing

- Cryptographically random, single-use, short-lived pairing code.
- Expiration, rate limiting, device identification, revocation,
  audit history. No permanent pairing codes.
- An installation is never assumed to belong to a person without
  validating GitHub identity through the approved pairing flow.

## PAT handling

- PATs encrypted in the OS credential store (Windows Credential
  Manager on Windows). Never raw in SQLite, JSON, logs, frontend
  state, or config.
- Token validation, expiration detection, manual replacement, removal,
  reconnection, per-account status. Expiration warning + reconnect flow.

## Token lifecycle

Installation tokens expire after ~1 hour; the broker mints fresh ones
on demand. PAT lifetimes vary per token and are tracked per account.
Permanent auth/permission failures are never retried indefinitely.
