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
`ai/memory/PROJECT_STATUS.md`, "Phase 8 — Settings page"; complete
2026-11-23 with notifications, frontend tests, true list
virtualization, tray-menu polish, and live progress bars (IPC
forwarding) — see "Phase 8 — Completion"). Phase 9
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
next startup, `cargo test` 395 passed / 0 failed; import/export
built 2026-11-22 — versioned JSON config file, native file
dialogs, 2 Tauri commands, Settings page section, `cargo test`
402 passed / 0 failed; performance pass built 2026-11-22 —
server-side pagination for repositories + jobs, bounded
repository-name lookups, measured before/after at 1,000 repos /
10,000 jobs, `cargo test` 404 passed / 0 failed; Phase 9
complete). Phase 10 — Security, QA and Release (started
2026-11-22: security audit first pass — threat-model review
with 3 findings recorded, dependency audits run for the first
time, V1→V4 + V3→V4 migration upgrade tests, failure-recovery
inventory, README rewritten — `cargo test` 406 passed /
0 failed; durable broker storage (F1) built 2026-11-22 —
SQLite store under `REPONYX_BROKER_DATA_DIR`, restart-safe
pairing + audit — `cargo test` 412 passed / 0 failed; key-rotation
runbook (F3) done 2026-11-23; weekly audit found + fixed
RUSTSEC-2026-0594 (cookie 0.18.2 → 0.18.3) the same day; Windows
verification items still open — see
`ai/memory/PROJECT_STATUS.md`, "Phase 10 — Security audit,
first pass"). See
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

Windows-only code (e.g. `crates/ipc` pipe security) cannot be
checked by the Linux build or by CI's `cargo check` on the
desktop crate alone. Cross-check it with the MinGW GNU target
(installed 2026-11-25: `rustup target add --toolchain 1.100.0
x86_64-pc-windows-gnu` + `apt install gcc-mingw-w64-x86-64`):

```bash
CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc cargo check -p reponyx-ipc --target x86_64-pc-windows-gnu
```

The MSVC target (`x86_64-pc-windows-msvc`) does NOT work on this
VM — C build scripts (aws-lc-sys, libsqlite3-sys) need MSVC.
The GNU check caught real errors the Linux build never saw
(wrong `windows-sys` module for `NO_INHERITANCE`, unexported
consts). Run it (and clippy with the same target) before pushing
any change to Windows-only code.

The desktop crate itself can also be checked with the GNU target
(proven 2026-11-27, W3) — `tauri-build` needs the two bundled
binaries to exist at `target/release/reponyx-agent.exe` and
`target/release/reponyx-auth-broker.exe`, so build them first and
copy them there (they are gitignored build artifacts; delete the
copies afterwards so a later native build is not confused by
Windows PE files):

```bash
CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc cargo build -p reponyx-background-agent -p reponyx-auth-broker --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/reponyx-agent.exe target/x86_64-pc-windows-gnu/release/reponyx-auth-broker.exe target/release/
CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc cargo check -p reponyx-desktop --target x86_64-pc-windows-gnu
```

This caught 4 real W3 errors the Linux build never saw
(`set_setting`/`delete_setting` return values, the missing
`CommandExt` import for `creation_flags`, `focus()` →
`set_focus()`). Clippy on the desktop crate with this target
currently shows 11 pre-existing findings (10× `collapsible_if`
in agent/auth/broker/diagnostics/lib + `too_many_arguments` on
`create_schedule_command`) — none of them is gated by CI (the
windows-build job runs `cargo check` only), so they are left for
a cleanup pass, not fixed drive-by.

## Layout

- `apps/desktop` — React frontend + Tauri backend (`src-tauri`)
- `crates/{core,logging,git-engine,provider-abstractions,github-provider,database,credential-store,scheduler,sync-queue,ipc}` — Rust libs
  (all workspace members since 2026-11-21; `sync-queue` and `ipc`
  were added by the Phase 6/7 fixes)
- `apps/auth-broker` — Auth Broker (holds the GitHub App private key; never a desktop dependency). Reads a repo-root `.env` file at startup (dotenvy); real OS env vars win over the file.
- `apps/background-agent` — background agent binary (landed 2026-11-21,
  workspace member since the Phase 6/7 fixes the same day; compiles;
  wired into the desktop via `agent.rs` / `tray.rs` / `/agent` route)
- `packages/{shared,ui,api-client,config}` — shared TS
- `docs/architecture`, `docs/decisions` (ADRs 001–010), `docs/security`
- `ai/phaseN/PLAN.md`, `ai/memory/PROJECT_STATUS.md`

Planned, not yet created: `crates/testing`.

## Hard rules

- Tauri 2 commands take **camelCase** argument names by default
  (`tauri-macros` `ArgumentCase::Camel`). The frontend must send
  camelCase keys in every `invoke` call — never the Rust
  snake_case parameter names (proven 2026-11-23: the
  snake_case keys broke every multi-word-arg command on the
  first Windows run). Response DTOs are camelCase too.
- Never modify an applied SQL migration — sqlx checksums the
  whole file including comments (proven 2026-11-23: the
  `-- Gitora V1` → `-- RepoNyx V1` comment rename in
  `0001_init.sql` broke the user's existing database).
  Schema changes always go in a new migration file.
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
