# RepoNyx

**Your Git. Your Machine. Always in Sync.**

RepoNyx is a desktop application that connects multiple GitHub accounts,
discovers their accessible repositories, and synchronizes those repositories
to a central local machine — as Git mirror archives and/or working clones —
with scheduled, background synchronization independent of the desktop UI.

> **Status: Phase 10 — Security, QA & Release (in progress).** Phases 1–9
> are complete and pushed: database + domain, Git engine, GitHub
> integration, authentication (GitHub App + fine-grained PAT), sync queue
> with crash recovery, scheduler + background agent, the full desktop UI,
> and advanced features (LFS, submodules, Both-mode, storage management,
> diagnostics export + DB backup, config import/export, server-side
> pagination measured at 1,000 repos / 10,000 jobs). See
> `ai/memory/PROJECT_STATUS.md` for the honest, current state, including
> what is not yet verified on a real Windows machine.

## Features

- Multiple GitHub accounts — GitHub App (primary) and fine-grained PAT (alternative)
- Automatic repository discovery (paginated, thousands of repos)
- Mirror clones (default archival mode) and working clones, or Both
- Scheduled synchronization with per-repository / per-account / global precedence
- Background agent that runs as a separate process (started from the
  desktop); start-at-logon via the Agent page (Windows Task Scheduler
  ONLOGON, systemd user unit, or macOS LaunchAgent)
- Local storage layout separating accounts and owners, with collision protection
- Detailed sync logs, retry with exponential backoff, crash recovery
- Git LFS and submodule support, including Both-mode
- Diagnostics export (redacted) + consistent DB backups with validated restore
- Config import/export (settings, repo selections, schedules) as versioned JSON
- Provider abstraction for future GitLab / Bitbucket support

## Using RepoNyx

1. **Install** the Windows build (NSIS installer from a GitHub Release,
   once releases are published) or build from source below.
2. **Add an account** on the Accounts page: pair via the GitHub App
   (the app is installed on GitHub, the desktop pairs with a short-lived
   code) or add a fine-grained PAT (Contents: Read, Metadata: Read).
   PATs live only in the OS credential store (Windows Credential
   Manager) — never in the database, logs, or files.
3. **Discover** repositories, pick a sync mode per repository
   (Mirror / Working / Both), and set schedules (per-repo, per-account,
   or global) on the Schedules page.
4. **Watch** progress on the Synchronization page (it polls every 5 s;
   there are no live progress bars yet) and history on the Activity page.

**Where your data lives (Windows):** `%APPDATA%\RepoNyx` — database at
`data\reponyx.sqlite`, logs at `logs\reponyx.log.<date>`, archives under
the storage root (default: the data directory). Diagnostics exports land
in `diagnostics/`, backups in `backups/`; both are managed from
Settings → Diagnostics & backup.

**Back up before upgrading:** Settings → Diagnostics & backup → Back up
database. A restore is validated first and staged for the next startup;
the live database is kept as a `pre-restore-<timestamp>` fallback.

## Security notes (read before trusting the archive)

- **Full-disk encryption is the archive's protection.** The archive
  (all repos, history, LFS objects) is stored unencrypted on disk.
  Use BitLocker on Windows; without it, a stolen disk exposes
  everything. See `docs/security/threat-model.md`.
- **Working clones can contain hooks.** A synced repository's hooks
  never execute inside RepoNyx (mirrors are bare; working clones are
  never auto-executed), but opening a working clone in another tool
  may run its hooks. Inspect unfamiliar repositories before opening
  them elsewhere.
- **Broker state survives restarts.** The Auth Broker persists
  installations, pairing codes, device tokens, and its audit log
  in SQLite (`broker.sqlite` under the broker data directory);
  only the short-lived token cache and the rate-limiter buckets
  are still in-memory. Audit events older than 90 days are pruned
  at startup.
- **Secrets never leave their stores.** The GitHub App private key
  exists only in the Auth Broker; PATs only in the OS credential
  store; the desktop↔agent IPC uses a per-install shared secret from
  the credential store. Nothing secret goes into the database, logs,
  URLs, or the repository — CI fails the build if a private key is
  found in the tree.

## Architecture overview

```
apps/desktop            Tauri 2 + React 19 + TypeScript + Vite + Tailwind 4
apps/auth-broker        GitHub App token minting + pairing (holds the private key)
apps/background-agent   Scheduler + queue worker, IPC server
crates/core             Domain types + structured errors
crates/git-engine       Safe Git CLI execution (arg arrays, timeouts, locks)
crates/database         SQLx + SQLite, versioned migrations (V1–V4)
crates/github-provider  GitHub API client, pagination, discovery
crates/provider-abstractions  GitProvider trait (GitHub impl)
crates/scheduler        Schedule evaluation + job materialization
crates/sync-queue       Claim/lease/retry/rate-limit queue
crates/ipc              Desktop↔agent protocol (token-authenticated)
crates/credential-store OS credential store access
crates/logging          tracing init + secret redaction
packages/shared|ui|api-client|config  Shared TS types, UI primitives, Tauri client
docs/architecture       System, auth, git-sync, background-agent, DB, IPC, security
```

## Requirements

| Tool | Minimum | Verified |
| --- | --- | --- |
| Rust (stable) | 1.100.0 | 1.100.0 |
| Node.js | 26 | 26.13.1 |
| pnpm | 12.21.0 | 12.21.0 |
| Git | 2.40 | 2.43.0 |
| Git LFS | 3.8 | 3.8.1 |
| Tauri | 2.12 | 2.12.7 (tauri-build 2.7.6) |

Windows (target platform): MSVC Build Tools + WebView2 runtime are required
for the Windows build; Linux builds need `libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`. Windows
installer, Task Scheduler integration, and Credential Manager storage are
Phase 10 verification items — the app has run on Windows, but the
installer and the scheduled-agent path have not been verified end to end
on a real Windows machine yet.

## Development setup

```bash
# Rust (rustup) and pnpm first, then:
git clone <repo> && cd reponyx
git checkout main
pnpm install
cargo test --workspace
pnpm --filter @reponyx/desktop build
cargo tauri build   # from apps/desktop; needs system WebView deps (see above)
```

## Documentation

- `AGENTS.md` / `CLAUDE.md` — agent and contributor operating rules
- `docs/architecture/` — system, authentication, git-sync, background-agent, database, IPC, security, deployment
- `docs/decisions/` — ADRs 001–010
- `docs/security/threat-model.md` — threat model, Phase 10 review, and open findings
- `ai/phase1/` … `ai/phase10/` — phase plans and status

## Roadmap

Phase 1 Foundation → 2 Database & Domain → 3 Git Engine → 4 GitHub
Integration → 5 Authentication → 6 Sync Queue → 7 Scheduler & Background
Agent → 8 Desktop Interface → 9 Advanced Features → 10 Security, QA & Release.

## License

MIT — see `LICENSE`.
