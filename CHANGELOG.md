# Changelog

All notable changes to RepoNyx are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

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
