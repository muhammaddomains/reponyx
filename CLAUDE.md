# CLAUDE.md — RepoNyx project guidance

## Project

RepoNyx — "Your Git. Your Machine. Always in Sync." Tauri 2 desktop app
(React 19, TypeScript strict, Vite 8, Tailwind 4, shadcn/ui-style
primitives, Lucide icons) + Rust backend (Tokio, SQLx/SQLite, tracing,
Git CLI). Windows-first, portable to macOS/Linux. Phase 1 — Foundation.

## Development commands

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # rustup
pnpm install
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets
pnpm -r typecheck
pnpm --filter @reponyx/desktop build
cd apps/desktop && cargo tauri build   # needs WebViewGTK dev libs on Linux
```

## Package responsibilities

| Package | Responsibility |
| --- | --- |
| `apps/desktop` | React UI (8 sections) + Tauri commands in `src-tauri` |
| `crates/core` | Domain types, `RepoNyxError` codes (§17) |
| `crates/git-engine` | Safe Git CLI execution, command builders, locks |
| `crates/database` | SQLx/SQLite, migrations in `migrations/sqlite/` |
| `crates/provider-abstractions` | `GitProvider` trait; GitHub impl = Phase 4 |
| `crates/logging` | tracing + secret redaction |
| `packages/shared` | TS types + zod schemas mirroring Rust domain |
| `packages/ui` | Button/Card/Badge primitives actually used by desktop |
| `packages/api-client` | Typed Tauri `invoke` wrapper |
| `packages/config` | Shared tsconfig / eslint base |

## Coding conventions

- Rust edition 2024; `Result<T, RepoNyxError>`; structured errors carry
  stable code, message, retryable flag, suggested action.
- Git commands: argument arrays, `tokio::process`, timeout, cancellation
  token, secret redaction. No shell.
- TS strict; domain types live in `packages/shared`, never duplicated.
- SQLite: migrations only, FKs on, WAL, UTC timestamps, indexes on query paths.

## Testing rules

- Unit tests for command construction, error mapping, scheduler math,
  path validation, credential lifecycle. Integration tests use local Git
  fixtures (`git init --bare`) and mocked GitHub APIs — never real GitHub
  credentials for unit tests.
- Never report tests as passing without running them; record actual counts
  in `ai/memory/PROJECT_STATUS.md`.

## Security requirements

- No secrets in code, logs, URLs, or argv. PATs in OS credential store only.
- GitHub App private key in Auth Broker only (Phase 5) — never desktop.
- Validate all path components; never auto-delete local repositories.
- See `docs/security/threat-model.md` and ADRs 001–010.

## Important constraints

- Do not implement Phases 2–10 in the Phase 1 change. Do not create empty
  crates/packages. Do not create buttons that do nothing or fake sync data.
- Do not publish a GitHub App, create/modify production credentials,
  deploy the Auth Broker, purchase services, delete user data, merge to
  `main`, or publish a release without explicit approval.
