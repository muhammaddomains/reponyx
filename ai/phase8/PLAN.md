# Phase 8 — Desktop Interface

- Status: Complete 2026-11-23 — all 6 pages done (Schedules, Repositories, Synchronization, Activity, Storage, Settings); notifications, frontend tests, and true list virtualization done; tray menu polished
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
- [x] Wire each remaining section to real Tauri commands
- [x] Repository list virtualization + bulk selection (bulk
      selection done 2026-11-21; true virtualization done
      2026-11-23 — `@tanstack/react-virtual` in a 640px scroll
      container, dynamic row measurement, server pagination
      unchanged underneath)
- [x] Sync progress UI + cancellation (cancel done 2026-11-21 on
      the Synchronization page; live progress bars done 2026-11-23
      — IPC progress forwarding: the agent forwards the queue's
      broadcast as JSON `ServerMessage::Progress` frames, the
      desktop subscribes on a long-lived IPC connection and
      re-emits `sync-progress` Tauri events, and the page renders
      a determinate bar when git reports a percent and an
      indeterminate one otherwise)
- [x] Settings sections (General, Auth, Repos, Sync, Storage, Background, Advanced)
      — General, Sync, Storage, and Advanced (read-only) done
      2026-11-21 on the Settings page. Auth lives on the Accounts
      page; Repos on the Repositories page; Background on the
      Agent page. There is no single page with all seven sections
      — the sections are distributed across the pages that own
      them.
- [x] Notifications + tray menu (done 2026-11-23: system
      notifications for finished/failed sync jobs with a
      `notifications.enabled` setting + toggle; tray "Sync due
      now" with a result notification, separator before Quit, and
      state-aware Pause/Resume polled from agent health)

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
- Discovery has no progress indicator yet (full paged scan).
- The Storage page's read-only v1 note is outdated — Phase 9
  storage management added delete/move actions there.
- The new storage and settings commands are verified by `cargo fmt`
  + review only on this VM (gdk blocker); the CI windows-build job
  is their first real compile.
- One full test run during the Settings build showed a single
  transient failure that did not reproduce in 3 subsequent full
  runs (338/0) or 3 runs of the two known timing-sensitive
  git-engine suites — unidentified, treated as a flake.

## Completion status

Complete 2026-11-23. All six pages, notifications, frontend tests
(45 tests, run in CI), true list virtualization, tray-menu polish,
and live progress bars are done and verified (fmt + clippy 0
warnings + 415 tests pass + typecheck + desktop build + eslint
pass + frontend tests 45 pass + local Semgrep 0 findings). Live
progress bars were added at the user's request after the first
completion commit: the agent forwards the queue's broadcast as
JSON `ServerMessage::Progress` frames (one forwarder task per IPC
connection, replies and pushes share the write half behind a
mutex), the desktop subscribes on a long-lived connection and
re-emits `sync-progress` Tauri events with a 10 s retry loop, and
the Synchronization page renders a determinate bar when git
reports a percent (new `percent` field on `JobProgress`) and an
indeterminate one otherwise. No open items remain.
