# AGENTS.md — RepoNyx repository instructions

## What this is

RepoNyx: Tauri 2 + React 19 desktop app that syncs GitHub repositories
(GitHub App primary, fine-grained PAT alternative) to a local machine as
Git mirror archives / working clones, with a background agent. Windows-first,
portable to macOS/Linux. Current phase: **Phase 1 — Foundation (complete,
merged to `main` 2026-11-18); Phase 2 — Core Database and Domain
(complete 2026-11-18); Phase 3 — Git Engine (complete
2026-11-18); Phase 4 — GitHub Integration (complete
2026-11-18); Phase 5 — Authentication (complete
2026-11-18); Phase 6 — Sync Queue (code landed on `main` 2026-11-21,
fixed + verified the same day — see `ai/memory/PROJECT_STATUS.md`,
"Phase 6 / Phase 7 fixes + re-verification"); Phase 7 —
Scheduler and Background Agent (scheduler crate verified complete
2026-11-19; agent, IPC, tray, and Agent page code landed 2026-11-21,
fixed + verified the same day); Phase 8 — Desktop UI (started
2026-11-21: all six pages built + pushed — Schedules, Repositories,
Synchronization, Activity, Storage, Settings — see
`ai/memory/PROJECT_STATUS.md`, "Phase 8 — Settings page"). Phase 9
— Advanced Features (started 2026-11-21: LFS sync + verification
built — engine `lfs` module + runner auto-enqueue + real git-lfs
fixture tests, `cargo test` 352 passed / 0 failed; submodule sync
+ Both-mode completion built the same day — engine `submodule`
module + runner auto-enqueue + 9 fixture tests, `cargo test` 373
passed / 0 failed; storage management + orphan detection built
the same day — engine scan + safe delete, DB `set_local_path`,
3 Tauri commands, Storage page sections + actions, `cargo test`
378 passed / 0 failed; diagnostics/backup built the same day —
secret scan + log tail, `VACUUM INTO` backup + validation, 6
Tauri commands, Settings page section, restore staged for the
next startup, `cargo test` 395 passed / 0 failed; import/export,
performance still open). See
`ai/memory/PROJECT_STATUS.md` for the honest current state.

## Branch & safety

- Work on `main` (current branch, per the user's 2026-11-18 instruction
  "move to main branch. and use it"). Never force-push / rewrite history.
  The old `feature/reponyx-foundation` branch was deleted on GitHub
  2026-11-18 after merging; keep the local copy only as the record of
  the original Phase 1 + rename commits (`49a44bf`, `977be27`) — do not
  delete it locally and do not recreate it remotely.
- Never commit secrets, private keys, tokens, or build artifacts.
  `.gitignore` covers `.env`, `*.pem`, `*.key`, `*.sqlite`.
- No GitHub App creation, no credentials, no deployment, no publishing
  without explicit approval.

## Commands

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --check && cargo clippy --workspace --all-targets && cargo test --workspace
pnpm install
pnpm -r typecheck
pnpm --filter @reponyx/desktop build        # vite build
cargo tauri build                          # apps/desktop; needs WebViewGTK dev libs on Linux
```

## Layout

- `apps/desktop` — React frontend + Tauri backend (`src-tauri`)
- `crates/{core,logging,git-engine,provider-abstractions,github-provider,database,credential-store,scheduler,sync-queue,ipc}` — Rust libs
  (all workspace members since 2026-11-21; `sync-queue` and `ipc`
  were added by the Phase 6/7 fixes)
- `apps/auth-broker` — Auth Broker (holds the GitHub App private key; never a desktop dependency)
- `apps/background-agent` — background agent binary (landed 2026-11-21,
  workspace member since the Phase 6/7 fixes the same day; compiles;
  wired into the desktop via `agent.rs` / `tray.rs` / `/agent` route)
- `packages/{shared,ui,api-client,config}` — shared TS
- `docs/architecture`, `docs/decisions` (ADRs 001–010), `docs/security`
- `ai/phaseN/PLAN.md`, `ai/memory/PROJECT_STATUS.md`

Planned, not yet created: `crates/testing`.

## Hard rules

- Git via argument arrays only; no shell; timeouts + cancellation; redact
  secrets from logs/process output; never put tokens in URLs or argv.
- Never trust repo names as paths — validate in `git-engine` path module.
- Never auto-delete local repos. Never fake sync results or UI actions.
- PATs/tokens: OS credential store only, never SQLite/plaintext/logs.
- GitHub App private key: Auth Broker only, never in the desktop binary.
- Report actual test counts; never claim a pass you did not run.
- Keep modules small; shared types in `packages/shared` / `crates/core`;
  no duplicated business logic; no circular dependencies.
- Update `CHANGELOG.md` and `ai/memory/PROJECT_STATUS.md` at phase end.
