# Security Policy

## Supported versions

Phase 1 is a development foundation, not a release. No released version is
supported yet. Security review is a Phase 10 deliverable.

## Reporting a vulnerability

Do **not** open a public issue. Contact the maintainers privately with:

- a description of the issue and its impact,
- steps to reproduce,
- the affected version / commit.

## Security invariants (enforced from Phase 1)

- The GitHub App private key **must never** be included in the desktop
  binary, frontend code, Tauri resources, local configuration, public
  repositories, or logs. It lives exclusively in the Auth Broker
  environment (Phase 5).
- PATs and installation tokens are stored only via the OS credential store
  (Windows Credential Manager on Windows) — never as plaintext in SQLite,
  JSON, logs, or frontend state. See ADR-008.
- Tokens are never placed in Git remote URLs or process arguments; the Git
  engine injects short-lived credentials via a credential helper / controlled
  environment, and redacts secrets from all process output and logs.
- Git commands are executed via argument arrays only — no shell, no string
  concatenation of user-controlled input.
- Repository names are never trusted as filesystem paths; every path
  component is validated before use.
- Local repositories are never automatically deleted when a remote is
  deleted, an account disconnects, or a token expires.
- Local IPC (Phase 7) must be restricted by OS-level ACLs and authenticated;
  arbitrary local processes must not be able to issue sync commands.

See `docs/security/threat-model.md` for the full threat model, security
boundaries, and residual risks.
