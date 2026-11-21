# Phase 8 — Desktop Interface

- Status: Started 2026-11-21 — Schedules + Repositories pages done; the rest not started
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
- [ ] Wire each remaining section to real Tauri commands
- [ ] Repository list virtualization + bulk selection (bulk
      selection done 2026-11-21; true virtualization deferred — the
      list pages 100 rows at a time with "Show more")
- [ ] Sync progress UI + cancellation
- [ ] Settings sections (General, Auth, Repos, Sync, Storage, Background, Advanced)
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

## Completion status

Started 2026-11-21. Schedules page and Repositories page done and
verified (fmt + clippy 0 warnings + 331 tests pass + typecheck +
desktop build + eslint pass). Synchronization, Activity, Storage,
Settings still placeholders.
