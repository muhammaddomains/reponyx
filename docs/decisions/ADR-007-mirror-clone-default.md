# ADR-007: Mirror Clone as Default

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

The product's core promise is a complete local archive of Git history.
A working clone only preserves the checked-out branch's history
efficiently; a mirror preserves all refs.

## Decision

**Mirror clone is the default sync mode** (`git clone --mirror`,
`<name>.git` bare layout); working clone and "both" are opt-in per
repository (spec §9, §13).

## Consequences

- Complete history, branches, and tags are archived even for repos
  nobody opens.
- A mirror is a Git archive, not a full backup: LFS objects,
  submodules, releases, issues, wikis, and Actions artifacts need
  separate handling (Phase 9) — the UI must not imply otherwise.
- Working clones are never auto-modified: fetch only; uncommitted
  changes are detected and never discarded.
