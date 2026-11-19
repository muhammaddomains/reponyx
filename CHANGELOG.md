# Changelog

All notable changes to RepoNyx are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added — Phase 2 (Core Database and Domain)

- **Repository-pattern data access for all 9 V1 tables** in
  `crates/database` (one module per table: accounts, installations,
  repositories, schedules, jobs, history, settings, devices,
  audit_logs), all SQL with bound parameters only. Domain types added
  in `crates/core` (`Schedule`, `SyncHistory`, `Setting`, `Device`,
  `AuditLog`, `Installation`, `AccountType`, `ScheduleScope`,
  `IntervalType`, `SyncResult`, `DevicePairingStatus`,
  `InstallationType`, `InstallationStatus`) with explicit
  allowed-transition tables (`AccountStatus`, `JobStatus`,
  `DevicePairingStatus`, `InstallationStatus`); TS mirrors added in
  `packages/shared` (including the previously missing `accountType`
  on `Account`). Highlights: upsert by `(account_id,
  provider_repository_id)` via atomic `ON CONFLICT DO UPDATE`
  (preserves local state); rename/ownership-change with
  full_name collision detection; effective-schedule resolution
  (repository > account > global) + `due_schedules`; atomic job
  claim (`UPDATE ... RETURNING`); job finish + history entry in one
  transaction (`complete_job_with_history`); audit logs append-only
  by design (no update/delete API).
- **Migration V2 (`0002_unique_full_name.sql`)**: UNIQUE index on
  `repositories (account_id, full_name)` — code-level collision
  checks can race under concurrent writes (deferred transactions),
  so the invariant is now enforced at the storage layer. V1 untouched.
- **Tests: 54 new database integration tests + 4 new core unit
  tests**, all against temp SQLite with migrations applied — CRUD
  round-trips, upsert identity, schedule precedence, atomic claim
  (4 concurrent claimers, 3 jobs), job+history transaction, FK
  violations, rollback on failure, invalid transitions, concurrent
  writes (8 tasks × 5 repos) and concurrent upsert of one repo.
  Re-verified: `cargo test` **91 passed, 0 failed** (core 11,
  database 54, git-engine 22, logging 4, provider-abstractions 0);
  `cargo fmt --check` pass; clippy (5 buildable crates) pass,
  0 warnings; `pnpm -r typecheck` pass; desktop build pass. `cargo
  test --workspace` still blocked on this VM by missing `gdk-3.0`
  system libraries (desktop crate only) — not claimed passed.

### Fixed

- **All JSON config files reformatted to the repo standard (2-space
  indent, LF, final newline, proper key order — `name` first in
  `package.json`, `extends` before `compilerOptions` in `tsconfig`).**
  Six files were pasted as single lines (`packages/config/package.json`,
  `packages/config/typescript/base.json`, and the four `tsconfig.json`
  files); `capabilities/default.json` was compacted the same way.
  No values changed — verified by comparing parsed JSON before/after.
  Also fixed a real bug found while doing this: `tauri.conf.json` was
  **invalid JSON** (missing the closing brace for `"bundle"`, so
  `"plugins"` was nested inside it) — it had gone unnoticed because
  the Vite build never parses it. Re-verified: `cargo fmt --check`
  pass, `pnpm -r typecheck` pass, desktop build pass.

### Changed

- **Phase 1 re-check (2026-11-18): user's 8 commits incorporated
  into `main` (merge `d3adc8c`) and all dependencies updated to
  latest stable.** Fixes to the user's edits: root `package.json`
  was invalid JSON (missing comma — fixed); `packageManager` restored
  from pnpm 12.19.0 to 12.21.0; `tauri "=2"` in `Cargo.toml` would
  have pinned exactly Tauri 2.0.0 (fixed to `"2"` → 2.12.7); `sqlx`
  0.8 → 0.9.1; TypeScript pinned at 6.0.3, not latest 7.0.2, because
  typescript-eslint 8.73.0 refuses TS 7.0 (TS 7.1 stable not released
  yet — only a beta exists); the updater the user configured is now
  actually wired in Rust (`tauri-plugin-updater` 2.14.1 +
  `Builder::new().build()`), and the missing app icons it references
  were generated. Re-verified: fmt/clippy pass (0 warnings), `cargo
  test` 36 passed / 0 failed, typecheck/lint/desktop build pass.
  Tauri binary build still blocked on this VM by missing `gdk-3.0` /
  WebViewGTK system libraries — not claimed passed.

- **Renamed from working name Gitora to RepoNyx on 2026-11-18.**
  Gitora was unusable because gitora.com is an existing Oracle Git
  product. Renamed: repo directory (`~/workspace/reponyx`), branch
  (`feature/reponyx-foundation`), Rust crates (`reponyx-*`), TS
  packages (`@reponyx/*`), Tauri product name `RepoNyx` / identifier
  `com.reponyx.app`, default storage `D:\RepoNyx\`, database
  `reponyx.sqlite`, env var `REPONYX_DATA_DIR`. Re-verified after the
  rename: `cargo fmt --check` pass, clippy (5 crates) pass, `cargo
  test` 36 passed / 0 failed, `pnpm -r typecheck` pass, desktop eslint
  pass, `pnpm --filter @reponyx/desktop build` pass. Tauri binary
  build still blocked on this Linux VM by the same missing `gdk-3.0` /
  WebViewGTK system libraries — not claimed passed.

### Added — Phase 1 (Foundation)

- Monorepo scaffold: Cargo workspace (edition 2024, resolver 2) + pnpm
  workspaces; Tauri 2.12.7 desktop app (React 19, Vite 8, Tailwind 4).
- `crates/core`: domain types and structured `RepoNyxError` error codes
  (§17) with retryability and suggested actions.
- `crates/git-engine`: Git detection (min 2.40), safe process execution
  (arg arrays, timeout, cancellation), command builders for mirror clone,
  fetch/prune/tags, `ls-remote`, `rev-parse`, `status`; path validation.
- `crates/database`: SQLx/SQLite migrations V1 (accounts, installations,
  repositories, schedules, sync_jobs, sync_history, settings, devices,
  audit_logs) with FKs, indexes, WAL.
- `crates/provider-abstractions`: `GitProvider` trait (GitHub impl in Phase 4).
- `crates/logging`: tracing init with file rotation and secret redaction.
- Desktop UI: 8-section routing; Overview shows real DB counts and real
  Git status via Tauri commands; other sections are honest Phase-1
  placeholders. Dark/light theme toggle persisted in localStorage.
- Docs: architecture (8 docs), ADRs 001–010, threat model, phase plans
  1–10, project status.

### Known limitations (Phase 1)

- No GitHub integration, no authentication, no scheduler, no background
  agent, no sync queue — planned in Phases 4–7.
- Linux Tauri build requires system WebViewGTK dev libraries; Windows
  installer / Task Scheduler / Credential Manager are Phase 7/10 items.
