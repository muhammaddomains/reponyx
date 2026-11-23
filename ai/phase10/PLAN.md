# Phase 10 — Security, QA and Release

- Status: In progress — first pass done 2026-11-22 (see below); Windows
  verification items and the first release still open. Durable broker
  storage (F1) done 2026-11-22; key-rotation runbook (F3) done
  2026-11-23.
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

- [x] Security audit + remediation — threat model reviewed
  mitigation-by-mitigation against the real code 2026-11-22; findings
  F1 (broker state in-memory only — open), F2 (named pipe has no ACL
  — documented, bounded by the same-user boundary), F3 (no
  key-rotation / incident runbook — open) recorded in
  `docs/security/threat-model.md`. Dependency audits run for the
  first time: `pnpm audit` clean; `cargo audit` found one
  vulnerability (`url` 2.5.8, RUSTSEC-2026-0592) and it was fixed
  the same day (`url` → 2.5.9); re-run clean (0 vulnerabilities,
  7 warnings with no available fix — see PROJECT_STATUS).
- [x] CI pipeline (fmt, clippy, tests, typecheck, lint, builds, audits)
  — verified already in place and green: ci.yml, lint.yml (private-key
  scan), security-audit.yml (Semgrep), codeql.yml,
  scheduled-maintenance.yml (weekly cargo audit + pnpm audit, also
  workflow_dispatch; first scheduled run is Monday 2026-11-23).
- [x] Failure-recovery test suite — coverage already existed across
  crates; inventoried in `docs/qa/failure-recovery.md` with the three
  honest gaps (no real process-kill test, no mid-transfer network
  loss, no Windows failure tests).
- [x] Performance test + report — done in Phase 9 (measured at 1,000
  repos / 10,000 jobs; `crates/database/tests/performance.rs`).
- [ ] Windows packaging + signing — release workflow exists
  (tag-triggered, NSIS + MSI, signed updater artifacts) but
  `TAURI_SIGNING_PRIVATE_KEY` (+ password) are not yet set in repo
  secrets, and the installer has never run on a real Windows machine.
  Verification checklist written 2026-11-23
  (`docs/qa/windows-verification.md`): installer, Task Scheduler
  autostart, Credential Manager, binary secret scan, real
  process-kill recovery.
- [x] Upgrade/migration testing — V1→V4 and V3→V4 upgrade tests in
  `crates/database/tests/migrations.rs` (data intact, dedupe index
  works on new rows; pre-upgrade rows carry NULL dedupe_key —
  documented).
- [x] Release documentation — README rewritten (was still "Phase 1"),
  user guide + security notes added; `docs/qa/failure-recovery.md`
  added; CHANGELOG updated.

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
- Broker state is durable (F1, fixed 2026-11-22): SQLite store
  survives restarts; token cache and rate-limiter buckets remain
  in-memory.
- Key-rotation / incident runbook (F3, fixed 2026-11-23):
  `docs/security/key-rotation-runbook.md` covers rotation of the
  GitHub App private key, broker setup token, device tokens, IPC
  shared secret, and PATs, plus incident response and audit
  queries.
- The `cargo audit` / `pnpm audit` weekly CI job ran for the first
  time Monday 2026-11-23 (run #1 failed on the Rust audit job;
  manual re-run #2 found `cookie` 0.18.2, RUSTSEC-2026-0594,
  fixed the same day by bumping to 0.18.3).

## Completion status

First pass done 2026-11-22 (audit, threat-model review, migration
tests, docs, secret scan re-run). F1 (durable broker storage) done
2026-11-22. F3 (key-rotation runbook) done 2026-11-23. Windows
verification checklist (`docs/qa/windows-verification.md`) written
2026-11-23; dead duplicate agent `windows_task.rs` removed and the
stale README "Task Scheduler not wired" line fixed the same day.
Not complete: running the checklist on a real Windows machine,
first release.
