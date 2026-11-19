# Phase 8 — Desktop Interface

- Status: Not started
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

- [ ] Wire each section to real Tauri commands
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

None yet.

## Completion status

Not started.
