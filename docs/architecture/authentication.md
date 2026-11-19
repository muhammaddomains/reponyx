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

## As implemented (Phase 5, 2026-11-18)

- **Broker** (`apps/auth-broker`, axum): separate binary, not a
  dependency of the desktop crate. Key from
  `REPONYX_BROKER_PRIVATE_KEY` or `REPONYX_BROKER_PRIVATE_KEY_PATH`;
  setup token from `REPONYX_BROKER_SETUP_TOKEN`; binds
  `127.0.0.1:8765` by default. JWT: RS256, `iss` = App id,
  `iat` = now − 60 s, `exp` = now + 9 min. Minted tokens cached per
  installation until 5 minutes before expiry; `force_refresh`
  bypasses the cache.
- **Pairing request authentication:** the broker setup token
  (operator-supplied at startup, stored in the desktop's OS
  credential store, `X-RepoNyx-Setup-Token`, compared by SHA-256
  hash). Code: 9 chars, unambiguous alphabet, `XXX-XXX-XXX`,
  single-use, 10-minute TTL, 5 wrong-device attempts then
  invalidated. Device token: random 32 bytes, stored server-side
  only as a SHA-256 hash.
- **Rate limits:** minting 30/min per device, pairing requests
  10/min per IP, pairing confirms 20/min per IP (hand-rolled token
  buckets); 429 with `Retry-After`.
- **401 handling:** the provider client invalidates the cached
  token and retries exactly once, then fails fast with
  `AuthenticationExpired`.
- **Broker state is in-memory** (installations, codes, device
  tokens, cache, audit). A restart forgets everything; devices
  must re-pair. Durable broker storage is a deployment-phase
  hardening item.
- **PAT expiry:** `pat_expires_at` on `accounts` (V3 migration);
  warning threshold 7 days; `days_left` rounds up.
- **Private-key scan:** `scripts/check-no-private-key.sh` — sources
  + dependency tree pass; the binary part needs a machine where the
  Tauri build works.
