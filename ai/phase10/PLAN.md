# Phase 10 — Security, QA and Release

- Status: Not started
- Depends on: All prior phases

## Objectives

Release candidate with verified test results: threat-model review,
security audit, dependency audit, integration + failure-recovery
testing, performance testing, Windows packaging, upgrade/migration
handling, release documentation.

## Requirements

- Threat-model review + security audit; fix or document every finding.
- `cargo audit` + `pnpm audit` in CI; secret scanning of desktop
  artifacts (private key / token detection fails the build).
- Integration + failure-recovery test suite (expired/revoked tokens,
  network loss, rate limits, disk full, process kill, corrupt repos,
  renames, remote deletion, restarts).
- Performance testing at 1,000+ repos (measured, not claimed).
- Windows installer (NSIS), upgrade + migration handling, signed builds.
- Release docs: README, user guide, changelog, migration notes.

## Implementation tasks

- [ ] Security audit + remediation
- [ ] CI pipeline (fmt, clippy, tests, typecheck, lint, builds, audits)
- [ ] Failure-recovery test suite
- [ ] Performance test + report
- [ ] Windows packaging + signing
- [ ] Upgrade/migration testing
- [ ] Release documentation

## Dependencies

All prior phases.

## Acceptance criteria

- All gates green with recorded evidence; no "100% complete" claims
  without verifiable results; known issues listed explicitly.

## Testing

- Full suite + Windows verification (installer, Task Scheduler,
  Credential Manager) on a real Windows machine.

## Known issues

- Windows installer / Task Scheduler / Credential Manager cannot be
  fully verified on this Linux dev VM; they are explicit Phase 10
  Windows-verification items.

## Completion status

Not started.
