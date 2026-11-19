# Phase 6 — Synchronization Queue

- Status: Not started
- Depends on: Phase 2, Phase 3, Phase 5

## Objectives

A reliable multi-repository sync queue: workers, concurrency,
priorities, retries with exponential backoff + jitter, persistence,
crash recovery, progress events.

## Requirements

- `crates/sync-queue`: persistent queue in SQLite; 5 concurrent ops
  default (configurable, max 10); per-account + global rate limits;
  priority queues; retry queue.
- Per-repository locks; job cancellation; graceful shutdown; timeouts.
- Retry policy: transient failures back off with jitter; permanent
  auth/permission errors are not retried indefinitely.
- Job deduplication; progress reporting to the UI/agent.
- Crash recovery: interrupted jobs detected and safely re-queued on restart.

## Implementation tasks

- [ ] Queue crate (enqueue, claim, complete, fail, retry)
- [ ] Worker pool with bounded concurrency
- [ ] Retry/backoff policy + failure classification
- [ ] Progress events (Tauri events / IPC)
- [ ] Recovery on startup

## Dependencies

Phases 2–5.

## Acceptance criteria

- 100-repo mocked sync completes with bounded concurrency; a killed
  worker's jobs recover without duplication; permanent failures stop
  after the retry budget with actionable errors.

## Testing

- Queue unit tests; failure-injection integration tests (network loss,
  rate limit, token expiry, disk full, duplicate jobs, restart).

## Known issues

None yet.

## Completion status

Not started.
