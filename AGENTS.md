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
2026-11-18); Phase 7 — Scheduler and Background Agent (partial:
scheduler crate only, 2026-11-19 — background agent, IPC, and Task
Scheduler work blocked on Phase 6)**. See `ai/memory/PROJECT_STATUS.md` for the
honest current state.

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
- `crates/{core,logging,git-engine,provider-abstractions,github-provider,database,credential-store,scheduler}` — Rust libs
- `apps/auth-broker` — Auth Broker (holds the GitHub App private key; never a desktop dependency)
- `packages/{shared,ui,api-client,config}` — shared TS
- `docs/architecture`, `docs/decisions` (ADRs 001–010), `docs/security`
- `ai/phaseN/PLAN.md`, `ai/memory/PROJECT_STATUS.md`

Planned, not yet created: `apps/background-agent`,
`crates/{sync-queue,ipc,testing}`.

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
