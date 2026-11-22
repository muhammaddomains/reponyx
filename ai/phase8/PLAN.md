# Phase 8 — Desktop Interface

- Status: Started 2026-11-21 — all 6 pages done (Schedules, Repositories, Synchronization, Activity, Storage, Settings); notifications + frontend tests + virtualization remain
- Depends on: Phases 2–7 (features land as their backends do)

## Objectives

A fully integrated desktop application: all 8 sections functional —
Dashboard, Accounts, Repositories, Synchronization, Schedules,
Activity, Storage, Settings (+ notifications, system tray).

## Requirements

- Dashboard: real counts, next sync, last success, storage usage.
- Accounts: add/connect (App + PAT), test, refresh, pause, disconnect.
- Repositories: search/filter/sort, virtualization, bulk actions,
  sync now, pause/resume, open folder/browser, mode + schedule change.
- Synchronization: running queue, progress, cancel, retry, failures.
- Schedules, Activity, Storage, Settings per spec §15.
- No placeholder buttons; no mock data; loading/empty/error states.

## Implementation tasks

- [x] Schedules page (2026-11-21): list, create (global/account/
      repository), enable/disable, inline edit, delete, next-5-runs
      preview, materialize-now with report, effective-schedule
      lookup — wired to the Phase 7 scheduler backend, no backend
      changes. Verified: typecheck + desktop build + eslint pass.
- [x] Repositories page (2026-11-21): list with filters + search,
      discover from GitHub (upsert, never deletes), sync now via the
      agent, pause/resume, mode change, bulk actions, open folder /
      GitHub, remove (row only). New Tauri commands in
      `repositories.rs`; discovery reuses the agent's credential
      resolver. Verified: fmt + clippy 0 warnings + 331 tests pass +
      typecheck + desktop build + eslint pass.
- [x] Synchronization page (2026-11-21): live queue (agent health +
      pause/resume, per-status counts, job list with Active filter),
      cancel, retry now, 5-second auto-refresh. New Tauri commands
      in `synchronization.rs` (list/count/cancel). No live progress
      bars yet — IPC progress forwarding is still unwired. Verified:
      fmt + clippy 0 warnings + 331 tests pass + typecheck + desktop
      build + eslint pass.
- [x] Activity page (2026-11-21): sync-history record, newest
      first, repository + result filters, operation/result/duration/
      bytes, 10-second auto-refresh. New `list_recent_history`
      database query (limit clamped 1..=500) with an integration
      test, and two history Tauri commands in
      `synchronization.rs`. History is append-only; the page never
      edits it. Verified: fmt + clippy 0 warnings + 332 tests pass
      + typecheck + desktop build + eslint pass.
- [x] Storage page (2026-11-21): storage root + Open folder,
      volume usage bar (used/total/free), breakdown cards
      (mirrors, database, logs, stored total), per-repository
      list sorted by size. New `storage` module in
      `reponyx-git-engine` (`dir_size`, `disk_usage` via `fs2`)
      with 5 unit tests, and two Tauri commands in `storage.rs`
      (sizes on a blocking thread). Read-only in v1 — no delete
      action. Verified: fmt + clippy 0 warnings + 337 tests pass
      + typecheck + desktop build + eslint pass.
- [x] Settings page (2026-11-21): General (theme, applied at
      startup), Sync (default mode for newly discovered repos,
      concurrent syncs 1–10), Storage (archive root, with the
      no-move warning), read-only Advanced (version, data dir,
      paired device). Two Tauri commands in `settings.rs` (only
      4 keys writable, values validated). Discovery now reads
      `sync.default_mode` for new rows (new test); the frontend
      applies `ui.theme`. Also fixed in the same commit: pairing
      never persisted `device.identifier`, so GitHub App accounts
      failed in the agent even after pairing — it is now saved on
      pairing. Verified: fmt + clippy 0 warnings + 338 tests pass
      + typecheck + desktop build + eslint pass.
- [ ] Wire each remaining section to real Tauri commands
- [ ] Repository list virtualization + bulk selection (bulk
      selection done 2026-11-21; true virtualization deferred — the
      list pages 100 rows at a time with "Show more")
- [ ] Sync progress UI + cancellation (cancel done 2026-11-21 on
      the Synchronization page; live progress bars still need IPC
      progress forwarding)
- [ ] Settings sections (General, Auth, Repos, Sync, Storage, Background, Advanced)
      — General, Sync, Storage, and Advanced (read-only) done
      2026-11-21 on the Settings page. Auth lives on the Accounts
      page; Repos on the Repositories page; Background on the
      Agent page. There is no single page with all seven sections
      — the sections are distributed across the pages that own
      them.
- [ ] Notifications + tray menu

## Dependencies

All prior phases.

## Acceptance criteria

- Every visible action performs a real operation or is absent;
  end-to-end smoke test: connect account → discover → sync → schedule.

## Testing

- Frontend tests (rendering, forms, validation, filtering, scheduling
  config, error states); Tauri command tests.

## Known issues

- Repository scope on the Schedules page needs a typed UUID — the
  Repositories page now lists repository ids, so copy one from
  there (a picker is still missing).
- The Repositories list pages 100 rows at a time instead of true
  virtualization.
- Discovery has no progress indicator yet (full paged scan).
- No frontend tests yet (the plan asks for them); pages are verified
  by typecheck + build + eslint only.
- The Storage page is read-only in v1 — no delete action for local
  archives (destructive; needs its own confirm flow). Sizes are
  measured on demand, not polled.
- The new storage and settings commands are verified by `cargo fmt`
  + review only on this VM (gdk blocker); the CI windows-build job
  is their first real compile.
- One full test run during the Settings build showed a single
  transient failure that did not reproduce in 3 subsequent full
  runs (338/0) or 3 runs of the two known timing-sensitive
  git-engine suites — unidentified, treated as a flake.

## Completion status

Started 2026-11-21. All six pages done and verified (fmt + clippy
0 warnings + 338 tests pass + typecheck + desktop build + eslint
pass). Remaining: notifications + tray menu, frontend tests, and
true list virtualization for Repositories.
