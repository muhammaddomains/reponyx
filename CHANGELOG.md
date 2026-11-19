# Changelog

All notable changes to RepoNyx are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added — Phase 3 (Git Engine)

- **Repository lock manager** (`crates/git-engine/src/locks.rs`):
  one operation at a time per local path — in-process async mutex
  per canonical path plus a cross-process advisory file lock via
  `fs2` (lock file lives next to the repo as
  `.reponyx-<name>.lock` and is never deleted; the OS releases an
  advisory lock when its holder dies, so a leftover file is never
  stale). `try_acquire` fails fast with `RepositoryLocked`,
  blocking acquire takes a timeout, the lock is released on drop
  (RAII guard).
- **Mirror clone + incremental fetch with progress reporting**
  (`mirror.rs`, `progress.rs`): `mirror_clone` (`clone --mirror
  --progress`, verifies bare + refs after) and `mirror_fetch`
  (`fetch --prune --tags --progress`, returns added/pruned ref
  diff). Progress is streamed as structured `ProgressEvent`s
  parsed from git's `--progress` stderr — only numbers git emits
  are reported, percentages are never invented.
- **Working-clone fetch + status/dirty detection + divergence**
  (`working.rs`): `working_fetch` (`fetch --all --prune --tags`
  only — never checkout/reset/clean), `working_status`
  (`status --porcelain=v1`: Conflicted > Staged > Modified >
  UntrackedOnly > Clean, with file lists), `divergence`
  (`rev-list --left-right --count` vs upstream; no upstream is
  `None`, not an error).
- **Branch/tag discovery from a mirror** (`refs.rs`):
  `list_branches` / `list_tags` / `head_sha` via `git
  for-each-ref` (NUL-separated; annotated tags carry the peeled
  commit SHA).
- **Credential helper integration** (`credentials.rs`): a
  short-lived helper script containing no secret; the credential
  travels only via `REPONYX_GIT_CREDENTIAL_*` env vars to the git
  child, `GIT_TERMINAL_PROMPT=0` so git never prompts, and the
  temp dir (0700) is deleted on drop even on failure/cancellation.
  URLs with embedded credentials are rejected, and the token never
  appears in argv, logs, or captured output.
- **LFS availability detection** (`detect.rs` + `LfsStatus` in
  `crates/core` and `packages/shared`): `detect_lfs()` /
  `lfs_status()`; version parser handles both `git-lfs version
  3.5.1` and `git-lfs/3.5.1` forms. Detection only — LFS sync
  itself is Phase 9.
- **Recovery — repository health diagnosis** (`health.rs`):
  `verify_repository` returns a structured `RepositoryDiagnosis`
  (never `Err`, never modifies anything): NotARepository,
  IncompleteClone (interrupted clone), Locked (reponyx file lock
  or `index.lock` — stale if mtime > 1 h, documented heuristic),
  Corrupt (`rev-parse`/`fsck` failure), Healthy (including an
  empty repo). Each status carries a suggested action. No
  auto-delete or auto-repair — diagnosis only.
- **Cancellation + timeouts end to end**: every long operation
  takes `SyncOptions` (timeout 300 s default, lock timeout 60 s,
  `CancellationToken`, progress channel, credential); on
  cancel/timeout the child process is killed and the lock is
  released, and the error is `OperationCancelled` /
  `OperationTimedOut`.
- **Tests: 65 new git-engine tests** (37 new lib unit tests and
  28 new integration tests; git-engine total is now 87, up from
  22): mirror clone, fetch after new commits, prune of deleted
  branches, tag preservation, lock contention, cancellation,
  corrupt-repo detection, credential-helper secrecy (fake local
  HTTP auth server), and disk-full failure injection. **Disk-full
  test:** a true ENOSPC was not feasible (a tiny tmpfs needs mount
  privileges this VM does not grant; filling the real filesystem
  would endanger the machine), so the test lowers `RLIMIT_FSIZE`
  to 1 MiB around a clone of a fixture with 3 × 1 MiB
  incompressible pseudo-random blobs over a `file://` URL. **Bugs
  found and fixed:** `try_acquire` returned success with no file
  lock when another process held it; test-fixture path collision
  (`work` dir); `--abbrev-ref` beating `--symbolic-full-name` in
  the upstream rev-parse builder. Re-verified: `cargo test
  --workspace --exclude reponyx-desktop` **156 passed, 0 failed**
  (core 11, database 54, git-engine 87, logging 4,
  provider-abstractions 0); `cargo fmt --check` pass; clippy pass,
  0 warnings; `pnpm -r typecheck` pass; desktop build pass.
  Tauri binary build still blocked on this VM by missing `gdk-3.0`
  / WebViewGTK system libraries — not claimed passed.
- **Dependencies added (all latest stable):** `fs2` 0.4.3,
  `tokio-util` 0.7.20, `libc` 0.2.191 (dev-only); `tempfile`
  3.27.0 moved to regular dependencies (credential helper).

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
