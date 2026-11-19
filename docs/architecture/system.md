# System Architecture

Status: Accepted (Phase 1) · Last updated: 2026-11-18

## Overview

RepoNyx is a Windows-first desktop application (Tauri 2 + React 19) that
centralizes synchronization of GitHub repositories to one local machine.
One central desktop installation, multiple GitHub accounts, multiple
authentication methods, one unified sync engine, persistent background
execution, complete local Git archives.

## Component map

```text
┌─────────────────────────────────────────────────────────────────┐
│ apps/desktop (Tauri 2)                                          │
│  React 19 UI ──invoke──► Rust commands ──► crates/*             │
│  (Overview, Accounts, Repositories, Sync, Schedules,            │
│   Activity, Storage, Settings)                                  │
└──────────────┬──────────────────────────────┬───────────────────┘
               │                              │
┌──────────────▼──────────────┐   ┌────────────▼──────────────────┐
│ apps/background-agent       │   │ apps/auth-broker (Phase 5)    │
│ (Phase 7) scheduler+queue,  │   │ GitHub App private key lives  │
│ runs without the UI         │   │ HERE ONLY. Issues short-lived │
│                             │   │ installation tokens.          │
└──────────────┬──────────────┘   └───────────────────────────────┘
               │ IPC (named pipes, Phase 7)
┌──────────────▼──────────────────────────────────────────────────┐
│ crates/                                                         │
│  core            domain types, structured errors (§17)          │
│  git-engine      safe Git CLI: detection, builders, execution   │
│  database        SQLx/SQLite, migrations, WAL                   │
│  provider-abstractions  GitProvider trait                       │
│  logging         tracing + secret redaction                     │
│  (planned) github-provider, scheduler, sync-queue,              │
│  credential-store, ipc, testing                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Data flow (sync)

1. Provider discovers repositories (paginated GitHub API).
2. Metadata lands in SQLite (`repositories`).
3. Scheduler (persistent, SQLite-backed) creates `sync_jobs`.
4. Queue workers (max 5 concurrent, configurable to 10) execute jobs.
5. `git-engine` runs Git CLI via argument arrays with timeout +
   cancellation; credentials injected via a short-lived helper, never
   in URLs or argv.
6. Results + history persisted; UI observes via Tauri events / polling.

## Key invariants

- The sync engine is provider-independent; GitHub is one `GitProvider`.
- The UI never keeps scheduled sync alive; the background agent does.
- No secrets in SQLite, logs, URLs, or process arguments.
- Local repositories are never auto-deleted on remote deletion/revocation.
- Timestamps stored in UTC; displayed in the user's timezone.

## Technology choices

| Layer | Choice | Why |
| --- | --- | --- |
| Desktop | Tauri 2.12.7 | Small, secure, Rust backend, WebView UI |
| Frontend | React 19, Vite 8, Tailwind 4 | Current stable, fast, typed |
| Backend | Rust 1.100, Tokio, SQLx/SQLite | Async, safe, single-file DB |
| Git | Installed Git CLI (≥ 2.40) | Full fidelity: mirror, LFS, submodules |
| Auth | GitHub App + fine-grained PAT | Least privilege, both via one trait |

## Deployment

Desktop: Tauri bundle (NSIS on Windows, .dmg/.deb/.AppImage later).
Auth Broker: separately hosted service (Phase 5), never shipped in the
desktop binary. Background agent: Windows Task Scheduler at login
(Phase 7), native Windows Service as a later option.

## Deviations from the target structure

- `tauri-build` is versioned independently of `tauri` (2.7.6 vs 2.12.7);
  both are pinned exact. See ADR-001.
- `apps/auth-broker` and `apps/background-agent` are documented, not
  scaffolded, in Phase 1 (no placeholder apps that appear implemented).
