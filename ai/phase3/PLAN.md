# Phase 3 — Git Engine

- Status: Complete (2026-11-18, on `main`)
- Depends on: Phase 1 (detection, builders, process, paths already exist)

## Objectives

Turn the Phase 1 Git primitives into a complete, independently tested
synchronization engine: mirror clone, working clone, fetch/prune/tags,
branch discovery, cancellation, error handling, repository locks.

## Requirements

- Git detection + version validation (done in Phase 1; extend with LFS detection).
- Mirror clone + incremental fetch with prune; working-clone fetch-all.
- Per-repository locking (one operation per local path).
- Credential injection via short-lived credential helper (no tokens in URLs/argv).
- Branch/tag discovery from a mirror; divergence + dirty-state detection for working clones.
- Cancellation + timeouts end to end.

## Implementation tasks

- [x] Repository lock manager
- [x] Mirror clone/fetch operations with progress reporting
- [x] Working-clone fetch + status/dirty detection
- [x] Credential helper integration
- [x] LFS availability detection (sync itself is Phase 9)
- [x] Recovery: interrupted/locked/corrupt repo handling

## Dependencies

Phase 1 git-engine + database (job persistence in Phase 6).

## Acceptance criteria

- Integration tests on local bare fixtures: mirror clone, fetch after
  new commits, prune of deleted branches, tag preservation, lock
  contention, cancellation, corrupt-repo detection.
- No network clones in tests; no real GitHub credentials.

## Testing

- Unit: command construction, error mapping, path validation.
- Integration: local fixtures; failure injection (disk full, lock, corrupt).

## Known issues

- This VM's `/tmp` (tmpfs) intermittently returns ETXTBSY
  ("Text file busy") when a freshly written script is executed
  immediately under parallel load — reproduced independently at
  ~5% with a write/chmod/exec stress loop; exec of an unchanged
  file never flakes. The LFS-shim test retries detection up to 5
  times for that reason (test only).
- Tests run as root (uid 0) on this VM, so the read-only-parent
  clone test skips itself there (permission bits do not stop
  root). The RLIMIT_FSIZE disk-full test is the real write-failure
  coverage.
- git-lfs is not installed on this VM; the LFS "present" path is
  covered by a fake shim in tests.

## Completion status

Complete 2026-11-18. Verification (actual): `cargo fmt --check`
pass; clippy (`--workspace --exclude reponyx-desktop
--all-targets`) pass, 0 warnings; `cargo test --workspace
--exclude reponyx-desktop` **156 passed, 0 failed** (core 11,
database 54, git-engine 87, logging 4, provider-abstractions 0);
`pnpm -r typecheck` pass; `pnpm --filter @reponyx/desktop build`
pass (2305 modules, 1.82 s). Git-engine suite re-run 3×
consecutively: 87 passed, 0 failed each run. Tauri binary build
still blocked on this VM by missing `gdk-3.0` / WebViewGTK system
libraries — not claimed passed. Deliberately not done: LFS sync
(Phase 9), repair of corrupt repos (diagnosis only), desktop/UI
changes.
