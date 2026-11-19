# Phase 5 — Authentication

- Status: Not started
- Depends on: Phase 1, Phase 4

## Objectives

Both authentication methods working end to end: GitHub App (primary)
via the Auth Broker, and fine-grained PAT (alternative) via the OS
credential store.

## Requirements

- `apps/auth-broker`: holds the GitHub App private key (only there);
  installation callback; token minting; device pairing; rate limiting;
  audit events; no secret logging.
- Secure one-time pairing: random, single-use, expiring, revocable,
  audited, device-identified.
- `crates/credential-store`: Windows Credential Manager (Keychain /
  Secret Service on other platforms); PAT validation, expiration
  detection, replacement, removal, reconnection.
- Mixed auth: App + PAT accounts coexist; sync engine is indifferent.
- Explicit approval gates: no GitHub App publishing, no production
  credentials, no broker deployment without user approval.

## Implementation tasks

- [ ] Auth Broker service (callback, token minting, pairing, audit)
- [ ] Credential-store crate
- [ ] PAT account flow in desktop (add/validate/replace/remove)
- [ ] GitHub App install + pairing flow in desktop
- [ ] Token expiration warnings + reconnect UX

## Dependencies

Phase 4 provider; Phase 2 accounts/installations/devices tables.

## Acceptance criteria

- A GitHub App installation can be paired to the desktop and yields
  working discovery; a PAT account validates and discovers repos.
- Private key is provably absent from the desktop binary (CI scan).
- Expired/revoked credentials surface actionable errors, no infinite retry.

## Testing

- Mocked broker + GitHub API; pairing expiry/reuse/revocation tests;
  credential-store round-trip tests (OS store or mocked backend in CI).

## Known issues

None yet.

## Completion status

Not started.
