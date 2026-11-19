# Phase 1 — Foundation

- Status: **Complete except the Tauri binary build (blocked — see below)**
- Branch: `feature/reponyx-foundation`

## Objectives

Buildable desktop foundation: monorepo, Tauri 2 + React 19 app shell,
core crates, SQLite V1, logging, docs, ADRs.

## Requirements

- pnpm + Cargo workspaces, strict TypeScript, edition 2024 Rust.
- Tauri 2 desktop with real commands (`get_app_info`, `get_git_status`,
  `get_dashboard_counts`) reading the real DB/Git — zeros on fresh DB.
- 8-section routing; Overview real; other sections honest placeholders.
- `crates/core` errors, `crates/logging` redaction, `crates/git-engine`
  detection/builders/process/paths, `crates/provider-abstractions`
  trait, `crates/database` V1 migrations.
- Docs: architecture ×8, ADRs 001–010, threat model, phase plans.

## Implementation tasks

- [x] Toolchain install + version verification
- [x] Repo init on `feature/reponyx-foundation`
- [x] Root + workspace scaffolding
- [x] Rust crates (core, logging, git-engine, provider-abstractions, database)
- [x] Desktop app (Tauri + React) + shared TS packages
- [x] Docs + ADRs + threat model
- [x] Verification: fmt, clippy, cargo test, pnpm typecheck/build, Tauri build
      — fmt/clippy/tests/typecheck/lint/vite build pass (see results below);
        Tauri build BLOCKED on this VM by missing system libraries.

## Verification results (actual, 2026-11-18)

- `cargo fmt --check`: pass.
- `cargo clippy` (5 buildable crates, `--all-targets`): pass, 0 warnings.
- `cargo test` (same 5 crates): 36 passed, 0 failed — core 7, database
  migrations 3, git-engine 21 unit + 1 mirror-clone integration, logging 4.
- `pnpm -r typecheck`: pass (4 packages). Desktop `eslint`: pass.
- `pnpm --filter @reponyx/desktop build`: pass (1844 modules, 1.03 s).
- `cargo check -p reponyx-desktop`: FAIL — `gdk-sys v0.18.2`: `Package
  gdk-3.0 was not found in the pkg-config search path`. `cargo tauri
  build` not run to completion (same blocker).

## Dependencies

None (first phase). Toolchain: Rust ≥ 1.90, Node 20+, pnpm, Git ≥ 2.40.

## Acceptance criteria

- `cargo test --workspace` passes; `cargo clippy` clean (or warnings recorded).
- `pnpm --filter @reponyx/desktop build` passes; typecheck clean.
- Tauri build attempted; result recorded honestly (Linux system deps may block).
- No fake functionality; no secrets; no empty crates.

## Testing

- Core error-code tests; logging redaction tests; git command-builder
  + error-mapping tests; path-validation tests; DB migration tests;
  git-engine mirror-clone integration test on local bare fixtures.

## Known issues

- `tauri-build` is versioned independently of `tauri` (2.7.6 vs 2.12.7).
- **Linux Tauri build blocked on this VM:** missing `gdk-3.0` /
  WebViewGTK dev libraries; apt cannot install them because
  `libzstd-dev` requires stock `libzstd1` but the image pins
  `libzstd1+hatch1` ("held broken packages"). Downgrading that core
  library was not attempted. Re-run on a clean Ubuntu 24.04 or Windows.
- Bugs found + fixed during verification: infinite loop in
  `redact_secrets` Bearer redaction; `tauri-build =2.12.6` does not
  exist; `reponyx-core` referenced `sqlx` without the dependency;
  `wait_with_output()` consumes the child (cancellation reworked to
  `wait()` + piped readers).

## Completion status

Phase 1 complete except the Tauri binary build, which is blocked by the
environment issue above (not by code). All other acceptance criteria
met; results recorded in `ai/memory/PROJECT_STATUS.md`.
