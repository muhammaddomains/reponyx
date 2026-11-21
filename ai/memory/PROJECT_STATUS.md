# RepoNyx — Project Status

Last updated: 2026-11-21 (Phase 8 Synchronization page built — see
"Phase 8 — Synchronization page" below; Schedules + Repositories
pages done the same day; Phase 6 / Phase 7 fixes verified + live
end-to-end run on this VM — see "Phase 6 / Phase 7 fixes +
re-verification" and "Live end-to-end run" below)

## Current phase

**Phase 1 — Foundation (complete, merged to `main` 2026-11-18 as
`9cc87b9`, PR #1). Phase 2 — Core Database and Domain — complete
2026-11-18. Phase 3 — Git Engine — complete 2026-11-18. Phase 4 —
GitHub Integration — complete 2026-11-18. Phase 5 — Authentication
— complete 2026-11-18. Phase 6 — Sync Queue — code landed on
`main` 2026-11-21 (user commits `adb21b9`, `b24acb1`), then fixed
and verified the same day (assistant, uncommitted at the time of
writing — see "Phase 6 / Phase 7 fixes + re-verification" below).
Phase 7 — Scheduler and Background Agent — scheduler crate verified
2026-11-19; agent, IPC, tray, and Agent page code landed 2026-11-21
and were fixed and verified the same day (same section).**
**Phase 8 — Desktop UI — started 2026-11-21: the Schedules,
Repositories, and Synchronization pages are built (see "Phase 8 —
Schedules page", "Phase 8 — Repositories page", and "Phase 8 —
Synchronization page" below); the other pages (Activity, Storage,
Settings) are still placeholders.** The
desktop Rust wiring is verified by `cargo fmt` + review only (the
gdk blocker); the real desktop compile check is the user's Windows
build. Remaining honest gaps are listed in the fixes section.
Work is on `main` going forward.

## Completed tasks

- **Phase 8 — Synchronization page (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the third Phase 8 page.
  New Tauri commands in
  `apps/desktop/src-tauri/src/synchronization.rs` (wired in
  `lib.rs`): list jobs (by status / repository), counts by status,
  cancel via `request_cancel_job`. No new Rust dependencies.
  - **What it does:** agent health line (reachable, paused,
    pending/running/workers) with pause/resume queue buttons;
    per-status counts; job list with an Active (pending + running)
    filter plus All and per-status filters; repository names resolved
    from the repository rows; job type, priority, attempt count,
    worker, created/started/finished times; "next attempt at …" for
    pending jobs waiting on retry backoff or rate-limit defer;
    error code + message on failed jobs; **Cancel** (pending jobs
    cancel at once, running jobs get a cancel request the worker
    honors on its next heartbeat); **Retry now** on failed/cancelled
    jobs (ensures the agent is running, then re-enqueues the
    repository — the same path as "Sync now"); auto-refresh every 5
    seconds.
  - **Honest limits:** no live progress bars — the queue emits
    `JobProgress` events but IPC does not forward them to the
    desktop, so the page polls job rows instead (and says so on the
    page); the new Rust commands are verified by `cargo fmt` +
    review only (gdk blocker) — the Windows build is their first
    real compile; the job list has no server-side limit (the page
    renders the first 100 rows); no frontend tests yet.
  - **Verification (actual, 2026-11-21):** `cargo fmt --check`
    pass; `cargo clippy --workspace --exclude reponyx-desktop
    --all-targets` pass, **0 warnings**; `cargo test --workspace
    --exclude reponyx-desktop` **331 passed, 0 failed**; `pnpm -r
    typecheck` pass; `pnpm --filter @reponyx/desktop build` pass;
    desktop eslint pass on the touched files (exit 0, after removing
    one unused constant the linter flagged).

- **Phase 8 — Repositories page (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the second Phase 8 page.
  Unlike Schedules, this one needed a backend: new Tauri commands
  in `apps/desktop/src-tauri/src/repositories.rs` (wired in
  `lib.rs`), plus `reponyx-sync-queue` added as a desktop
  dependency so discovery reuses the agent's credential resolver
  (PAT from the OS store, GitHub App via the broker — no duplicated
  resolver logic).
  - **What it does:** lists repositories with account/status/sync
    filters and debounced name search; **discover** from GitHub per
    account (pages through `GET /user/repos`, upserts into the DB —
    new rows start sync-enabled in mirror mode, existing rows keep
    their settings, unseen local rows are reported and never
    deleted); **sync now** per repo (ensures the agent is running,
    then enqueues via IPC); pause/resume; sync-mode change
    (mirror/working/both); bulk select with sync-now / pause /
    resume; open local folder (only when a local copy exists) and
    open on GitHub (owner/name validated to safe path characters,
    opened as a single argument — never through a shell); **remove**
    with a confirm dialog — deletes the row only, local files are
    kept.
  - **Honest limits:** the new Rust commands are verified by
    `cargo fmt` + review only (gdk blocker) — the Windows build is
    their first real compile; the list shows 100 rows at a time
    with a "Show more" button instead of true virtualization (the
    plan asks for virtualization; deferred, noted in the plan);
    discovery is a full paged scan with no progress indicator yet;
    no frontend tests yet.
  - **Verification (actual, 2026-11-21):** `cargo fmt --check`
    pass; `cargo clippy --workspace --exclude reponyx-desktop
    --all-targets` pass, **0 warnings**; `cargo test --workspace
    --exclude reponyx-desktop` **331 passed, 0 failed**; `pnpm -r
    typecheck` pass; `pnpm --filter @reponyx/desktop build` pass
    (1.77 s); desktop eslint pass on the touched files (exit 0).
    `cargo tree -p reponyx-desktop` confirms the new
    `reponyx-sync-queue` dependency resolves; `Cargo.lock`
    regenerated.

- **Phase 8 — Schedules page (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the first Phase 8 page,
  built on the Phase 7 scheduler backend (7 Tauri commands, zod
  schemas, typed api-client calls — no backend changes were needed).
  `apps/desktop/src/routes/Schedules.tsx` (new) is wired to
  `/schedules` in `App.tsx`, replacing the placeholder.
  - **What it does:** lists every schedule (scope, cadence in plain
    words, timezone, next run, enabled badge, next-run error when
    the cadence no longer parses); create with scope global /
    account (dropdown from `listAccounts`) / repository (id field —
    no repository list exists yet, so the id is typed in);
    enable/disable toggle; inline edit of cadence, timezone, and
    enabled flag; delete with a confirm dialog; per-schedule
    preview of the next 5 runs in the schedule's timezone; "Run due
    schedules now" (`materializeDue`) with the full report
    (checked/primed/created/skipped/errors); effective-schedule
    lookup by repository id (repository > account > global).
  - **Honest limits:** repository scope needs a typed UUID (no
    Repositories page yet); cadence input is free text per type
    (hours / `HH:MM` / `days HH:MM` / minutes) and the backend is
    the validator — an invalid cadence is rejected with a real
    error, never silently stored; no frontend tests exist yet (the
    plan asks for them; the page is verified by typecheck + build +
    eslint only, as with the other pages).
  - **Verification (actual, 2026-11-21):** `cargo fmt --check`
    pass (no Rust changes); `pnpm -r typecheck` pass; `pnpm
    --filter @reponyx/desktop build` pass (1.88 s); desktop eslint
    pass on the two touched files (exit 0). No Rust tests change:
    the suite stands at 331 passed / 0 failed. The desktop itself
    still needs its first Windows compile check.

- **Phase 7 — Scheduler crate only (PARTIAL, actual, 2026-11-19,
  on `main`):** the scheduler part of Phase 7.
  - **Scheduler crate (`crates/scheduler`):** next-run calculation,
    due detection, and job creation for due schedules, built on the
    Phase 2 schedules/jobs repository functions (no duplicated
    logic, no raw SQL in the crate, no schema change — no V4
    migration was needed). Dependencies are `reponyx-core` +
    `reponyx-database` only (proven by `cargo tree -p
    reponyx-scheduler`: no git-engine, no provider, no queue) — the
    crate creates job rows and never runs a sync.
  - **Cadence (`cadence.rs`):** `interval_value` encoding, plain or
    JSON. Plain: `"6"` (hours), `"02:30"` (daily `HH:MM`), `"1,3,5
    02:30"` (weekly, ISO weekday numbers 1 = Monday .. 7 =
    Sunday), `"90"` (custom minutes). JSON: `{"hours": 6}`,
    `{"time": "02:30"}`, `{"minutes": 90}`, plus optional
    `"weekdays": [1..7]` and `"window": {"start", "end"}`. A window
    is `[start, end)` in local time; `end <= start` crosses
    midnight; `start == end` is rejected. Bounds: hours 1..=168,
    minutes 1..=10080. An unparsable cadence is `InvalidInput`,
    never a panic or a silent default.
  - **Next-run math (`next_run.rs`):** first run strictly after
    "now", in UTC. Interval schedules are absolute-time cadences
    anchored on the stored `next_run_at` (or `created_at`); daily /
    weekly are wall-clock times in the schedule's IANA timezone.
    DST rules: a local time that does not exist (spring-forward
    gap) fires at the first valid time after the gap; a local time
    that occurs twice (fall back) fires once, at the first
    occurrence. A disabled schedule has no next run; a daily/weekly
    time outside its own window never fires (strict semantics — the
    preview shows no runs).
  - **Timezone (`timezone.rs`):** IANA names via `chrono-tz`
    0.10.4 (never hand-rolled DST tables); unknown names are
    `InvalidInput`; display formatting is UTC storage + local
    display.
  - **Precedence:** repository > account > global, resolved by
    Phase 2's `effective_schedule_for` — not reimplemented.
  - **Due + materialize (`due.rs`):** due = enabled with stored
    `next_run_at` passed (Phase 2 `due_schedules`). `materialize_due`
    primes schedules whose `next_run_at` is `NULL` (computes an
    initial value, no job), then per due schedule creates one job
    per governed repository (global: all; account: its own;
    repository: that one), skipping sync-disabled repositories and
    repositories with a pending/running job (scheduling-time dedup
    only), then advances the schedule by setting `next_run_at` to
    the next computed run — that advance is the progress marker;
    there is no `last_run_at` column and none was added. Job type:
    `fetch` when the repo already has a local path, else a clone
    job for its sync mode (`mirror`/`both` → mirror clone,
    `working` → working clone), priority 0. A due run outside the
    current window/weekdays creates no job and is advanced to the
    next valid run. Per-schedule failures are recorded in the
    report and never abort the run; a schedule whose cadence no
    longer parses is left un-advanced so it stays visible.
  - **Desktop (`apps/desktop/src-tauri/src/schedules.rs`, 7 Tauri
    commands):** list schedules (with computed next run + parse
    errors surfaced per schedule), create (validates cadence and
    timezone before writing, computes the first run), update
    (recomputes the next run when the cadence changes and the
    schedule is enabled; the old `next_run_at` stays the interval
    anchor; empty string clears `interval_value`), delete, preview
    next N runs (count 1..=50, UTC + local display in the requested
    timezone), materialize due now, effective schedule for a
    repository (next 3 runs). Typed api-client wrappers + zod
    schemas in `packages/shared` / `packages/api-client`. No
    scheduler UI page — Phase 8. **Desktop Rust verified by `cargo
    fmt` + review only (gdk blocker), not by a build — stated
    honestly.**
  - **Dependencies added (latest stable, verified on crates.io
    2026-11-19):** `chrono-tz` 0.10.4 (resolved from `"0.10"`).
    Note: the RustSec advisory DB carries an informational
    "unmaintained" notice for `chrono` / `chrono-tz` (no
    vulnerability, no patch exists); `chrono` 0.4.45 is already a
    workspace dependency used by every phase, so this changes
    nothing.
  - **Test bug found and fixed during this work (test only):** the
    new `materialize_after_job_completes` test called
    `complete_job` on a `pending` job, which the Phase 2 transition
    table correctly rejects (`pending` → `running` → `completed`);
    the test now claims the job first. No production code was
    wrong.
  - **Deliberately NOT done:** the sync queue
    / worker pool / job execution / retries / crash recovery, the
    background-agent binary, the IPC crate, Windows Task Scheduler
    registration, tray, auto-restart, agent health UI. The
    scheduler creates job rows only; nothing runs them until
    Phase 6 lands.
  - **Verification (actual, 2026-11-19):** `cargo fmt --check`
    pass; clippy (`--workspace --exclude reponyx-desktop
    --all-targets`) pass, 0 warnings (1 collapsible-`if` warning
    and 1 unused-workspace-dependency warning found and fixed
    during the run); `cargo test --workspace --exclude
    reponyx-desktop` **302 passed, 0 failed** (baseline 262; new:
    scheduler 40 — lib 8, dst 5, materialize 10, next_run 14,
    precedence 3; all other crates unchanged); `cargo tree -p
    reponyx-scheduler` confirms no execution dependencies (chrono,
    chrono-tz, core, database, serde, serde_json, sqlx, uuid only);
    `pnpm -r typecheck` pass (Node 26.13.1); `pnpm --filter
    @reponyx/desktop build` pass; desktop eslint pass. No flakes
    observed in the full-suite runs. Tauri binary build still
    blocked on this VM by missing `gdk-3.0` / WebViewGTK system
    libraries — not claimed passed.

- **Phase 6 — Sync Queue — CODE LANDED 2026-11-21, FIXED + VERIFIED
  the same day (see the fixes section below):**
  - crates/sync-queue: WorkerPool, claim/complete/fail/retry, crash recovery,
    progress broadcast events, JobRunner trait.
  - GitJobRunner: mirror_clone / mirror_fetch / working_fetch / working clone
    via git-engine; credentials via DefaultCredentialResolver (PAT from OS
    store, GitHub App via Auth Broker).
  - Retries create a new sync_jobs row (never running → pending).
  - Tests (in the crate, not run yet — see verification below):
    queue_integration (100 jobs, retry, auth permanent, recovery),
    git_runner_unit (storage.root, LFS reject).
  - CORRECTION to the earlier claim in this file: the workspace was NOT
    updated — `crates/sync-queue` is not in root `Cargo.toml`
    `members`, there is no `reponyx-sync-queue` workspace path
    dependency, and `Cargo.lock` has no entry for it. The crate cannot
    be built by `cargo --workspace` or standalone (cargo: "current
    package believes it's in a workspace when it's not"). **Fixed
    2026-11-21: all three new crates are workspace members now and
    `Cargo.lock` has entries for them.**
  - Deliberately NOT done: background-agent binary / Task Scheduler / tray
    (rest of Phase 7), desktop queue UI (Phase 8), LFS/submodules (Phase 9).

- **Phase 6 / Phase 7 verification (actual, 2026-11-21, on `main`
  `95340a0`, local merge `b55a10d`):** the user's Phase 6 + Phase 7
  code was pulled, merged, read end to end, and checked with the
  real toolchain. What is true on disk vs. what is not:
  - **Workspace (FAILED):** `crates/sync-queue`, `crates/ipc`,
    `apps/background-agent` are not workspace members; `Cargo.lock`
    has zero entries for them. `cargo check --manifest-path
    crates/sync-queue/Cargo.toml` and `.../crates/ipc/Cargo.toml`
    both fail with the workspace-membership error. `cargo test
    --workspace` silently skips all three crates.
  - **Existing code still green:** `cargo fmt --check` pass; clippy
    (`--workspace --exclude reponyx-desktop --all-targets`) 0
    warnings; `cargo test --workspace --exclude reponyx-desktop`
    **302 passed, 0 failed** — but that count covers only the
    pre-existing crates; no new-crate test has ever run.
  - **Typecheck (FAILED):** `pnpm -r typecheck` fails in
    `apps/desktop/src/routes/Agent.tsx` — it imports
    `agentHealth`, `ensureAgentRunning`, `agentPause`,
    `agentResume`, `registerAgentAutostart`,
    `unregisterAgentAutostart` from `@reponyx/api-client` and
    `AgentHealth` from `@reponyx/shared`, none of which exist.
  - **Agent compile error (found by reading):**
    `apps/background-agent/src/runtime.rs:59` calls
    `materialize_due(&pool)` with one argument; the function takes
    `(pool, now)` (`crates/scheduler/src/due.rs:243`). The desktop's
    own call passes `Utc::now()` (`schedules.rs:314`).
  - **Not wired:** desktop `lib.rs` declares no `mod agent` /
    `mod tray`; no agent commands in `generate_handler!`; no
    `/agent` route in `App.tsx`; desktop `Cargo.toml` has no
    `reponyx-ipc` / `reponyx-sync-queue` dependency; `tauri.conf.json`
    has no `externalBin` for the agent binary. `agent.rs`,
    `tray.rs`, `Agent.tsx` are unreachable dead code as committed.
  - **Retry budget (found by reading):** a claimed job row always
    executes with `attempt_count == 1` and a transient failure
    creates a brand-new row with `attempt_count = 0`, so the
    `max_attempts` guard never binds — transient failures retry
    forever. The computed backoff delay is also not attached to the
    job (no `available_at` column); only the failing worker sleeps.
  - **Missing vs. the Phase 6 spec (found by reading):** no
    heartbeat / lease / `worker_id` columns or code (no new
    migration); no queue-level `cancel` (IPC `CancelJob` only logs
    and returns `ok: true`); no global/per-account rate limiting in
    the queue; no priority aging; no `JobProgress` emission; IPC
    progress push unwired (`set_progress_source` has no callers);
    no per-repo exclusion at claim time (only at git-execution time
    via the git-engine lock).
  - **Agent is PAT-only (found by reading):** `runtime.rs` uses
    `DefaultCredentialResolver::pat_only`, so GitHub App accounts
    fail every job. `storage.root` is seeded as `''` in V1 and
    nothing in the desktop ever sets it, so a real `GitJobRunner`
    run fails with "storage.root is not configured" before
    touching git.
  - **Tests present but unrun and partial:** sync-queue has 13 tests
    (5 config/retry unit + 5 integration + 3 git-runner unit), ipc
    has 4 (frame + protocol roundtrips), background-agent has 0,
    desktop agent/tray/Agent.tsx have 0. The 100-job test uses one
    repository (not 100 repos) and never asserts the concurrency
    ceiling; no test covers concurrent claiming, same-repo
    exclusion, cancellation, rate limits, or IPC auth.
  - **Docs not updated by the landing commits:** no changes to
    `docs/`, `CHANGELOG.md`, or `AGENTS.md` in `adb21b9` /
    `b24acb1`. `AGENTS.md` still listed the new crates as "planned,
    not yet created". `ai/phase6/PLAN.md` and `ai/phase7/PLAN.md`
    status lines claim implementation/complete; this file is the
    source of truth for verified state.
  - **Also committed:** `apps/desktop/src-tauri/gen/schemas/*.json`
    (two 2,370-line generated Tauri schema files) — generated
    build artifacts in the repo.
  - **What unblocks completion:** add the three crates to the
    workspace (`members` + path deps) and regenerate `Cargo.lock`;
    fix the `materialize_due` arity; add the six agent functions to
    api-client, `AgentHealth` to shared, the `/agent` route, and
    declare the desktop modules; carry attempt counts across retry
    rows and enforce backoff on the job; decide rate limiting,
    cancel, heartbeats/leases, aging, `storage.root` provisioning,
    GitHub App support in the agent, and single-instance
    enforcement; then run the new crates' tests and re-verify.
  - Phase 6 and the Phase 7 remainder are therefore NOT complete
    as of this verification. The scheduler crate (Phase 7 partial,
    2026-11-19) remains the only verified-complete Phase 7 piece.

- **Phase 6 / Phase 7 fixes + re-verification (actual, 2026-11-21,
  assistant, on top of the user's code; uncommitted at the time of
  writing):** every issue in the verification above was fixed, in
  small steps, by editing the user's own code directly (no
  re-delegation of the build):
  - **Workspace (Fix 1):** root `Cargo.toml` `members` gained
    `apps/background-agent`, `crates/sync-queue`, `crates/ipc`;
    `[workspace.dependencies]` gained path deps for sync-queue and
    ipc; `Cargo.lock` regenerated with entries for all three.
    `cargo metadata` confirms 13 members. The new crates were
    written against an older rand API — fixed to rand 0.10
    (`RngExt` / `rand::rng().fill`, `compute_backoff` bound), the
    `tokio-util` `sync` feature (does not exist in 0.7) dropped in
    all three crates, a `read_exact` return-type fix, a `gen`
    reserved-keyword rename in `server.rs`, and a missing `.await`
    in `config.rs`.
  - **Agent arity (Fix 2):** `runtime.rs:59` now calls
    `materialize_due(&pool, Utc::now())`.
  - **V4 migration (Fix 5/6):**
    `migrations/sqlite/0004_queue_fields.sql` adds `available_at`,
    `worker_id`, `lease_expires_at`, `heartbeat_at`,
    `cancel_requested_at`, `dedupe_key` to `sync_jobs`, a partial
    UNIQUE index on `dedupe_key` for active jobs, and indexes on
    `available_at` / running leases. V1–V3 untouched.
  - **Retry budget (Fix 5):** `create_retry_job` carries
    `attempt_count` from the failed parent and sets `available_at`
    to now + the computed exponential backoff (full jitter); the
    executor computes the delay before the retry row is written
    (unique-violation on the dedupe index is tolerated as "already
    queued"). The old worker sleep was removed — backoff is now
    enforced on the job, not on the worker. A new test scripts 20
    transient failures with `max_attempts = 3` and asserts exactly
    3 failed rows with attempt counts [1, 2, 3] and 0 pending.
  - **Leases / heartbeats / cancel (Fix 6):**
    `claim_next_job(pool, worker_id, lease)` sets worker, lease
    (default 60 s), and heartbeat, and refuses jobs whose
    `available_at` is in the future; `heartbeat_job` renews only
    for the owning worker; `request_cancel_job` cancels pending
    jobs outright and marks running jobs for cooperative cancel;
    `defer_job` is the rate-limit path (pending again, attempt
    refunded). Crash recovery is lease-based
    (`lease_expires_at <= now`, with the old `started_at` grace
    only for rows without a lease) and `failed_only` is now actually
    counted. IPC `CancelJob` is real: the agent calls
    `request_cancel_job` and reports cancelled / cancel requested /
    already finished instead of a false success. `IpcClient` gained
    `cancel_job`; the desktop gained `agent_cancel_command`.
  - **Rate limiting + aging + progress (Fix 7):** new
    `crates/sync-queue/src/rate_limit.rs` — in-memory token buckets,
    global (default 120/min) + per-account (default 60/min);
    `QueueConfig` gained `lease_duration`, `global_jobs_per_minute`,
    `per_account_jobs_per_minute`. A worker whose claim is refused
    defers the job via `defer_job` and sleeps `min(wait, 5 s)`.
    Claim ordering applies priority aging (up to +3 effective
    priority after 3 h waiting, documented in `jobs.rs`).
    `JobProgress` events are now emitted: `git_runner` forwards git
    `ProgressEvent`s over the broadcast channel and emits
    `started`/`finished` stage events; the working-clone path no
    longer uses `never_cancel()` — it watches the job token.
    Worker panics no longer shrink the pool silently: execution
    runs in a child task and a panic fails the job with
    `WorkerPanic`. `QueueHandle.join` takes `&self` and logs join
    errors.
  - **Agent auth + storage (Fix 8):** the agent now uses
    `default_resolver_from_settings` (PAT from the OS store, GitHub
    App via the broker when a device is paired) instead of
    `pat_only`; paused accounts are refused at resolve time with a
    permanent error; `ensure_storage_root` seeds `storage.root` to
    `<data_dir>` on agent start (corrected 2026-11-21 — see the live
    run below; the first version seeded `<data_dir>/repositories`,
    which doubled the `repositories/` segment) so real syncs no
    longer fail before touching git; agent `generation` is unix
    millis.
  - **Desktop wiring (Fix 3/4):** desktop `Cargo.toml` depends on
    `reponyx-ipc`; `lib.rs` declares `mod agent; mod tray;`, sets up
    the tray (warns and continues without it on failure), and
    registers 9 agent commands; `agent.rs` gained camelCase serde
    renames + `agent_cancel_command`; `tray.rs` no longer unwraps
    the window icon. `packages/shared` gained `AgentStatus` /
    `AgentHealth` types + zod schemas; `packages/api-client` gained
    the 9 agent functions; `App.tsx` gained the `/agent` route and
    the sidebar gained an Agent entry. `externalBin` was deliberately
    NOT added to `tauri.conf.json` (tauri-build errors when the file
    is missing at build time; the agent resolves via same-dir /
    `REPONYX_AGENT_EXE` / PATH).
  - **Concurrency hardening found by the new tests:** the 100-job
    test failed with "database is locked" (no busy timeout + a
    deferred-transaction lock-upgrade deadlock in
    `complete_job_with_history`). Fixed with a 5 s busy timeout in
    `pool.rs` and an 8-attempt retry inside
    `complete_job_with_history` (read-back kept outside the retry
    so a committed job is never completed twice). The 100-job test
    now uses 100 repos (one job each) instead of 100 jobs for one
    repo.
  - **New tests (all passing):** database `jobs.rs` 12 (4 new:
    retry not claimable before `available_at`, priority aging,
    cancel pending/running, heartbeat owner-only); sync-queue
    rate-limit unit 3 (new module); queue_integration 8 (3 new:
    retry budget, rate limiter defers, lease-driven recovery).
  - **Verification (actual, 2026-11-21):** `cargo fmt --check`
    pass; clippy (`--workspace --exclude reponyx-desktop
    --all-targets`) pass, 0 warnings (5 warnings found and fixed
    during the run: `map_or` → `is_some_and`, redundant closure,
    identity `map_err`, 9-arg worker function bundled into a
    `WorkerArgs` struct, unused imports, `len() > 0` →
    `!is_empty()`); `cargo test --workspace --exclude
    reponyx-desktop` **328 passed, 0 failed** (was 302); `pnpm -r
    typecheck` pass; `pnpm --filter @reponyx/desktop build` pass
    (1.76 s); desktop eslint pass. Tauri binary build still blocked
    on this VM by missing `gdk-3.0` / WebViewGTK system libraries —
    not claimed passed.
  - **Honest remaining gaps (not overclaimed):** desktop Rust
    (agent/tray/lib wiring) is verified by `cargo fmt` + review
    only on this VM — the real compile check is the user's Windows
    build. IPC progress push is still not wired end-to-end to the
    desktop UI (`set_progress_source` has no callers;
    `ServerMessage::Progress` never sent) — the queue now emits
    `JobProgress` events, but forwarding them over IPC is not done.
    No `queued`/`retry_wait` lifecycle states (retry state is
    `available_at` on a pending row). No per-repo exclusion at claim
    time (serialization still happens at git-execution time via the
    git-engine lock). No single-instance enforcement for the agent.
    `windows_task.rs` schtasks helpers still have zero callers (the
    desktop does its own autostart registration). The generated
    `apps/desktop/src-tauri/gen/schemas/*.json` files remain
    committed. The Phase 6 spec's 12 acceptance tests are only
    partially covered by automated tests (no real-GitHub or Windows
    runs on this VM — but see the live run below: the full chain has
    now run for real on this VM against a local bare repo).

- **Live end-to-end run (actual, 2026-11-21, assistant, on this
  VM — NOT a unit test):** at the user's request ("sb ko apna system
  pe chla k dekho 1 dafa"), the real binaries were run for real:
  - **New E2E test** `crates/sync-queue/tests/e2e_live_chain.rs`
    (`scheduler_to_queue_to_real_mirror_clone`): seeds a local bare
    git repo (2 commits) as the remote, a PAT account, a repo row
    with a `file://` clone URL, and a due global schedule; asserts
    `materialize_due` creates one `MirrorClone` job, the worker pool
    runs it with the real `GitJobRunner`, and the mirror on disk
    carries the remote's HEAD and `refs/heads/main`, with the repo
    row's `local_path` set and a history row recorded. Passes in
    ~0.2 s. No network, no real GitHub credentials (static fake PAT
    source; `file://` needs no auth).
  - **Real agent binary** (`reponyx-agent`, debug build): run for
    45 s against a seeded data dir (account + repo + due schedule
    inserted by SQL; PAT stored in a throwaway gnome-keyring
    Secret Service session — the VM had no OS credential store
    until `gnome-keyring` was installed for this run). The agent
    started, migrated the DB, provisioned `storage.root`, ran crash
    recovery, started 5 workers, bound IPC
    (`ipc/agent.sock`), materialized 1 job from the due schedule,
    and completed a real `mirror_clone` in ~50 ms; history row
    `mirror_clone / success`; schedule `next_run_at` advanced;
    graceful shutdown on SIGTERM. **One real bug found by this run:**
    the mirror landed at
    `<data>/repositories/repositories/github/...` — `ensure_storage_root`
    defaulted `storage.root` to `<data_dir>/repositories` while
    `RepositoryLayout` appends `repositories/github/...` under the
    product root (spec: `D:\RepoNyx\repositories\github\...`). Fixed
    the same day (`lifecycle.rs`: default is now `<data_dir>` itself)
    and re-verified with a fresh data dir: mirror at
    `<data>/repositories/github/octocat/octocat/hello.git`, HEAD
    identical to the remote, no doubled segment.
  - **Real broker binary** (`reponyx-auth-broker`, debug build):
    run with a throwaway RSA key + App id + setup token (all in
    `/tmp`, never committed). `/health` returned
    `{"status":"ok","version":"0.1.0"}`; a pairing request with the
    correct setup token passed auth and was correctly refused with
    "installation not recorded; install the GitHub App first" (no
    real App exists); a wrong token got 401 "invalid or missing
    setup token".
  - **What this run did NOT prove:** no real GitHub account, App,
    or PAT was ever involved (the remote was a local bare repo, the
    PAT was fake); the desktop UI + Tauri backend never ran (the
    Tauri binary build is still blocked on this VM by the missing
    WebViewGTK libs); IPC progress push to the UI is still unwired.
    The remaining real proof is the user's Windows build + a first
    sync against a real GitHub account.
  - **Follow-up fix from the live-run review (2026-11-21):**
    `is_unique_violation` in `crates/database/src/jobs.rs` only
    checked `err.message`, but `map_error` rewrites a UNIQUE
    violation's message to a friendly string and puts the raw SQLite
    text in `details` — so the helper could never match, and the
    executor's / recovery's "already queued" tolerance was dead code
    under contention. Fixed to check `details` too (same pattern as
    `is_locked`), with a regression test
    (`duplicate_retry_row_is_a_unique_violation`) that fails without
    the fix and passes with it. Uncommitted — needs a push.
  - **Verification after the fix:** `cargo fmt --check` pass;
    clippy 0 warnings; `cargo test --workspace --exclude
    reponyx-desktop` **331 passed, 0 failed**; `pnpm -r typecheck`
    pass; desktop vite build pass; desktop eslint pass.

- **Phase 5 — Authentication (actual, 2026-11-18, on `main`):**
  - **Auth Broker (`apps/auth-broker`, axum 0.8.9):** a separate
    binary crate — NOT a dependency of the desktop crate
    (enforced by `scripts/check-no-private-key.sh`). Holds the
    GitHub App private key exclusively: loaded at startup from
    `REPONYX_BROKER_PRIVATE_KEY` (PEM text) or
    `REPONYX_BROKER_PRIVATE_KEY_PATH` (file, read once); no
    default key, no committed key file; refuses to start without
    the App id, the key, and a setup token of ≥ 16 chars
    (`REPONYX_BROKER_SETUP_TOKEN`). Binds loopback
    (`127.0.0.1:8765`) by default. Endpoints: `GET /health`;
    `POST /setup/state` (one-time CSRF `state` + install URL);
    `GET /github/callback` (validates `state` — single-use,
    10-minute TTL — records the installation); `POST
    /pairing/request`, `POST /pairing/confirm`, `GET
    /pairing/status`, `POST /pairing/revoke`; `POST
    /installations/{id}/token` (mint); `GET /installations`;
    `GET /audit`. Errors are `{ error: { code, message } }` with
    `RepoNyxError` codes; rate limits return 429 with
    `Retry-After`.
  - **Pairing design (the choice the plan left open):** the
    pairing request is authenticated by the broker setup token
    (operator-supplied at broker startup, stored in the desktop's
    OS credential store, presented as `X-RepoNyx-Setup-Token` and
    compared by SHA-256 hash). The pairing code is 9 characters
    from an unambiguous alphabet (no 0/O/1/I), shown as
    `XXX-XXX-XXX` (~45 bits), single-use, 10-minute TTL, max 5
    wrong-device attempts then invalidated, revocable, audited,
    bound to the requesting device identifier. On confirm the
    broker issues a random 32-byte device token, stored server-side
    only as a SHA-256 hash; the desktop stores the raw token in
    the OS credential store and presents it as `Authorization:
    Bearer <device_token>` when minting. Broker state is in-memory
    (installations, codes, `state` values, device tokens, token
    cache, audit log); a restart forgets everything and devices
    must re-pair — durable broker storage is a deployment-phase
    hardening item.
  - **Token minting (`tokens.rs`):** App JWT, RS256 (`jsonwebtoken`
    12.0.0 with the `rust_crypto` feature), `iss` = App id, `iat`
    = now − 60 s, `exp` = now + 9 min; `POST
    /app/installations/{id}/access_tokens`; minted tokens cached
    per installation until 5 minutes before expiry;
    `force_refresh` bypasses the cache (used after a 401).
  - **Rate limiting:** hand-rolled per-key token buckets (no extra
    dependency): minting 30/min per device, pairing requests
    10/min per IP, pairing confirms 20/min per IP; pairing codes
    have their own 5-attempt lockout on top.
  - **Audit:** append-only, in-memory: pairing
    requested/confirmed/revoked/failed, token minted/failed,
    callback received/failed, auth failures. Events carry
    timestamp, actor (device id, `setup`, or client IP), action,
    outcome, detail — never a secret. Tests prove a minted token,
    device token, and pairing code appear in neither the audit
    output nor the broker logs (logs carry method/path/status and
    ids only).
  - **Credential-store crate (`crates/credential-store`):**
    `CredentialStore` trait (store/get/delete/exists; listing is
    explicitly unsupported for OS backends — it fails loudly
    rather than pretending the store is empty). OS backend via
    `keyring` 4.2.0 (Windows Credential Manager / macOS Keychain /
    Linux Secret Service); in-memory backend for tests/CI.
    `SecretString` zeroes on drop and redacts in `Debug`.
    Services: `reponyx:pat` (account = account UUID),
    `reponyx:broker` (`setup-token`, `device-token:<id>`).
    PAT flow (`pat.rs`): format validation (`github_pat_` /
    `ghp_`, before any network call), `add_pat_account`
    (validate via Phase 4 `validate_auth` → store → create
    account with `authentication_method = fine_grained_pat`;
    duplicate identity rejected; the PAT is provably absent from
    the database — a test scans every column of the accounts row),
    `replace_pat` (re-validate, same GitHub identity required),
    `remove_pat` (delete from store + mark account revoked; the
    row stays — local data is never deleted), `reconnect_pat_account`
    (only the explicit reconnect path returns an account to
    `connected`), `validate_pat_account` (refreshes
    `last_validated_at`), `pat_expiry_status` (7-day warning
    threshold; `days_left` rounds up — see bugs). `PatTokenSource`
    implements the Phase 4 `CredentialSource` (username = GitHub
    login, `expires_in_secs` from the stored expiry when known).
    File-backed encrypted storage is out of scope, stated in the
    crate docs.
  - **V3 migration (`migrations/sqlite/0003_pat_expiry.sql`):**
    nullable `pat_expires_at TEXT` on `accounts` (metadata, not a
    secret); `Account.pat_expires_at` in `crates/core`;
    `set_pat_expiry` / `mark_validated` in `crates/database`.
    V1/V2 untouched.
  - **Phase 4 `CredentialSource` implementations
    (`crates/github-provider/src/broker.rs`):** `BrokerClient`
    (pairing + minting over HTTP; secret-free — setup/device tokens
    are passed per call) and `BrokerTokenSource` (mints via the
    broker, caches until 5 min before expiry, drops the cache on
    `invalidate()`). The trait gained a default no-op
    `invalidate()` method; `GitHubClient.get_json` now refreshes
    once after a 401 and retries a single time, then fails fast
    (this changed two Phase 4 test contracts — see bugs). Mixed
    auth proven by a test: a PAT provider and a broker provider
    both validate against the same mocked GitHub API through the
    `GitProvider` trait; an end-to-end test runs a real broker
    (throwaway key) + `BrokerTokenSource` + `GitHubProvider`
    against the mocked API and verifies the presented JWT (RS256
    signature, `iss`, 9-minute lifetime).
  - **Desktop (`apps/desktop/src-tauri/src/auth.rs`, 17 Tauri
    commands + `AuthState`):** PAT add/replace/remove/reconnect/
    validate, PAT expiry status, broker setup-token save, broker
    status (configured/reachable), broker installations, pairing
    start/complete (creates/reconnects the account row and mirrors
    the installation locally), GitHub App validate, device
    revocation. Commands return `{ code, message }` with the
    message passed through `redact_secrets`. The device identifier
    is a UUID stored once in the settings table. DTOs are camelCase
    to match the TS types; core structs stay snake_case. **The
    desktop crate cannot be compiled on this VM (gdk blocker), so
    these commands are verified by `cargo fmt` + review here, not
    by a build — stated honestly.** Frontend
    (`apps/desktop/src/routes/Accounts.tsx`, wired into `App.tsx`
    in place of the Phase 5 placeholder): account list with status
    badges and PAT expiry warnings, add-PAT form, validate /
    reconnect / replace / remove actions, broker setup-token form,
    installations + pairing flow with the code shown for
    confirmation. New zod schemas + typed api-client wrappers in
    `packages/shared` / `packages/api-client` (`zod` added as a
    direct dependency of api-client).
  - **Private-key scan (`scripts/check-no-private-key.sh`):**
    greps desktop sources for key markers and broker-key
    references, verifies the desktop crate does not depend on the
    broker (Cargo.toml + `cargo tree`), checks for committed key
    files, and scans a built desktop binary when one exists.
    **Result: PASSED on sources + dependency tree; binary part
    SKIPPED with an explicit note** — no desktop binary can be
    built on this VM (WebViewGTK blocker). Full binary scan needs a
    machine where `cargo tauri build` works (Phase 10).
  - **Dependencies added (all latest stable, verified on
    crates.io 2026-11-19):** `axum` 0.8.9, `jsonwebtoken` 12.0.0
    (feature `rust_crypto`), `keyring` 4.2.0, `rand` 0.10.4,
    `sha2` 0.11.1, `zeroize` 1.9.2 (feature `derive`),
    `rsa` 0.10.0 (dev-only, throwaway test keys),
    `pkcs8` 0.11.0 (dev-only, PEM encoding); `reqwest` 0.13.5
    gained the `query` feature; `tempfile` 3.27.0 moved to
    workspace dependencies.
  - **Bugs found and fixed during Phase 5 (caught by the new
    tests):** (1) `pat_expiry_status` used `num_days()` (truncates)
    so a token stored 3 days out reported 2 days left — now rounds
    up; (2) `jsonwebtoken` 12 panics without a crypto provider —
    the `rust_crypto` feature must be enabled explicitly; (3)
    `zeroize` derive macros need the `derive` feature (was only
    enabled accidentally via feature unification before);
    (4) `rsa` 0.10 has no `pem` feature — PEM encoding comes from
    the `pkcs8` crate; (5) rand 0.10 moved `random_range`/`fill`
    to the `RngExt` trait; (6) `keyring` 4 has no
    `update_password` — `set_password` upserts; (7) sha2 0.11's
    `finalize()` no longer implements `LowerHex` — hex is formatted
    manually; (8) `ErrorCode::NotImplemented` does not exist in
    core — the broker's status mapping drops that arm;
    (9) `chrono::Duration::from_secs` does not exist — `Duration::seconds`;
    (10) `i64::div_ceil` is still unstable — manual ceiling;
    (11) test PATs used the `test-pat-…` prefix, which the
    format validator (correctly) rejects — tests now use
    realistic-format obviously-fake PATs (`ghp_0000…0001` etc.).
  - **Behaviour change from Phase 4 (deliberate):** the client
    refreshes once after a 401 and retries a single time. The
    Phase 4 test `unauthorized_is_authentication_expired_and_not_retried`
    now expects 2 requests (renamed to
    `unauthorized_refreshes_once_then_fails_as_authentication_expired`),
    and the PAT 401 test expects 2 attempts. Both assert the
    failure is terminal — no infinite retry.
  - **Deliberately NOT done in Phase 5 (approval gates):** no
    GitHub App published or created; no production credential
    created or modified; the Auth Broker was not deployed anywhere
    (binds loopback); no real PAT/token/private key was used —
    every test credential is obviously fake or a throwaway key
    generated in-memory.
  - **Verification (actual, 2026-11-18):** `cargo fmt --check`
    pass; clippy (`--workspace --exclude reponyx-desktop
    --all-targets`) pass, 0 warnings (4 collapsible-`if`
    warnings found and fixed during the run); `cargo test
    --workspace --exclude reponyx-desktop` **262 passed, 0
    failed** (auth-broker 22 — lib 7, broker integration 15;
    core 11; credential-store 20 — lib 6, mixed_auth 1, pat_flow
    10, store 3; database 55; git-engine 87; github-provider 63
    — lib 26, broker_source 5, discovery 8, errors 8,
    installations 4, rate_limit 4, redaction 6, sync 2;
    logging 4; provider-abstractions 0); private-key scan pass
    (binary part skipped, see above); `pnpm -r typecheck` pass
    (Node 26.13.1); `pnpm --filter @reponyx/desktop build` pass
    (2306 modules, 1.95 s); desktop eslint pass. One transient
    failure during verification: the untouched Phase 3 test
    `healthy_mirror_and_working_clone` failed under full parallel
    load (`Locked` instead of `Healthy`) — once during the build,
    twice more during independent verification (2 failures in ~8
    full-workspace runs; never in isolation, never on re-run). Fixed
    in a follow-up commit (test only): the test retries `verify` up
    to 3 times while transiently `Locked`; no production code
    changed. Tauri binary
    build still blocked on this VM by missing `gdk-3.0` /
    WebViewGTK system libraries — not claimed passed.
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

### Phase 5 — 2026-11-18 (Authentication, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check | Result |
| --- | --- |
| cargo fmt --check | pass |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings (4 collapsible-`if` warnings found and fixed during the run) |
| cargo test --workspace --exclude reponyx-desktop | **262 passed, 0 failed** — auth-broker 22 (lib 7, broker integration 15), core 11, credential-store 20 (lib 6, mixed_auth 1, pat_flow 10, store 3), database 55, git-engine 87, github-provider 63 (lib 26, broker_source 5, discovery 8, errors 8, installations 4, rate_limit 4, redaction 6, sync 2), logging 4, provider-abstractions 0 |
| scripts/check-no-private-key.sh | **pass** on desktop sources + dependency tree; **binary part SKIPPED** — no desktop binary can be built on this VM (WebViewGTK blocker) |
| pnpm -r typecheck | pass (shared, ui, api-client, desktop) |
| pnpm --filter @reponyx/desktop build (vite) | pass (2306 modules, 1.95 s) |
| pnpm --filter @reponyx/desktop lint (eslint) | pass |
| cargo check -p reponyx-desktop | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1 (desktop auth commands verified by `cargo fmt` + review only) |

One transient failure during verification: the untouched Phase 3
test `healthy_mirror_and_working_clone` failed once under full
parallel load and passed 3/3 in isolation and on the full-suite
re-run (262 passed, 0 failed) — recorded as a flake, not fixed here.

### Phase 7 partial — 2026-11-19 (scheduler crate only, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check | Result |
| --- | --- |
| cargo fmt --check | pass |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings (1 collapsible-`if` + 1 unused-workspace-dependency warning found and fixed during the run) |
| cargo test --workspace --exclude reponyx-desktop | **302 passed, 0 failed** — scheduler 40 new (lib 8, dst 5, materialize 10, next_run 14, precedence 3); auth-broker 22, core 11, credential-store 20, database 55, git-engine 87, github-provider 63, logging 4, provider-abstractions 0 |
| cargo tree -p reponyx-scheduler | core + database only (chrono, chrono-tz, serde, serde_json, sqlx, uuid) — no git-engine, no provider, no queue |
| pnpm -r typecheck | pass (shared, ui, api-client, desktop) |
| pnpm --filter @reponyx/desktop build (vite) | pass |
| pnpm --filter @reponyx/desktop lint (eslint) | pass |
| cargo check -p reponyx-desktop | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1 (desktop schedule commands verified by `cargo fmt` + review only) |

No flakes observed in the full-suite runs during this work.

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
- **Web preview (plain browser) showed a cryptic error (fixed
  2026-11-19, unreleased):** opening the Vite dev server at
  localhost:1420 in a plain browser made the Accounts and Overview
  pages show `Cannot read properties of undefined (reading
  'invoke')`, and the Git / Auth Broker checks spun on "checking…"
  forever. Cause: `invoke` reads `window.__TAURI_INTERNALS__`,
  which only exists inside the Tauri webview. Fix: every api-client
  call goes through a guard that throws a clear "Tauri backend is
  unavailable in the web preview" message instead; Overview and
  Accounts load their data sources independently and resolve a
  failed check to an "unavailable" state. The Repositories
  placeholder (which claimed Phase 4 — the backend is done, only the
  page is Phase 8) and Overview's stale "Next steps" text were
  corrected at the same time.

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
5. ~~Phase 5: Authentication (`ai/phase5/PLAN.md`) — awaiting the
   user's go-ahead.~~ **Done 2026-11-18** (see "Completed tasks" above).
6. ~~Phase 6: Sync Queue (`ai/phase6/PLAN.md`) — code landed on
   `main` 2026-11-21 (user) but NOT verified complete.~~ **Fixed +
   verified 2026-11-21** (see "Phase 6 / Phase 7 fixes +
   re-verification" above): workspace membership, retry budget +
   backoff on the job, leases/heartbeats/cancel, rate limiting,
   priority aging, `JobProgress` emission, 26 new tests, full suite
   328 passed / 0 failed. Remaining gaps: IPC progress push to the
   UI, per-repo exclusion at claim time, single-instance
   enforcement.
7. ~~Phase 7: Scheduler and Background Agent — scheduler crate done
   2026-11-19 (verified). Agent, IPC, tray, and Agent page code
   landed 2026-11-21 but are unwired and uncompilable as
   committed.~~ **Fixed + verified 2026-11-21** (same section):
   `materialize_due` arity, desktop modules + 9 commands + `/agent`
   route, agent resolver (PAT + GitHub App), `storage.root`
   provisioning. Desktop Rust still needs the user's Windows build
   to compile-check (gdk blocker on this VM); the agent binary is
   resolved via same-dir / `REPONYX_AGENT_EXE` / PATH, not
   `externalBin`.
8. On a machine with the WebViewGTK dev libraries (or Windows):
   re-run `cargo check -p reponyx-desktop` and `cargo tauri build`.
