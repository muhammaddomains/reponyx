# ADR-003: GitHub App Authentication (Primary)

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

Users need RepoNyx to access private repositories across personal and
organization accounts without handing over broad personal tokens.
GitHub Apps give scoped, installable, revocable access; a single app
can be installed by many accounts.

## Decision

GitHub App is the **primary** authentication method (Phase 5):

- One public GitHub App, installable by personal accounts and orgs.
- Minimum permissions: Metadata: Read, Contents: Read.
- The App's private key lives **only** in the Auth Broker environment.
- Installation tokens (~1 h) are minted fresh on demand by the broker.
- Team members authorize via browser + secure one-time device pairing;
  no RepoNyx install is needed to authorize.

## Consequences

- Least privilege by design; access is revocable per installation.
- Requires a hosted Auth Broker — a real operational dependency.
- A compromised broker is high-impact (mitigated by short token life,
  least privilege, audit logs; see threat model).
- PAT remains available as the alternative (ADR-004).
