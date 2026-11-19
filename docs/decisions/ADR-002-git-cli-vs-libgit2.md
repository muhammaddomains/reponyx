# ADR-002: Git CLI vs libgit2

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

Git operations could use the installed Git CLI or libgit2 (via
`git2`). libgit2 offers in-process calls but lags Git features and
has historically incomplete support for mirror semantics, LFS, and
some transports.

## Decision

Use the **installed Git CLI (≥ 2.40)** as the primary execution engine,
invoked via argument arrays with timeouts and cancellation
(`crates/git-engine`).

## Consequences

- Full fidelity: `clone --mirror`, `fetch --prune`, LFS, submodules,
  and credential helpers all behave exactly as Git does.
- Git must be installed (detected at startup; a clear error otherwise).
- Process-execution safety is our responsibility: no shell, controlled
  environment, secret redaction, per-repo locks.
- libgit2 remains an option for read-only local inspection later, but
  it will not replace the CLI for sync.
