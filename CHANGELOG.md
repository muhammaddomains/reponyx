# Changelog

All notable changes to RepoNyx are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed — web preview (plain browser, no Tauri backend)

- **Cryptic `invoke` error:** opening the Vite dev server in a plain
  browser showed `Cannot read properties of undefined (reading
  'invoke')` on the Accounts and Overview pages. All api-client calls
  now go through a guard that detects the missing Tauri backend and
  throws a clear message instead ("Tauri backend is unavailable —
  this page is running in a plain browser (web preview) instead of
  the RepoNyx desktop app").
- **Spinning "checking…" states:** the Overview Git check and the
  Accounts Auth Broker check never resolved in the web preview. Both
  pages now load their data sources independently, and a failed check
  resolves to an "unavailable" state with the error instead of
  spinning forever.
- **Stale page text:** the Repositories placeholder claimed Phase 4
  (the discovery backend is done — only the page is Phase 8 UI work),
  and Overview's "Next steps" claimed account connection "arrives in
  Phase 5". Both now state the actual status.

### Added — Phase 7 (partial: scheduler crate only)

- **Scheduler crate (`crates/scheduler`, `chrono-tz` 0.10.4):**
  next-run calculation, due detection, and job creation for due
  schedules, built on the Phase 2 schedules/jobs repository
  functions. No schema change was needed (no V4 migration).
  Dependencies are core + database only — proven by `cargo tree`,
  no git-engine, no provider, no queue: the crate creates job rows
  and never runs a sync.
- **Cadence:** `interval_value` in plain or JSON form. Plain: `"6"`
  (hours), `"02:30"` (daily), `"1,3,5 02:30"` (weekly, ISO weekday
  numbers 1 = Monday .. 7 = Sunday), `"90"` (custom minutes). JSON:
  `{"hours": 6}`, `{"time": "02:30"}`, `{"minutes": 90}`, plus
  optional `"weekdays"` and `"window"` (`[start, end)` in local
  time; `end <= start` crosses midnight). Unparsable values are
  `InvalidInput`, never a fake time.
- **DST-safe next runs:** a local time that does not exist
  (spring-forward gap) fires at the first valid time after the gap;
  a local time that occurs twice (fall back) fires once, at the
  first occurrence. Tested for America/New_York and Europe/London
  (2026 transitions) with America/Phoenix as the no-DST control.
- **Precedence and materialize:** repository > account > global via
  Phase 2's `effective_schedule_for`. `materialize_due` primes
  schedules with no `next_run_at`, creates one job per governed
  repository (skipping sync-disabled repos and repos with a
  pending/running job), then advances each schedule's `next_run_at`
  to its next run. Job type: `fetch` when the repo already has a
  local path, else a clone job for its sync mode.
- **Desktop (7 Tauri commands + typed api-client calls):** list
  schedules (with computed next run), create, update, delete,
  preview next runs (UTC + local display), materialize due now,
  effective schedule for a repository. No scheduler UI page yet
  (Phase 8). Desktop Rust verified by `cargo fmt` + review only
  (Tauri binary build blocked on this VM).
- **Verification (actual, 2026-11-19):** `cargo fmt --check` pass;
  clippy pass, 0 warnings; `cargo test --workspace --exclude
  reponyx-desktop` **302 passed, 0 failed** (scheduler 40 new);
  `pnpm -r typecheck` pass; desktop build + eslint pass.
- **Deliberately NOT done (blocked on Phase 6):** the sync queue,
  background agent, IPC, Task Scheduler registration, tray,
  auto-restart, agent health UI. The scheduler creates job rows
  only; nothing runs them until Phase 6 lands.

### Added — Phase 5 (Authentication)

- **Auth Broker (`apps/auth-broker`, axum 0.8.9):** a separate binary
  crate that holds the GitHub App private key exclusively — it is not
  a dependency of the desktop crate. The key loads at startup from
  `REPONYX_BROKER_PRIVATE_KEY` (PEM text) or
  `REPONYX_BROKER_PRIVATE_KEY_PATH`; the broker refuses to start
  without the App id, the key, and a setup token of ≥ 16 characters
  (`REPONYX_BROKER_SETUP_TOKEN`), and binds loopback
  (`127.0.0.1:8765`) by default. Endpoints: `GET /health`; `POST
  /setup/state` (one-time CSRF `state` + install URL); `GET
  /github/callback` (validates `state` — single-use, 10-minute TTL —
  records the installation); `POST /pairing/request`, `POST
  /pairing/confirm`, `GET /pairing/status`, `POST /pairing/revoke`;
  `POST /installations/{id}/token` (mint); `GET /installations`;
  `GET /audit`. Token minting signs an App JWT (RS256,
  `jsonwebtoken` 12.0.0, `exp` = now + 9 min) and calls `POST
  /app/installations/{id}/access_tokens`; minted tokens are cached
  per installation until 5 minutes before expiry, with
  `force_refresh` after a 401. Rate limiting is hand-rolled
  per-key token buckets (minting 30/min per device, pairing
  requests 10/min per IP, pairing confirms 20/min per IP; 429 with
  `Retry-After`). The audit log is append-only and in-memory; events
  never contain a secret (tested for tokens, codes, and key
  material in both audit output and broker logs).
- **Pairing design:** the pairing request is authenticated by the
  broker setup token (operator-supplied at startup, stored in the
  desktop's OS credential store, `X-RepoNyx-Setup-Token`, compared
  by SHA-256 hash). The code is 9 characters from an unambiguous
  alphabet (`XXX-XXX-XXX`, ~45 bits), single-use, 10-minute TTL,
  max 5 wrong-device attempts then invalidated, revocable, audited,
  device-bound. On confirm the broker issues a random 32-byte
  device token, stored server-side only as a SHA-256 hash. Broker
  state is in-memory in Phase 5 — a restart forgets installations,
  codes, and device tokens (durable broker storage is a
  deployment-phase hardening item).
- **Credential-store crate (`crates/credential-store`, `keyring`
  4.2.0):** `CredentialStore` trait (store/get/delete/exists; OS
  backends report listing as unsupported rather than pretending the
  store is empty), OS backend via `keyring`, in-memory backend for
  tests/CI, `SecretString` (zeroed on drop, redacted in `Debug`).
  PAT flow: format validation before any network call, add
  (validate → store → create account), replace (same GitHub
  identity required), remove (store entry deleted + account
  revoked, row kept), reconnect (the only path back to
  `connected`), validate (refreshes `last_validated_at`), expiry
  detection with a 7-day warning threshold. A test proves the PAT
  appears in no column of the accounts table. `PatTokenSource`
  implements the Phase 4 `CredentialSource`.
- **Migration V3 (`0003_pat_expiry.sql`):** nullable
  `pat_expires_at` on `accounts` (metadata, not a secret).
- **Phase 4 `CredentialSource` implementations
  (`crates/github-provider/src/broker.rs`):** `BrokerClient`
  (pairing + minting; secret-free, tokens passed per call) and
  `BrokerTokenSource` (mints via the broker, caches until 5 min
  before expiry, drops the cache on `invalidate()`). The trait
  gained a default no-op `invalidate()`; `GitHubClient` now
  refreshes once after a 401 and retries a single time, then fails
  fast. Mixed auth is proven by tests: PAT + broker providers both
  validate against the same mocked GitHub API through the
  `GitProvider` trait, and an end-to-end test runs a real broker
  (throwaway key) + `BrokerTokenSource` + `GitHubProvider` and
  verifies the presented JWT.
- **Desktop flows (`apps/desktop/src-tauri/src/auth.rs`, 17 Tauri
  commands):** PAT add/replace/remove/reconnect/validate, PAT
  expiry status, broker setup-token save, broker status,
  installations, pairing start/complete (creates/reconnects the
  account row, mirrors the installation locally), GitHub App
  validate, device revocation. Errors are `{ code, message }` with
  redacted messages. The Accounts page (`routes/Accounts.tsx`) is
  now real: account list with status badges and PAT expiry
  warnings, add-PAT form, validate/reconnect/replace/remove,
  broker setup-token form, installations + pairing flow.
  The desktop crate cannot be compiled on this VM (gdk blocker),
  so these commands are verified by `cargo fmt` + review, not by a
  build.
- **Private-key scan (`scripts/check-no-private-key.sh`):** checks
  desktop sources for key markers and broker-key references,
  verifies the desktop crate does not depend on the broker
  (Cargo.toml + `cargo tree`), checks for committed key files, and
  scans a built desktop binary when one exists. **Result: PASSED
  on sources + dependency tree; binary part SKIPPED** — no desktop
  binary can be built on this VM.
- **Tests: 47 new tests** (auth-broker 22: lib 7, broker integration
  15; credential-store 20: lib 6, mixed_auth 1, pat_flow 10, store
  3; github-provider broker_source 5) plus 1 updated Phase 4 test
  contract (401 refresh). Re-verified: `cargo test --workspace
  --exclude reponyx-desktop` **262 passed, 0 failed** (Phase 4
  baseline 215); `cargo fmt --check` pass; clippy pass, 0 warnings;
  `pnpm -r typecheck` pass; desktop build pass (2306 modules).
  Tauri binary build still blocked on this VM by missing `gdk-3.0`
  / WebViewGTK system libraries — not claimed passed. One transient
  failure during verification (an untouched Phase 3 git-engine
  health test, once under parallel load; passes in isolation and on
  re-run) is recorded as a flake in `ai/phase5/PLAN.md`.
- **Dependencies added (all latest stable):** `axum` 0.8.9,
  `jsonwebtoken` 12.0.0 (feature `rust_crypto`), `keyring` 4.2.0,
  `rand` 0.10.4, `sha2` 0.11.1, `zeroize` 1.9.2 (feature `derive`),
  `rsa` 0.10.0 (dev-only), `pkcs8` 0.11.0 (dev-only); `reqwest`
  0.13.5 gained the `query` feature; `tempfile` 3.27.0 moved to
  workspace dependencies.
- **Deliberately NOT done (approval gates):** no GitHub App
  published or created; no production credential created or
  modified; the Auth Broker was not deployed anywhere; no real
  PAT/token/private key was used.

### Added — Phase 4 (GitHub Integration)

- **New crate `crates/github-provider`** implementing the
  `GitProvider` trait: `validate_auth` (`GET /user`), paginated
  `discover_repositories` (`GET /user/repos`, `per_page=100`,
  `sort=full_name`), `clone_url`, `acquire_credential`,
  `capabilities()`, `provider()`. The provider talks to the GitHub
  REST API only — it never runs Git, and the engine stays
  provider-independent.
- **API client (`client.rs`, reqwest 0.13.5):** 30 s request
  timeout, `Authorization` per token kind (`Bearer` for
  installation tokens, `token` for PATs), retries with exponential
  backoff + jitter (max 3 attempts) for 5xx / network errors only.
  Error mapping: 401 → `AuthenticationExpired`, 403 revoked /
  suspended → `AuthenticationRevoked`, other 403 →
  `InsufficientPermissions`, 404 → `RepositoryNotFound`, 5xx
  exhausted → `InternalError`, transport failure →
  `NetworkUnavailable`.
- **Pagination + streaming discovery (`pagination.rs`,
  `provider.rs`):** `Link: rel="next"` parsing; the cursor is the
  next-page URL, validated to stay on the configured API base
  (a prefix-match hole that let `api.github.com.evil…` through was
  caught by a test and fixed). Missing/malformed `Link` header is
  treated as the last page, never a crash. `discover_all_stream`
  yields one message per API page over a channel, in order, so
  thousands of repos never sit in memory at once.
- **Rate-limit awareness (`rate_limit.rs`):**
  `X-RateLimit-Limit/Remaining/Reset` and `Retry-After` parsed on
  every response and exposed via `rate_limit()`; on 429 / 403-rate
  -limit the client sleeps until reset (capped at 300 s per wait,
  3 waits per request) and resumes from the same cursor; beyond
  the cap it returns `RateLimitExceeded` with the reset time.
- **Installation metadata + permission checks:** `GET
  /user/installations` and `GET
  /user/installations/{id}/repositories` (user-token endpoints
  only — `GET /app/installations` needs an App JWT, Phase 5);
  `check_repository_access` reports Admin/Write/Read/None plus
  `can_read_contents`.
- **Discovery writes to the database (`sync.rs`):** upserts via the
  Phase 2 repositories preserve local state; local repos absent
  from discovery are returned in `not_seen` and never deleted.
  `sync_installations` persists installations via a new
  `upsert_installation` in `reponyx-database`. Schema gap recorded
  in the project status: the V1 `installations` table has no
  columns for `app_id`, `app_slug`, `repository_selection`, or the
  `suspended_at` timestamp.
- **Credentials (`credentials.rs`):** pluggable `CredentialSource`
  trait (GitHub-specific token kinds) with `StaticTokenSource` for
  tests and `FailingTokenSource` for failure tests; Phase 5 must
  supply a broker-backed installation-token source and a PAT
  source. No JWT signing, no private key, no credential-store
  reads, no production credential created or modified. Token
  secrecy: `ShortLivedCredential` now has a hand-written `Debug`
  that redacts the token (the derived impl printed it); `Token`,
  `StaticTokenSource`, `GitHubClient` and `GitHubProvider` all
  redact in `Debug`.
- **Tests: 58 new github-provider tests + 1 new database test**
  (lib 26, discovery 8, errors 8, installations 4, rate_limit 4,
  redaction 6, sync 2; database installations 8 with the new
  upsert test), all against a mocked GitHub API (wiremock 0.6.5,
  dev-only) — multi-page discovery incl. an empty page,
  rate-limit back-off and resume mid-pagination, error mapping,
  pagination edge cases, installation metadata and permission
  checks, discovery-to-DB with local-state preservation and
  `not_seen` reporting, token redaction. wiremock-rs has no
  scenarios, so stateful sequences use a `SequencedResponder`
  that answers successive requests with queued responses.
  Re-verified: `cargo test --workspace --exclude reponyx-desktop`
  **215 passed, 0 failed** (core 11, database 55, git-engine 87,
  github-provider 58, logging 4, provider-abstractions 0);
  `cargo fmt --check` pass; clippy pass, 0 warnings; `pnpm -r
  typecheck` pass; desktop build pass. Tauri binary build still
  blocked on this VM by missing `gdk-3.0` / WebViewGTK system
  libraries — not claimed passed.
- **Dependencies added (all latest stable):** `reqwest` 0.13.5,
  `wiremock` 0.6.5 (dev-only).

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
