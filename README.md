# RepoNyx

**Your Git. Your Machine. Always in Sync.**

RepoNyx is a desktop application that connects multiple GitHub accounts,
discovers their accessible repositories, and synchronizes those repositories
to a central local machine — as Git mirror archives and/or working clones —
with scheduled, background synchronization independent of the desktop UI.

> **Status: Phase 1 — Foundation (in progress).** The desktop foundation
> builds and runs; repository discovery, authentication, scheduling, and the
> background agent are planned in later phases and are **not** yet
> implemented. See `ai/memory/PROJECT_STATUS.md` for the honest, current
> state. Nothing in this repo pretends to sync repositories yet.

## Features (target)

- Multiple GitHub accounts — GitHub App (primary) and fine-grained PAT (alternative)
- Automatic repository discovery (paginated, thousands of repos)
- Mirror clones (default archival mode) and working clones
- Scheduled synchronization with per-repository / per-account / global precedence
- Background agent that survives UI closure (Windows Task Scheduler first; native service later)
- Local storage layout separating accounts and owners, with collision protection
- Detailed sync logs, retry with exponential backoff, crash recovery
- Git LFS and submodule support (Phase 9)
- Provider abstraction for future GitLab / Bitbucket support

## Architecture overview

```
apps/desktop            Tauri 2 + React 19 + TypeScript + Vite + Tailwind 4
crates/core             Domain types + structured errors
crates/git-engine       Safe Git CLI execution (arg arrays, timeouts, locks)
crates/database         SQLx + SQLite, versioned migrations
crates/provider-abstractions  GitProvider trait (GitHub impl: Phase 4)
crates/logging          tracing init + secret redaction
packages/shared|ui|api-client|config  Shared TS types, UI primitives, Tauri client
docs/architecture       System, auth, git-sync, background-agent, DB, IPC, security
```

Planned (not yet created — see `docs/architecture/`): `apps/auth-broker`
(holds the GitHub App private key — **never** in the desktop binary),
`apps/background-agent`, and the `github-provider`, `scheduler`,
`sync-queue`, `credential-store`, `ipc`, `testing` crates.

## Requirements

| Tool | Minimum | Phase 1 verified |
| --- | --- | --- |
| Rust (stable) | 1.90 | 1.100.0 |
| Node.js | 20 LTS | 24.22.1 |
| pnpm | 10 | 12.21.0 |
| Git | 2.40 | 2.43.0 |
| Tauri | 2.12 | 2.12.7 (tauri-build 2.7.6) |

Windows (target platform): MSVC Build Tools + WebView2 runtime are required
for the Windows build; Linux builds need `libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`. Windows
installer, Task Scheduler integration, and Credential Manager storage are
Phase 7 / Phase 10 verification items — not claimed done here.

## Development setup

```bash
# Rust (rustup) and pnpm first, then:
git clone <repo> && cd reponyx
git checkout feature/reponyx-foundation
pnpm install
cargo test --workspace
pnpm --filter @reponyx/desktop build
cargo tauri build   # from apps/desktop; needs system WebView deps (see above)
```

## Documentation

- `AGENTS.md` / `CLAUDE.md` — agent and contributor operating rules
- `docs/architecture/` — system, authentication, git-sync, background-agent, database, IPC, security, deployment
- `docs/decisions/` — ADRs 001–010
- `docs/security/threat-model.md` — threat model and residual risks
- `ai/phase1/` … `ai/phase10/` — phase plans and status

## Roadmap

Phase 1 Foundation → 2 Database & Domain → 3 Git Engine → 4 GitHub
Integration → 5 Authentication → 6 Sync Queue → 7 Scheduler & Background
Agent → 8 Desktop Interface → 9 Advanced Features → 10 Security, QA & Release.

## License

MIT — see `LICENSE`.
