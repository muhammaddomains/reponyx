# Phase 3 — Git Engine

- Status: Not started
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

- [ ] Repository lock manager
- [ ] Mirror clone/fetch operations with progress reporting
- [ ] Working-clone fetch + status/dirty detection
- [ ] Credential helper integration
- [ ] LFS availability detection (sync itself is Phase 9)
- [ ] Recovery: interrupted/locked/corrupt repo handling

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

None yet.

## Completion status

Not started.
