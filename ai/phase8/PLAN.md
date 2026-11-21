# Phase 8 — Desktop Interface

- Status: Started 2026-11-21 — Schedules page done; the rest not started
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
- [ ] Wire each remaining section to real Tauri commands
- [ ] Repository list virtualization + bulk selection
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

- Repository scope on the Schedules page needs a typed UUID — no
  Repositories page or repository list command exists yet.
- No frontend tests yet (the plan asks for them); pages are verified
  by typecheck + build + eslint only.

## Completion status

Started 2026-11-21. Schedules page done and verified (typecheck +
desktop build + eslint pass). Repositories, Synchronization,
Activity, Storage, Settings still placeholders.
