# RepoNyx — Project Status

Last updated: 2026-11-18 (Phase 3: Git engine complete — see below)

## Current phase

**Phase 1 — Foundation (complete, merged to `main` 2026-11-18 as
`9cc87b9`, PR #1). Phase 2 — Core Database and Domain — complete
2026-11-18. Phase 3 — Git Engine — complete 2026-11-18. Phase 4 —
GitHub Integration — complete 2026-11-18 (see below).** Work is on
`main` going forward.

## Completed tasks

- **Phase 4 — GitHub Integration (actual, 2026-11-18, on `main`):**
  new crate `crates/github-provider` implementing the `GitProvider`
  trait: `validate_auth`, paginated `discover_repositories`,
  `clone_url`, `acquire_credential`, `capabilities()`,
  `provider()`. API client (`client.rs`): reqwest with a 30 s
  request timeout, `Authorization` header per token kind (`Bearer`
  for installation tokens, `token` for PATs), `User-Agent:
  RepoNyx/<version>`, retries with exponential backoff + jitter
  (max 3 attempts) for 5xx / network errors only — never for
  401/403. Error mapping: 401 → `AuthenticationExpired`, 403 with
  a revoked/suspended message → `AuthenticationRevoked`, other 403
  → `InsufficientPermissions`, 404 → `RepositoryNotFound`, 5xx
  exhausted → `InternalError` (upstream failure, not a local
  error), transport failure → `NetworkUnavailable` (timeout →
  `OperationTimedOut`). Pagination (`pagination.rs`): parses the
  `Link: rel="next"` header; the cursor is the next-page URL
  itself, validated to stay on the configured API base (a plain
  prefix match was a real bug — `api.github.com.evil…` passed it;
  now the cursor must equal the base or continue it with `/`).
  Missing/malformed `Link` header is treated as the last page,
  never a crash. `discover_all_stream` yields one message per API
  page over an mpsc channel, in order, so thousands of repos never
  sit in memory at once; an error message is terminal. Discovery
  uses `GET /user/repos?per_page=100&sort=full_name&direction=asc`
  for a deterministic page order. Rate limits (`rate_limit.rs`):
  `X-RateLimit-Limit/Remaining/Reset` and `Retry-After` parsed on
  every response and exposed via `rate_limit()`; on 429 / 403-rate
  -limit the client sleeps until reset, capped at 300 s per wait
  and 3 waits per request, then resumes from the same cursor;
  beyond the cap it returns `RateLimitExceeded` with the reset
  time in `details`. Installations: `GET /user/installations`
  and `GET /user/installations/{id}/repositories` (user-token
  endpoints only — `GET /app/installations` and installation-token
  minting need an App JWT and belong to Phase 5, which supplies a
  broker-backed `CredentialSource`); `check_repository_access`
  (`GET /repos/{o}/{r}`) reports Admin/Write/Read/None plus
  `can_read_contents`. Discovery writes via the Phase 2
  repositories (`sync.rs`): upserts preserve local state
  (`sync_enabled`, `sync_mode`, `local_path`, `archive_status`);
  local repos absent from discovery are returned in `not_seen`
  and never deleted. `sync_installations` persists installations
  via a new `upsert_installation` in `reponyx-database` (the only
  database-crate change; it refreshes `permissions_json` and
  status and deliberately bypasses the status transition table —
  the API is the source of truth). Schema gap recorded: the V1
  `installations` table has no columns for `app_id`, `app_slug`,
  `repository_selection`, or the `suspended_at` timestamp (only
  suspended-or-not is stored as the status) — needs a V3 migration
  if a later phase wants them. Credentials: `CredentialSource`
  trait lives in the provider crate (GitHub-specific token kinds);
  `StaticTokenSource` for tests, `FailingTokenSource` for failure
  tests; Phase 5 must supply a broker-backed installation-token
  source (cache until near expiry, refresh on 401) and a PAT
  source (PAT from the OS credential store, GitHub login as the
  username). No JWT signing, no private key, no credential-store
  reads, no production credential was created or modified in this
  phase. Token secrecy: `ShortLivedCredential` now has a
  hand-written `Debug` that redacts the token (the derived impl
  printed it); `Token`, `StaticTokenSource`, `GitHubClient` and
  `GitHubProvider` all redact in `Debug`; error messages never
  contain a token (tested). **Dependencies added (all latest
  stable, verified on crates.io):** `reqwest` 0.13.5 (already in
  the lockfile via tauri), `wiremock` 0.6.5 (dev-only). **Bugs
  found and fixed during Phase 4 (caught by the new tests):** (1)
  the cursor prefix-match hole above; (2) a test filter that
  matched `per_page` when looking for a `page` parameter
  (`per_page` contains the substring `page=`). **Verification
  (actual, 2026-11-18):** `cargo fmt --check` pass; clippy
  (`--workspace --exclude reponyx-desktop --all-targets`) pass, 0
  warnings; `cargo test --workspace --exclude reponyx-desktop`
  **215 passed, 0 failed** (core 11, database 55 — installations
  8 with the new upsert test — git-engine 87, github-provider 58
  — lib 26, discovery 8, errors 8, installations 4, rate_limit 4,
  redaction 6, sync 2 — logging 4, provider-abstractions 0);
  `pnpm -r typecheck` pass (Node 26.13.1); `pnpm --filter
  @reponyx/desktop build` pass (2305 modules, 1.56 s). Tauri
  binary build still blocked on this VM by missing `gdk-3.0` /
  WebViewGTK system libraries — not claimed passed. Deliberately
  NOT done in Phase 4: GitHub App JWT signing / private-key
  handling (Phase 5), PAT reads from the OS credential store
  (Phase 5), `GET /app/installations` (App-JWT-only), any
  desktop/UI changes, any TS changes (no new core domain types —
  provider types stay in the provider crate until the UI consumes
  them).
- **Phase 3 — Git Engine (actual, 2026-11-18, on `main`):**
  repository lock manager (`crates/git-engine/src/locks.rs`):
  in-process async mutex per canonical path + cross-process
  advisory file lock via `fs2` (lock file lives next to the repo as
  `.reponyx-<name>.lock`, never deleted — the OS releases an
  advisory lock when its holder dies, so a leftover file is never
  stale; deleting it would reintroduce the unlink race);
  `try_acquire` (fail fast with `RepositoryLocked`), blocking
  acquire with timeout, RAII release on drop. Mirror clone/fetch
  (`mirror.rs`): `mirror_clone` (`clone --mirror --progress`,
  verifies bare + refs, partial dest left for diagnosis, never
  deleted) and `mirror_fetch` (`fetch --prune --tags --progress`,
  returns added/pruned ref diff). Working clones (`working.rs`):
  `working_fetch` (`fetch --all --prune --tags` only — never
  checkout/reset/clean), `working_status` (`status --porcelain=v1`,
  Conflicted > Staged > Modified > UntrackedOnly > Clean), and
  `divergence` (`rev-list --left-right --count` vs upstream; no
  upstream is `None`, not an error). Branch/tag discovery
  (`refs.rs`): `list_branches` / `list_tags` / `head_sha` via
  `git for-each-ref` (NUL-separated; annotated tags carry the
  peeled commit SHA). Credential helper (`credentials.rs`):
  temp-dir helper script containing no secret; the credential
  travels only via `REPONYX_GIT_CREDENTIAL_*` env vars to the git
  child, `GIT_TERMINAL_PROMPT=0` so git never prompts, temp dir
  (0700) deleted on drop even on failure/cancellation; URLs with
  embedded credentials are rejected. Progress (`progress.rs` +
  streaming runner in `process.rs`): git `--progress` stderr
  parsed into structured `ProgressEvent`s (Counting/Compressing/
  Receiving/Resolving/CheckingOut/UpdatingFiles/Other) — only
  numbers git emits are reported, percentages never invented.
  LFS detection (`detect.rs` + `LfsStatus` in `crates/core` and
  `packages/shared`): `detect_lfs()` / `lfs_status()`; version
  parser handles both `git-lfs version 3.5.1` and `git-lfs/3.5.1`
  forms (git-lfs is NOT installed on this VM — the present path is
  covered by a fake shim in tests). Recovery (`health.rs`):
  `verify_repository` returns a structured `RepositoryDiagnosis`
  (never `Err`, never modifies anything): NotARepository,
  IncompleteClone (interrupted clone: missing HEAD/objects),
  Locked (reponyx file lock, or `index.lock` — stale if mtime >
  1 h, documented heuristic), Corrupt (`rev-parse`/`fsck`
  failure), Healthy (incl. empty repo, head `None`); each status
  carries a suggested action. Cancellation/timeouts end to end:
  every long op takes `SyncOptions` (timeout 300 s default, lock
  timeout 60 s, `CancellationToken`, progress channel, credential);
  on cancel/timeout the child is killed and the lock released.
  **Dependencies added (all latest stable, verified on
  crates.io):** `fs2` 0.4.3, `tokio-util` 0.7.20 (CancellationToken
  is under default features — there is no `sync` feature), `libc`
  0.2.191 (dev-only; the `1.0.0-alpha.5` listing is pre-release and
  was not used); `tempfile` 3.27.0 moved to regular deps (the
  credential helper needs it at runtime). **Bugs found and fixed
  during Phase 3 (caught by the new tests):** (1) `try_acquire`
  returned success with no file lock when another process held it
  (`Ok(None)` from the lock attempt was treated as acquired) —
  now returns `RepositoryLocked`; (2) test-fixture collision: the
  seed fixture's internal clone dir was named `work`, the same
  path tests used for their own clones — renamed to `seed-work`;
  (3) `build_rev_parse_upstream` passed both `--abbrev-ref` and
  `--symbolic-full-name` and git let `--abbrev-ref` win
  (`origin/main` instead of `refs/remotes/origin/main`) —
  `--abbrev-ref` dropped. **Disk-full failure injection:** a true
  ENOSPC was not feasible (a tiny tmpfs needs mount privileges the
  VM does not grant; filling the real filesystem would endanger
  the machine), so the test lowers `RLIMIT_FSIZE` to 1 MiB in the
  test process (inherited by the git child) and clones a fixture
  with 3 × 1 MiB incompressible pseudo-random blobs over a
  `file://` URL (a plain local-path clone would hardlink and never
  write pack data); the limit is set only around the clone and
  restored by an RAII guard, and the file holds exactly one test
  because a process-wide rlimit is unsafe with parallel tests. A
  read-only-parent clone test is skipped when running as root
  (this VM runs tests as uid 0, for whom permission bits do not
  block writes). **Environment quirk found:** this VM's `/tmp`
  (tmpfs) intermittently returns ETXTBSY ("Text file busy") when a
  freshly written script is executed immediately under parallel
  load — reproduced independently at ~5% with a write/chmod/exec
  stress loop, while exec of an unchanged file never flakes and
  `sh <script>` never flakes; a 2 ms delay nearly eliminates it.
  The LFS-shim test retries detection up to 5 times for that
  reason (test only — production `git-lfs` is installed, never
  freshly written, and git's own startup latency covers the
  credential helper). **Verification (actual, 2026-11-18):**
  `cargo fmt --check` pass; clippy (`--workspace --exclude
  reponyx-desktop --all-targets`) pass, 0 warnings; `cargo test
  --workspace --exclude reponyx-desktop` **156 passed, 0 failed**
  (core 11, database 54, git-engine 87 — lib 58, cancellation 3,
  credentials 4, disk_full 1, health 6, locks 3, mirror 6,
  mirror_clone 1, refs 1, working 4 — logging 4,
  provider-abstractions 0); `pnpm -r typecheck` pass (Node
  26.13.1); `pnpm --filter @reponyx/desktop build` pass (2305
  modules, 1.82 s). Git-engine suite re-run 3× consecutively with
  the same result. Tauri binary build still blocked on this VM by
  missing `gdk-3.0` / WebViewGTK system libraries — not claimed
  passed. Deliberately NOT done in Phase 3: LFS sync itself
  (Phase 9), repair of corrupt repos (diagnosis only here — a
  later phase), any desktop/UI changes, any dependency changes
  beyond the three additions above.
- **Phase 2 — Core Database and Domain (actual, 2026-11-18, on
  `main`):** repository-pattern data access for all 9 V1 tables in
  `crates/database` (one module per table: `accounts`,
  `installations`, `repositories`, `schedules`, `jobs`, `history`,
  `settings`, `devices`, `audit`), all SQL with bound parameters
  only. Domain types added in `crates/core` (`Schedule`,
  `SyncHistory`, `Setting`, `Device`, `AuditLog`, `Installation`,
  `AccountType`, `ScheduleScope`, `IntervalType`, `SyncResult`,
  `DevicePairingStatus`, `InstallationType`, `InstallationStatus`)
  with explicit allowed-transition tables; `revoked` /
  `token_expired` accounts return to `connected` only via explicit
  `reconnect_account`. TS mirrors added in `packages/shared`
  (including the previously missing `accountType` on `Account`).
  Behaviours: upsert by `(account_id, provider_repository_id)` via
  atomic `ON CONFLICT DO UPDATE` (preserves local state);
  rename/ownership-change with full_name collision detection;
  effective-schedule resolution (repository > account > global) +
  `due_schedules`; atomic job claim via single `UPDATE ...
  RETURNING` statement; job finish + history entry in one
  transaction (`complete_job_with_history`); audit logs append-only
  by design (no update/delete API). **Migration V2 was needed:**
  `migrations/sqlite/0002_unique_full_name.sql` adds a UNIQUE index
  on `repositories (account_id, full_name)` — code-level collision
  checks can race under concurrent writes (SQLite transactions are
  deferred by default), so the invariant is now enforced at the
  storage layer; V1 was not edited. One dependency added:
  `serde_json` to `reponyx-database` (already in the lockfile at
  1.0.154, latest stable; used for audit-log metadata JSON).
  **Bugs found and fixed during Phase 2 (caught by the new tests):**
  (1) `transition_job` / `complete_job_with_history` were missing
  the final `.bind(id)` — sqlx does not check bind counts for
  `query()`, so `WHERE id = ?` matched NULL and the UPDATE silently
  affected 0 rows; (2) `claim_next_job` used select-then-update in
  a transaction, which deadlocks under contention (shared-lock
  upgrade) — rewritten as a single atomic `UPDATE ... RETURNING`.
  **Verification (actual, 2026-11-18):** `cargo fmt --check`
  pass; clippy (5 buildable crates) pass, 0 warnings; `cargo test`
  **91 passed, 0 failed** (core 11, database 54 — accounts 8,
  installations 7, jobs 8, migrations 3, repositories 10, schedules
  7, settings/devices/audit 7, transactions 4 — git-engine 22,
  logging 4, provider-abstractions 0); `pnpm -r typecheck` pass
  (4 packages); `pnpm --filter @reponyx/desktop build` pass
  (1.81 s). `cargo test --workspace` exits 101 on this VM solely
  because the desktop crate cannot build (`gdk-sys`: `Package
  gdk-3.0 was not found`) — the known blocker, not claimed passed.
  pnpm had vanished from this VM again and was reinstalled at
  12.21.0 (latest stable).
- Toolchains installed and verified: Rust 1.100.0, pnpm 12.21.0,
  Node 24.22.1, Git 2.43.0.
- **Phase 1 re-check + dependency update (actual, 2026-11-18, on `main`):**
  the user's 8 commits on `origin/main` (6346ae9, 350f0b2, ac5e87e,
  254700a, de73403, 973eb05, 2d9641c, 8727c65) were merged into local
  `main` as `d3adc8c` (after committing the local doc updates as
  `b45e55e`; no conflicts, no rebase, no force-push). What the user
  changed, and the assessment of each:
  - `6346ae9` root `package.json`: reformat; `packageManager` pnpm
    12.19.0 (a downgrade — restored to 12.21.0, latest stable);
    `engines: node >=26` (kept); TypeScript ^6.0.3; **invalid JSON —
    missing comma after `@types/node` (fixed)**.
  - `350f0b2` desktop `package.json`: rewrite; all versions bumped to
    latest stable (verified against npm); added `@tauri-apps/cli`
    2.12.6 (needed by the `tauri` script) and `plugin-sql` 2.6.0
    (removed again in 8727c65).
  - `ac5e87e` `tauri.conf.json`: pretty-printed; centered window;
    expanded CSP (incl. `connect-src https://api.reponyx.com` — a
    future endpoint; the domain is not bought yet); updater artifacts
    + updater plugin config with a minisign pubkey; icon list
    (the icon files did not exist — generated, see below); NSIS/WiX.
  - `254700a` `build.rs`: Windows app manifest + `try_build` with
    `Attributes` (API valid for tauri-build 2.7.6); formatting fixed
    with `cargo fmt` (leading blank line, wrapping).
  - `de73403` `default.json`: added `updater:default` permission —
    valid now that the Rust updater plugin is registered (see fixes).
  - `973eb05` root `package.json`: fixed `tauri:dev`/`tauri:build`
    scripts to `tauri dev` / `tauri build` (correct).
  - `2d9641c` `Cargo.toml`: `rust-version` 1.90 → 1.100.0 (correct);
    `tauri "=2.12.7"` → `"=2"` (**wrong — in Cargo `"=2"` means
    exactly 2.0.0 and would downgrade Tauri; fixed to `"2"`, which
    resolves to latest stable 2.12.7**); `tauri-build` `"=2.7.6"` →
    `"2"` (fine).
  - `8727c65` desktop `package.json`: removed `plugin-sql` (safe —
    no code references it).
  Fixes made in this re-check: root `package.json` comma;
  `packageManager` back to pnpm@12.21.0; `tauri "=2"` → `"2"`;
  `sqlx` 0.8 → 0.9 (latest stable 0.9.1, compiles and tests clean);
  wired the updater the user configured — added
  `tauri-plugin-updater = "2"` (workspace + `src-tauri`) and
  `.plugin(tauri_plugin_updater::Builder::new().build())` in
  `lib.rs`, so the `updater:default` capability and plugin config
  are no longer dangling; generated the missing app icons
  (`apps/desktop/src-tauri/icons/`: 32x32.png, 128x128.png,
  128x128@2x.png, icon.ico, icon.icns, plus 1024px icon.png source);
  `cargo fmt` on `build.rs`. Node 26.13.1 (latest stable v26;
  v27 is alpha, not used) installed at
  `~/.local/node-v26.13.1-linux-x64` for verification — the system
  Node is still 24.22.1; pnpm had vanished from this VM and was
  reinstalled at 12.21.0. `pnpm install` also added a
  `minimumReleaseAgeExclude` block to `pnpm-workspace.yaml`
  (pnpm 12's auto-generated supply-chain policy for packages
  published within the last day); kept as generated.
  **Dependency updates (old → latest stable):** TypeScript 5.9.3 →
  6.0.3 (see "deliberately not updated" below); zod 4.3.5 → 4.6.7;
  react 19.3.0 → 19.3.1; react-dom 19.3.0 → 19.3.1; react-router
  8.4.0 → 8.5.2; @tauri-apps/api 2.9.0 → 2.12.5 (api-client);
  @types/react 19.2.0 → 19.3.0; sqlx 0.8.6 → 0.9.1; all other Rust
  crates already at latest stable in the lockfile (tokio 1.54.1,
  serde 1.0.229, serde_json 1.0.154, chrono 0.4.45, thiserror 2.0.21,
  tracing 0.1.44, tracing-appender 0.2.5, tracing-subscriber 0.3.23,
  uuid 1.29.3, async-trait 0.1.92, tempfile 3.27.0, tauri 2.12.7,
  tauri-build 2.7.6); desktop npm deps were already latest stable
  (lucide-react 1.75.0, eslint 10.15.0, vite 8.3.8, tailwindcss
  4.3.5, @vitejs/plugin-react 6.3.0, typescript-eslint 8.73.0,
  @tauri-apps/cli 2.12.6, @tauri-apps/plugin-updater 2.14.1).
  **Re-verification (actual, 2026-11-18):** `cargo fmt --check`
  pass; clippy (5 crates) pass, 0 warnings; `cargo test`
  **36 passed, 0 failed** (same breakdown as Phase 1);
  `pnpm install` pass (Node 26.13.1, pnpm 12.21.0, no peer issues
  after the TS pin); `pnpm -r typecheck` pass (4 packages);
  desktop eslint pass; `pnpm -r lint` pass;
  `pnpm --filter @reponyx/desktop build` pass (2305 modules,
  ~1 s). `cargo check -p reponyx-desktop` still fails with the
  same missing `gdk-3.0` / `gdk-pixbuf-sys` system libraries —
  Tauri binary build remains BLOCKED on this VM, not claimed
  passed; no tauri-build, capability, icon, or updater error
  appeared before that failure.
  **Deliberately NOT updated:** TypeScript stays at 6.0.3, not
  latest stable 7.0.2 — tried 7.0.2 first (typecheck passed), but
  typescript-eslint 8.73.0 (latest stable) hard-fails with
  "typescript-eslint does not support TS 7.0" (peer range
  `>=4.8.4 <6.1.0`); TS 7.1 stable, which typescript-eslint will
  support, is not released yet (only 7.1.0-beta exists — pre-release,
  forbidden). Revisit when TS 7.1 stable lands. Tauri stays at
  2.12.7 — 3.0.0-alpha.6 exists but is alpha. Rust stays at 1.100.0
  (`rustup check`: stable up to date).
- Repo initialized on `feature/reponyx-foundation`; nothing pre-existing
  to preserve (empty workspace).
- Monorepo scaffold: Cargo workspace (edition 2024, resolver 2) +
  pnpm workspaces; root docs (README, AGENTS.md, CLAUDE.md,
  CONTRIBUTING.md, SECURITY.md, CHANGELOG.md, LICENSE MIT).
- Rust crates: core (domain + structured errors), logging (tracing +
  secret redaction), git-engine (detection, command builders, safe
  process execution, path validation), provider-abstractions
  (`GitProvider` trait), database (SQLx/SQLite V1 migrations).
- Desktop app: Tauri 2.12.7 + React 19 + Vite 8 + Tailwind 4;
  8-section routing; Overview reads real DB counts + real Git status;
  other sections are honest Phase-1 placeholders.
- Shared TS packages: shared (types + zod), ui (Button/Card/Badge),
  api-client (typed invoke wrapper), config (tsconfig/eslint base).
- Docs: architecture ×8, ADRs 001–010, threat model (12 threats),
  phase plans 1–10.
- **Phase 1 verification (actual, 2026-11-18):** `cargo fmt --check`
  pass; clippy on the 5 buildable crates pass, 0 warnings; `cargo test`
  36 passed / 0 failed (core 7, database migrations 3, git-engine 21
  unit + 1 mirror-clone integration, logging 4); `pnpm -r typecheck`
  pass; desktop eslint pass; `pnpm --filter @reponyx/desktop build`
  pass. **Tauri build BLOCKED on this VM** by missing system libs
  (see Known issues) — not claimed passed.
- **Renamed from working name Gitora to RepoNyx on 2026-11-18.**
  Gitora was unusable because gitora.com is an existing Oracle Git
  product. Renamed: repo directory (`~/workspace/reponyx`), branch
  (`feature/reponyx-foundation`), Rust crates (`reponyx-*`), TS
  packages (`@reponyx/*`), Tauri product name `RepoNyx` / identifier
  `com.reponyx.app`, default storage `D:\RepoNyx\`, database
  `reponyx.sqlite`, env var `REPONYX_DATA_DIR`. Two rename-induced
  rustfmt width fixes applied (`mirror_clone.rs`, `lib.rs` import
  order). **Re-verified after the rename (actual, 2026-11-18):**
  `cargo fmt --check` pass; clippy (5 crates) pass, 0 warnings;
  `cargo test` 36 passed / 0 failed; `pnpm install` pass;
  `pnpm -r typecheck` pass; desktop eslint pass;
  `pnpm --filter @reponyx/desktop build` pass (1.39 s).
  `cargo check -p reponyx-desktop` still fails with the same missing
  `gdk-3.0` / WebViewGTK system libraries — Tauri binary build
  remains BLOCKED on this VM, not claimed passed.
- Two real bugs found and fixed during verification: (1) `tauri-build
  =2.12.6` does not exist — corrected to 2.7.6, `tauri` to 2.12.7
  (verified against the crates.io index); (2) an infinite loop in
  `logging::redact_secrets` Bearer-header redaction (re-searching from
  the start re-found the prefix forever) — fixed with a search cursor.
  Also fixed: `reponyx-core` referenced `sqlx` without depending on it;
  `wait_with_output()` consumes the child, so cancellation now uses
  `wait()` + separately-read pipes.

## Pending tasks (Phase 1)

- [x] `cargo fmt --check`, `cargo clippy` (5 buildable crates),
      `cargo test` — results below.
- [x] `pnpm install`, `pnpm -r typecheck`, `pnpm --filter @reponyx/desktop build`,
      desktop `eslint` — all pass.
- [x] Tauri build attempted — **blocked** on this Linux VM by missing
      system libraries (see Known issues); not claimed passed.

## Tests executed

Actual results, 2026-11-18 (all commands run, not inferred).
Every check in this table was re-run after the Gitora → RepoNyx
rename with the same results (desktop build 1.39 s on the re-run).

| Check | Result |
| --- | --- |
| cargo fmt --check | pass (after `cargo fmt`) |
| cargo clippy -p core,logging,git-engine,provider-abstractions,database --all-targets | pass, 0 warnings |
| cargo test (same 5 crates) | **36 passed, 0 failed** — core 7, database migrations 3, git-engine unit 21 + mirror-clone integration 1, logging 4, provider-abstractions 0 |
| pnpm install | pass (151 packages; pnpm-lock.yaml committed) |
| pnpm -r typecheck | pass (shared, ui, api-client, desktop) |
| pnpm --filter @reponyx/desktop lint (eslint) | pass |
| pnpm --filter @reponyx/desktop build (vite) | pass (1844 modules, 1.03 s) |
| cargo check -p reponyx-desktop | **FAIL — blocked**: `gdk-sys v0.18.2` build script: `Package gdk-3.0 was not found in the pkg-config search path` |
| cargo tauri build (Linux) | **not run to completion — blocked** by the same missing system libraries; see below |

### Phase 4 — 2026-11-18 (GitHub integration, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check | Result |
| --- | --- |
| cargo fmt --check | pass |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings |
| cargo test --workspace --exclude reponyx-desktop | **215 passed, 0 failed** — core 11, database 55 (installations 8), git-engine 87, github-provider 58 (lib 26, discovery 8, errors 8, installations 4, rate_limit 4, redaction 6, sync 2), logging 4, provider-abstractions 0 |
| pnpm -r typecheck | pass (shared, ui, api-client, desktop) |
| pnpm --filter @reponyx/desktop build (vite) | pass (2305 modules, 1.56 s) |
| cargo check -p reponyx-desktop | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1 |

### Phase 3 — 2026-11-18 (Git engine, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0,
Git 2.43.0 (git-lfs not installed on this VM).

| Check | Result |
| --- | --- |
| cargo fmt --check | pass |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings |
| cargo test --workspace --exclude reponyx-desktop | **156 passed, 0 failed** — core 11, database 54, git-engine 87 (lib 58, cancellation 3, credentials 4, disk_full 1, health 6, locks 3, mirror 6, mirror_clone 1, refs 1, working 4), logging 4, provider-abstractions 0 |
| cargo test -p reponyx-git-engine (3 consecutive runs) | 87 passed, 0 failed each run |
| pnpm -r typecheck | pass (shared, ui, api-client, desktop) |
| pnpm --filter @reponyx/desktop build (vite) | pass (2305 modules, 1.82 s) |
| cargo check -p reponyx-desktop | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1 |

### Re-check 2026-11-18 (user's 8 commits + dependency updates, on `main`)

All commands actually run, after the merge (`d3adc8c`) and the fixes
listed above. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check | Result |
| --- | --- |
| cargo fmt --check | pass (after `cargo fmt` on `build.rs`) |
| cargo clippy -p core,logging,git-engine,provider-abstractions,database --all-targets | pass, 0 warnings |
| cargo test (same 5 crates) | **36 passed, 0 failed** — core 7, database migrations 3, git-engine unit 21 + mirror-clone integration 1, logging 4, provider-abstractions 0 |
| cargo update | pass — sqlx 0.8.6 → 0.9.1, tauri-plugin-updater 2.14.1 added, tauri 2.12.7, tauri-build 2.7.6, all others already latest stable |
| pnpm install | pass (Node 26.13.1; `pnpm peers check`: no peer issues after TS pinned to 6.0.3) |
| pnpm -r typecheck | pass (shared, ui, api-client, desktop) — also passed under TS 7.0.2 before the eslint-driven pin-back |
| pnpm --filter @reponyx/desktop lint (eslint) | pass |
| pnpm -r lint | pass |
| pnpm --filter @reponyx/desktop build (vite) | pass (2305 modules, ~1 s) |
| cargo check -p reponyx-desktop | **FAIL — blocked**: `gdk-sys v0.18.2` + `gdk-pixbuf-sys v0.18.0`: `Package gdk-3.0 was not found in the pkg-config search path`. No tauri-build, capability, icon, or updater error appeared before this failure. |

## Files created

See `git status` on `feature/reponyx-foundation` (full list in the
Phase 1 handoff). Top level: `Cargo.toml`, `package.json`,
`pnpm-workspace.yaml`, `rust-toolchain.toml`, `.gitignore`,
`.editorconfig`, `LICENSE`, `README.md`, `CONTRIBUTING.md`,
`SECURITY.md`, `CHANGELOG.md`, `AGENTS.md`, `CLAUDE.md`;
`crates/{core,logging,git-engine,provider-abstractions,database}`;
`apps/desktop` (+ `src-tauri`); `packages/{shared,ui,api-client,config}`;
`migrations/sqlite/0001_init.sql`; `docs/architecture/*`,
`docs/decisions/ADR-001..010`, `docs/security/threat-model.md`;
`ai/phase1..10/PLAN.md`.

## Architectural decisions

- ADRs 001–010 accepted (see `docs/decisions/`).
- Git CLI (not libgit2) as the Git engine.
- Mirror clone as default sync mode.
- SQLite + SQLx, WAL, versioned migrations.
- Credentials in OS credential store only; GitHub App private key in
  the Auth Broker only — never in the desktop binary.

## Known issues

- **Linux Tauri build is BLOCKED on this VM (environment, not code).**
  `cargo check -p reponyx-desktop` fails in `gdk-sys v0.18.2`:
  `Package gdk-3.0 was not found in the pkg-config search path`.
  `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev
  libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev` fails with
  `E: Unable to correct problems, you have held broken packages` because
  `libzstd-dev` requires `libzstd1 (= 1.5.5+dfsg2-2build1.1)` but the
  image pins `libzstd1 1.5.5+dfsg2-2build1.1+hatch1` (no repo source for
  reinstall). Downgrading that core library (used by systemd/apt/curl)
  was deliberately **not** attempted. Fix on a clean Ubuntu 24.04 (or a
  Windows machine) by installing the dev libraries, then re-run
  `cargo tauri build`.
- `cargo clippy --workspace` / `cargo test --workspace` cannot run on
  this VM until the Tauri system deps exist; the 5 non-Tauri crates
  were verified individually (see table above).
- Windows installer / Task Scheduler / Credential Manager cannot be
  verified on this Linux VM — explicit Phase 7/10 items.

## Blockers

- **Linux Tauri build blocked** by the held-broken-packages issue
  above (missing `gdk-3.0` / WebViewGTK dev libraries). Does not block
  Phase 2 (Rust crates + frontend work do not need the Tauri shell),
  but the desktop binary cannot be produced on this VM until the
  system libraries are installed.

## Next steps

1. ~~Phase 1 commit `49a44bf` plus the rename commit on
   `feature/reponyx-foundation` (do not merge to main).~~ **Done
   2026-11-18:** merged to `main` via PR #1 (`9cc87b9`); the remote
   `feature/reponyx-foundation` branch was deleted by the user after
   merging. The local branch is kept only as the record of the original
   commits `49a44bf` + `977be27`, which GitHub holds only as the single
   snapshot commit `83a68f99`.
2. ~~Phase 2: domain repositories on top of the V1 schema
   (`ai/phase2/PLAN.md`) — awaiting the user's go-ahead.~~ **Done
   2026-11-18** (see "Completed tasks" above).
3. ~~Phase 3: Git engine (`ai/phase3/PLAN.md`) — awaiting the user's
   go-ahead.~~ **Done 2026-11-18** (see "Completed tasks" above).
4. ~~Phase 4: GitHub integration (`ai/phase4/PLAN.md`) — awaiting the
   user's go-ahead.~~ **Done 2026-11-18** (see "Completed tasks" above).
5. Phase 5: Authentication (`ai/phase5/PLAN.md`) — awaiting the
   user's go-ahead.
6. On a machine with the WebViewGTK dev libraries (or Windows):
   re-run `cargo check -p reponyx-desktop` and `cargo tauri build`.
