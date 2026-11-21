# PROJECT_STATUS.md — Phase 6 snippet (paste into ai/memory/PROJECT_STATUS.md after merge)

**Insert under Current phase / Completed tasks after the code is on `main` and tests pass.**

```
Phase 6 — Sync Queue — IMPLEMENTED (2026-11-21, pending remote merge if not yet on main).

- crates/sync-queue: WorkerPool, claim/complete/fail/retry, crash recovery,
  progress broadcast events, JobRunner trait.
- GitJobRunner: mirror_clone / mirror_fetch / working_fetch / working clone
  via git-engine; credentials via DefaultCredentialResolver (PAT from OS
  store, GitHub App via Auth Broker).
- Retries create a new sync_jobs row (never running → pending).
- Tests: queue_integration (100 jobs, retry, auth permanent, recovery),
  git_runner_unit (storage.root, LFS reject).
- Workspace: members += crates/sync-queue; reponyx-sync-queue path dep.
- Deliberately NOT done: background-agent binary / Task Scheduler / tray
  (rest of Phase 7), desktop queue UI (Phase 8), LFS/submodules (Phase 9).
```

**Current phase line should become something like:**

```
Phase 6 — Sync Queue — complete 2026-11-21 (after merge + cargo test -p reponyx-sync-queue).
Phase 7 — Scheduler and Background Agent — PARTIAL (scheduler crate only; agent still blocked until queue is merged and wired).
```
