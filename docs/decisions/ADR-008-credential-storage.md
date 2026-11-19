# ADR-008: Credential Storage

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

PATs and tokens must be stored somewhere the app can retrieve them
unattended (for background sync) without exposing them in files, logs,
or the database.

## Decision

**OS credential store only**: Windows Credential Manager on Windows
(Phase 5, `credential-store` crate); macOS Keychain / Linux
Secret Service equivalents on other platforms. Rules:

- Never raw PATs in SQLite, JSON, logs, frontend state, or config.
- Never tokens in Git remote URLs or process arguments.
- Short-lived credentials (installation tokens) are minted per use
  and discarded, never persisted.
- The GitHub App private key is never stored on the desktop at all
  (Auth Broker only).

## Consequences

- Credentials benefit from OS-level protection (DPAPI on Windows).
- Background sync as a different user/service account cannot read
  the desktop user's credentials — a known constraint for any future
  Windows Service design.
- Credential-store failures surface as `CredentialStoreError` with a
  reconnect action.
