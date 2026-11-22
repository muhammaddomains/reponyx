# Phase 9 — Advanced Features

- Status: In progress — LFS sync + verification, submodule sync,
  and Both-mode completion done 2026-11-21
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

- [x] LFS sync + verification
- [x] Submodule sync
- [x] Both-mode completion (mirror + working clone)
- [x] Storage management UI + orphan detection
- [x] Diagnostics export + DB backup tools
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

- Relative submodule URLs are resolved against the mirror's
  origin URL with git's own rule (the full origin URL is the
  base: `../x` consumes the superproject's last segment, `./x`
  does not); a raw `git fetch <relative-url>` would resolve
  against the process cwd instead, so the engine resolves it
  first (see `submodule.rs`, `resolve_submodule_url`).
- The working-clone layout is
  `<root>/working-clones/<account>/<owner>/<name>` — no `github`
  segment, unlike the mirror path.
- `git lfs fetch` silently skips an object file that merely
  exists, even when corrupt — the engine deletes provably-corrupt
  objects before fetching so they re-download (see `lfs.rs`).
- Local-path remotes hardlink LFS objects; see the LFS tests for
  why corruption fixtures write a new file instead of editing in
  place.
- Renames update the repository row but not `local_path`
  (discovery, by design), so a renamed repo's archive directory
  keeps the old name until the Storage page moves it; the move
  refuses to overwrite an existing destination and rolls back
  on failure.
- Deletes never trust the UI's word alone: every delete re-scans
  the storage root at delete time and refuses paths outside the
  root, non-archive directories, and (for orphans) paths a
  repository row still points at.
- A database restore replaces the live file only at the next
  startup, and only after the backup passed validation (SQLite
  header + `integrity_check` + `settings` table) and the
  background agent was stopped over IPC — the agent holds the
  database open, and it is re-spawned on demand. The replaced
  live database is kept as a `pre-restore` fallback, and stale
  WAL sidecars are dropped before the swap so old frames cannot
  replay into the restored file.
- The diagnostics export is built from redacted strings and then
  scanned again (`verify_no_secrets`): a failed scan writes
  nothing. Only the four seeded non-secret settings are included.

## Completion status

LFS sync + verification done 2026-11-21 (engine module +
runner wiring + 5 fixture tests; `cargo test` 352 passed /
0 failed). Submodule sync + Both-mode completion done the same
day (engine `submodule` module + runner wiring + 9 fixture
tests; `cargo test` 373 passed / 0 failed). Storage management
+ orphan detection done the same day (engine scan + safe
delete, DB `set_local_path`, 3 new Tauri commands, Storage
page sections + actions; `cargo test` 378 passed / 0 failed).
Diagnostics export + DB backup done the same day (secret scan
+ log tail in `reponyx-logging`, counts + `VACUUM INTO` backup
+ validation in `reponyx-database`, 6 new Tauri commands,
Settings page section, restore staged for the next startup;
`cargo test` 395 passed / 0 failed).
Remaining: import/export, performance.
