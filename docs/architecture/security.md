# Security Model

Status: Accepted (Phase 1) · See also `docs/security/threat-model.md`

## Boundaries

1. **Desktop app** — trusted UI + Tauri backend. Holds no private keys
   and no long-lived GitHub tokens. Reads credentials only through the
   OS credential store, per operation.
2. **Auth Broker** — the only component holding the GitHub App private
   key. Issues short-lived installation tokens (~1 h). Rate-limited,
   audited, never logs secrets.
3. **Background agent** — same trust level as the desktop; same
   credential rules; IPC-authenticated.
4. **Git subprocesses** — untrusted input surface: repository content
   is untrusted data. Git runs with argument arrays, controlled env,
   timeouts; its output is redacted before logging.
5. **Local disk** — the archive is only as safe as the machine; disk
   theft is a residual risk (see threat model §10). No claim is made
   that encryption alone makes the system secure.

## Controls (Phase 1, implemented)

- Git via argument arrays only; no shell; no string concatenation.
- Path-component validation before any filesystem use.
- Secret redaction (`crates/logging::redact_secrets`) for
  `Authorization:` headers, `token=`/`password=`/`client_secret=`
  values, and GitHub PAT shapes (`ghp_…`, `github_pat_…`).
- No secrets in SQLite, logs, URLs, or process arguments.
- Least-privilege GitHub permissions (Contents: Read, Metadata: Read).
- Structured errors with stable codes; no credential material in
  error details.

## Controls (planned)

- Windows Credential Manager integration (Phase 5, ADR-008).
- IPC ACLs + authentication (Phase 7, ADR-009).
- Auth Broker rate limiting, audit events, pairing revocation (Phase 5).
- Dependency auditing (`cargo audit`, `pnpm audit`) in CI (Phase 10).
- Diagnostics export that excludes credentials and source content
  (Phase 9).

## Non-goals / residual risks

- RepoNyx does not protect a repository's contents from a malicious
  local administrator or from disk theft without OS-level disk
  encryption (BitLocker/FileVault).
- A compromised Auth Broker can mint tokens for installed accounts
  until installations are revoked; mitigation is least-privilege
  scopes, short token lifetimes, audit logs, and revocation tooling.
- See `docs/security/threat-model.md` for the full 12-threat model.
