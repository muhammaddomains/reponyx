# Phase 6 — Synchronization Queue

- Status: **Implemented (code ready 2026-11-20/21)** — awaiting push/merge to `main` (GitHub integration write returned 403 from this session)
- Depends on: Phase 2, Phase 3, Phase 5

## Objectives

A reliable multi-repository sync queue: workers, concurrency,
priorities, retries with exponential backoff + jitter, persistence,
crash recovery, progress events, and production git execution via
Phase 5 credentials.

## Requirements

- `crates/sync-queue`: persistent queue in SQLite; 5 concurrent ops
  default (configurable, max 10); priority via existing job priority;
  retry via **new job rows** (domain rule: never `running` → `pending`).
- Per-repository locks (git-engine); job cancellation; graceful shutdown; timeouts.
- Retry policy: transient failures back off with jitter; permanent
  auth/permission errors are not retried indefinitely.
- Job deduplication at schedule time (Phase 7); progress reporting via broadcast events.
- Crash recovery: interrupted `running` jobs detected and safely re-queued on restart.
- Production runner: `GitJobRunner` + `CredentialResolver` (PAT / GitHub App).

## Implementation tasks

- [x] Queue crate (enqueue via Phase 2 `create_job`, claim, complete, fail, retry)
- [x] Worker pool with bounded concurrency
- [x] Retry/backoff policy + failure classification
- [x] Progress events (broadcast channel; Tauri/IPC wiring is Phase 7/8)
- [x] Recovery on startup
- [x] `JobRunner` trait + `MockJobRunner` for tests
- [x] Production `GitJobRunner` (`mirror_clone` / `mirror_fetch` / `working_fetch` / working clone + Phase 5 credentials)
- [x] `DefaultCredentialResolver` (PAT via OS store; GitHub App via broker + device token)
- [ ] Desktop / background-agent wiring (Phase 7 remaining / Phase 8)

## Crate layout

```
crates/sync-queue/
  src/
    lib.rs
    config.rs              # QueueConfig (concurrency, backoff, timeouts)
    runner.rs              # JobRunner trait + MockJobRunner
    credentials_resolve.rs # CredentialResolver + DefaultCredentialResolver
    git_runner.rs          # GitJobRunner
    executor.rs            # claim → run → complete / fail / retry
    pool.rs                # WorkerPool + graceful shutdown
    progress.rs            # SyncProgressEvent
    recovery.rs            # abandoned running → fail + new pending job
    retry.rs               # classify + exponential backoff + jitter
  tests/
    queue_integration.rs
    git_runner_unit.rs
```

## Dependencies

Phases 2–5. Workspace crates used:

- `reponyx-core`, `reponyx-database`
- `reponyx-git-engine`
- `reponyx-credential-store`, `reponyx-github-provider`

## Acceptance criteria

- [x] 100-repo mocked sync completes with bounded concurrency (integration test)
- [x] Abandoned running jobs recover without reusing the same job id
- [x] Permanent failures stop without retry; transient failures retry within budget
- [x] GitJobRunner resolves credentials without putting tokens in URLs

## Testing

```bash
cargo test -p reponyx-sync-queue
cargo clippy -p reponyx-sync-queue --all-targets
```

- Unit: config clamp, classify/backoff, mock runner, storage.root missing, LFS reject
- Integration: 100 jobs, transient retry, permanent auth, crash recovery, graceful shutdown

## Known issues

- Retry backoff sleeps only a capped slice in the worker; the new pending job is claimable immediately by other workers (acceptable for Phase 6; optional `not_before` column later).
- Working-clone path uses `run_git_command` + `never_cancel` for cancel watch; outer job timeout still applies.
- Official `set_local_path` helper not yet in `reponyx-database` — GitJobRunner updates via bound SQL (equivalent).
- **This session could not push to GitHub** (`403 Resource not accessible by integration`). Apply locally via `APPLY.md` / tarball.

## Completion status

Core queue + GitJobRunner implemented in deliverable tree. Merge to `main` and green CI still required before claiming Phase 6 complete on the remote.
