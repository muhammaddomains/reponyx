# Phase 9 — Advanced Features

- Status: Not started
- Depends on: Phases 3, 6, 7, 8

## Objectives

Git LFS, submodules, both clone modes, storage management,
diagnostics, backup tools, import/export, performance improvements.

## Requirements

- LFS: availability check, `lfs fetch`, object verification, failure
  reporting, storage accounting — as a separate step from mirror sync.
- Submodules: optional recursive sync; graceful handling of
  inaccessible private submodules.
- Both-modes per repository (mirror + working clone).
- Storage: usage reporting, disk monitoring, orphan detection,
  cleanup, safe rename handling.
- Diagnostics export (excludes credentials + source content);
  database backup/restore; import/export of settings/selections.
- Performance: pagination everywhere, bounded concurrency, no
  full-list loads; measure before claiming limits.

## Implementation tasks

- [ ] LFS sync + verification
- [ ] Submodule sync
- [ ] Storage management UI + orphan detection
- [ ] Diagnostics export + DB backup tools
- [ ] Import/export
- [ ] Performance pass with measurements

## Dependencies

Phases 3, 6–8.

## Acceptance criteria

- LFS objects archived and verified for a fixture repo; submodule
  failures isolated per repo; diagnostics export contains no secrets
  (verified by scan).

## Testing

- LFS/submodule fixture tests; storage + orphan tests; export scan tests.

## Known issues

None yet.

## Completion status

Not started.
