# Changelog

All notable changes to RepoNyx are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed — CI run #34 failure (2026-11-21)

- `rust` job: `corrupt_object_is_caught_by_fsck` (git-engine
  health test) failed on the runner with `PermissionDenied` at
  `health.rs:150` — the test overwrote a git object file, but git
  stores objects read-only (0444), so the write fails with EACCES
  for a non-root user. This VM runs as root, so the bug never
  showed here. The test now removes the object file first
  (deleting needs write permission on the directory, not the
  file) and writes the corrupted bytes back under the same name.
  Root cause was proven by running the test binary as user
  `nobody` (like the runner): it failed with the exact CI panic
  before the fix and passes after it. Note: run #34's annotations
  list only the `rust` error — the `windows-build` job passed,
  so the run #32 desktop-compile fix is confirmed working in CI.

### Added — Phase 8 Storage page (2026-11-21)

- Storage page (`/storage`): storage root (setting or default
  data dir) with an Open folder button, volume usage bar
  (used/total/free via `fs2`), breakdown cards (mirrors, database
  file, logs, stored total), and a per-repository list sorted by
  size with account name, last synced, local path, and Open folder.
  Sizes are measured on demand (Refresh button, no polling — a
  large archive takes a moment to walk). New `storage` module in
  `reponyx-git-engine` (`dir_size`: recursive, never follows
  symlinks, missing paths are 0; `disk_usage` via `fs2`) with 5
  unit tests, and two new Tauri commands in
  `apps/desktop/src-tauri/src/storage.rs`. Read-only in v1: no
  delete action — deleting a local archive is destructive and
  needs its own confirm flow.

### Fixed — CI run #32 failures (2026-11-21)

- `rust` job: `cancel_mirror_clone_leaves_diagnosable_destination`
  (git-engine cancellation test) failed on the runner with
  diagnosis `Corrupt` after a cancelled clone. The 300 ms cancel
  can land while git is still writing its skeleton, so a torn
  metadata state is a legitimate outcome — the test now also
  accepts `Corrupt` as a diagnosable post-cancellation state. The
  real invariant is unchanged: the destination must never claim to
  hold the remote's commits (no HEAD). Test-only change.
- `windows-build` job: `cargo check -p reponyx-desktop` failed
  with two E0308 type errors in
  `apps/desktop/src-tauri/src/synchronization.rs` — `SyncJobDto`
  declares `priority`/`attempt_count` as `i64` while the domain
  `SyncJob` has them as `i32`. Fixed with lossless `as i64` casts
  at the mapping site. This was the first real desktop-crate
  compile error caught by CI; the desktop crate still cannot be
  compiled on the Linux VM (gdk blocker).

### Changed — GitHub Actions pins updated to latest majors (2026-11-21)

- `actions/checkout` v4 → v7, `actions/setup-node` v4 → v7,
  `pnpm/action-setup` v4 → v6 across all workflows (18 pins, 6
  files). Correction: an earlier CHANGELOG entry claimed these
  versions did not exist — that was wrong; all three were verified
  to exist (setup-node v7.1.0, pnpm/action-setup v6.1.1, checkout
  v7 via Dependabot PR #2). `codeql-action` stays at v3: v4 could
  not be verified (GitHub API rate limit) and v3 works — Dependabot
  will propose the bump if v4 exists. Note: Dependabot PR #2
  (checkout 4 → 7) is now superseded by this change and can be
  closed.

### Added — More GitHub workflows tailored to RepoNyx (2026-11-21)

- **`lint.yml`** — Markdown trailing-whitespace check, workflow
  YAML parse check, and the private-key scan
  (`scripts/check-no-private-key.sh`). No frontmatter / link /
  docs-status steps: RepoNyx has no such scripts, so they were
  left out rather than ported as dead steps.
- **`scheduled-maintenance.yml`** — weekly (Monday 07:00 UTC)
  dependency audits: `cargo audit` (RustSec) and `pnpm audit`.
  The template's stale-skill and upstream-monitoring jobs have no
  RepoNyx equivalent and were dropped.
- **`security-audit.yml`** — private-key scan + Semgrep SAST
  (`p/rust` over the crates and Rust binaries, `p/typescript` over
  the desktop frontend and shared packages) on every push/PR and
  weekly. Verified locally before committing: Semgrep 1.186.0 ran
  85 rules over 125 files — 1 finding (`current_exe` in
  `agent.rs`), a false positive for this use (path hint for the
  bundled agent binary, not a trust decision), suppressed with a
  `nosemgrep` comment carrying the justification; re-run is
  0 findings. `pnpm audit` also run locally: no known
  vulnerabilities found.
- The uploaded `labeler.yml` was NOT changed: it is the old
  wrong-paths version (`src/**`, `src-tauri/**` at the repo
  root); the repo's tailored `labeler.yml` (already pushed) is
  correct and covers it.

### Fixed — GitHub workflows and templates tailored to RepoNyx (2026-11-21)

- **`.github` was copy-pasted from another project** (DesktopDashboards):
  `dependabot.yml` and `labeler.yml` were comments only (no config at
  all), `ci.yml` and `codeql.yml` had no jobs at all, `release.yml`
  named the wrong app/binary, and the PR template listed test scripts
  that do not exist. All 12 files are now written for RepoNyx:
  - `ci.yml` — real CI: Rust job (fmt, clippy 0 warnings, tests
    excl. desktop), frontend job (Node 26, pnpm 12.21.0, typecheck,
    desktop build, lint), and a Windows job that compiles the
    desktop crate (`cargo check -p reponyx-desktop`) — the first
    real compile check for the desktop wiring.
  - `codeql.yml` — real CodeQL scan (JavaScript/TypeScript only;
    Rust is covered by clippy + tests).
  - `release.yml` — tag-triggered draft releases for RepoNyx
    (Node 26, `projectPath: apps/desktop`, correct
    `reponyx-desktop.exe`, portable ZIP). Needs
    `TAURI_SIGNING_PRIVATE_KEY` (+ password) set in repo secrets
    before the first release, for the updater's signed artifacts.
  - `dependabot.yml` — real weekly config (npm/pnpm, cargo,
    github-actions). `labeler.yml` — real path-based labels
    (frontend/backend/agent/broker/workflows/documentation/tests).
  - Issue templates fixed (correct log path
    `%APPDATA%\RepoNyx\logs\reponyx.log.<date>`, account type,
    sync mode) and the PR template rewritten with the real
    verification commands and the RepoNyx security checklist.
  - `rust.yml` deleted — its checks now live in `ci.yml`; two
    workflows running the same checks was duplication.
  - All YAML validated by parsing; action pins use known-valid
    versions (`checkout@v4`, `setup-node@v4`,
    `pnpm/action-setup@v4`, `labeler@v7` — verified to exist —
    `tauri-action@v0`, `codeql-action@v3`).

### Added — Phase 8 Activity page (2026-11-21, verified)

- **Activity page** (`apps/desktop/src/routes/Activity.tsx`,
  wired to `/activity`): the sync-history record — newest first,
  repository + result filters, operation label, result badge,
  started/finished times, duration, bytes received, 10-second
  auto-refresh. New `list_recent_history` database query (limit
  clamped 1..=500) with an integration test, and two new Tauri
  commands (`list_recent_history_command`,
  `list_history_for_repository_command`). History is append-only;
  the page never edits it. Latest 100 entries shown.

### Added — Phase 8 Synchronization page (2026-11-21, verified)

- **Synchronization page**
  (`apps/desktop/src/routes/Synchronization.tsx`, wired to
  `/synchronization`): live queue with agent health + pause/resume,
  per-status counts, job list (Active filter, per-status filters),
  cancel (immediate for pending, requested for running), retry now
  via re-enqueue, 5-second auto-refresh. New Tauri commands in
  `apps/desktop/src-tauri/src/synchronization.rs`. No live progress
  bars yet — IPC does not forward the queue's progress events to the
  desktop.

### Added — Phase 8 Repositories page (2026-11-21, verified)

- **Repositories page** (`apps/desktop/src/routes/Repositories.tsx`,
  wired to `/repositories`): list with account/status/sync filters
  and debounced search; discover from GitHub per account (upsert,
  unseen rows reported, never deleted); sync now via the agent;
  pause/resume; sync-mode change; bulk select + bulk actions; open
  folder / open on GitHub; remove (row only, local files kept).
  New Tauri commands in `apps/desktop/src-tauri/src/repositories.rs`
  (+ `reponyx-sync-queue` as a desktop dependency, reusing the
  agent's credential resolver). List shows 100 rows at a time with
  "Show more" instead of true virtualization (deferred).

### Added — Phase 8 Schedules page (2026-11-21, verified)

- **Schedules page** (`apps/desktop/src/routes/Schedules.tsx`,
  wired to `/schedules`): lists schedules with cadence, timezone,
  next run and enabled state; create (global / account / repository
  scope), enable/disable, inline edit, delete with confirm,
  per-schedule next-5-runs preview, "Run due schedules now" with the
  materialize report, and an effective-schedule lookup by repository
  id. Built on the Phase 7 scheduler backend with no backend
  changes. Repository scope needs a typed repository id until the
  Repositories page exists.

### Added — Live end-to-end run (2026-11-21, on this VM)

- **New E2E test** `crates/sync-queue/tests/e2e_live_chain.rs`:
  a due schedule creates a `MirrorClone` job, the worker pool runs
  it with the real `GitJobRunner`, and a real mirror clone lands on
  disk with the remote's HEAD and refs, with `local_path` set and a
  history row recorded. No network, no real credentials (local bare
  repo as the remote, static fake PAT).
- **Real agent + broker binaries run live:** the agent completed a
  real `mirror_clone` (~50 ms) from a due schedule against a local
  bare repo, with IPC listening and graceful shutdown; the broker
  served `/health` and correctly refused pairing for an unrecorded
  installation (401 on a wrong setup token).

### Fixed — Live end-to-end run (2026-11-21, verified)

- **`storage.root` default doubled `repositories/`:** the agent
  seeded `storage.root` to `<data_dir>/repositories` while the
  repository layout appends `repositories/github/...` under the
  product root, so mirrors landed at
  `<data>/repositories/repositories/github/...`. The default is now
  `<data_dir>` itself; re-verified live with a fresh data dir
  (mirror at `<data>/repositories/github/...`, HEAD identical to
  the remote).
- **`is_unique_violation` never matched (found in the live-run
  review, fixed 2026-11-21):** `map_error` rewrites a UNIQUE
  violation's message to a friendly "already exists" string and puts
  the raw SQLite text in `details`, but the helper only checked
  `message` — so the executor's and recovery's "already queued"
  tolerance could never trigger under contention. The helper now
  checks `details` too (same pattern as `is_locked`), with a
  regression test that fails without the fix and passes with it.

### Fixed — Phase 6 + Phase 7 integration (2026-11-21, verified)

- **Workspace membership:** `crates/sync-queue`, `crates/ipc`, and
  `apps/background-agent` are now workspace members (`Cargo.lock`
  regenerated); the new crates build and their tests run for the
  first time. Fixed the rand 0.10 API breaks and the bogus
  `tokio-util` `sync` feature in the process.
- **Retry budget binds:** transient failures now carry
  `attempt_count` across retry rows (`create_retry_job`) and the
  exponential backoff with full jitter is enforced on the job's
  `available_at` (new V4 migration `0004_queue_fields.sql`), so
  retries stop at `max_attempts` instead of looping forever.
- **Leases, heartbeats, cancel:** `claim_next_job` takes a worker
  id + lease; heartbeats renew only for the owning worker; crash
  recovery is lease-based; IPC `CancelJob` is real (pending jobs
  cancel, running jobs get a cooperative cancel request).
- **Rate limiting + priority aging:** in-memory token buckets
  (global + per-account); refused claims defer the job instead of
  failing it; claim ordering adds up to +3 effective priority after
  3 h waiting.
- **JobProgress emission:** the git runner forwards git progress
  events and stage events over the queue's broadcast channel; the
  working-clone path now honours the job cancellation token.
- **Agent:** GitHub App accounts now work (broker-backed resolver),
  paused accounts are refused, `storage.root` is provisioned on
  start, `materialize_due` arity fixed.
- **Desktop wiring:** agent/tray modules declared, 9 agent commands
  registered, `/agent` route + sidebar entry, `AgentHealth` /
  `AgentStatus` types and all nine api-client agent functions added
  — `pnpm -r typecheck` passes.
- **Verification (actual, 2026-11-21):** `cargo fmt --check` pass;
  clippy 0 warnings; `cargo test --workspace --exclude
  reponyx-desktop` **328 passed, 0 failed** (was 302; 26 new tests
  incl. retry-budget, rate-limit, and lease-recovery coverage);
  `pnpm -r typecheck` pass; desktop vite build + eslint pass.
  Desktop Rust is verified by `cargo fmt` + review only on this VM
  (gdk blocker); IPC progress push to the UI is still unwired.

### Added — Phase 6 + Phase 7 code (user, 2026-11-21; NOT verified complete)

- **Sync queue (`crates/sync-queue`):** WorkerPool, atomic claiming
  via Phase 2 `claim_next_job`, complete/fail/retry, crash recovery,
  progress broadcast events, `GitJobRunner` using the git-engine
  (mirror clone/fetch, working fetch/clone) with credentials from the
  OS store (PAT) or the Auth Broker (GitHub App).
- **IPC (`crates/ipc`):** protocol v1 over named pipes (Windows) /
  Unix sockets, length-prefixed JSON frames, shared-secret auth from
  the OS credential store, server + client.
- **Background agent (`apps/background-agent`):** scheduler loop,
  queue startup, IPC server, Task Scheduler helpers, plus desktop
  `agent.rs` / `tray.rs` and an Agent health page (`Agent.tsx`).
- **Verification (actual, 2026-11-21) — these were NOT done as
  landed:** the three new crates are not workspace members and
  cannot be built (`cargo check` fails with the workspace-membership
  error); `pnpm -r typecheck` fails because `Agent.tsx` imports six
  api-client functions and an `AgentHealth` type that do not exist;
  the agent does not compile (`materialize_due` called with one
  argument, needs two); the desktop never declares the agent/tray
  modules and has no `/agent` route; the retry budget never binds
  (transient failures create a fresh row with `attempt_count = 0`);
  no heartbeat/lease columns, no queue cancel, no queue rate
  limiting, no priority aging. See
  `ai/memory/PROJECT_STATUS.md` ("Phase 6 / Phase 7 verification")
  for the full list. The scheduler crate remains the only
  verified-complete Phase 7 piece. **All of the above was fixed the
  same day — see "Fixed — Phase 6 + Phase 7 integration" above.**

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
