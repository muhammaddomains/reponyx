# RepoNyx — Project Status

Last updated: 2026-11-23 (Phase 10 F3 key-rotation runbook done —
see "Phase 10 — F3" below; Phase 8 now 100% complete — see
"Phase 8 — Completion" below; Phase 10 started — see "Phase 10 —
Security audit (first pass)" below; Phase 9 performance pass built
— see "Phase 9 — Performance pass" below; Phase 9 now complete;
CI on `0c1112f` fully green; Phase 9 import/export built — see
"Phase 9 — Import/export" below;
Phase 9 diagnostics export + DB backup built — see "Phase 9 —
Diagnostics export + DB backup" below;
storage management + orphan detection built the same day;
submodules + Both-mode completion built the same day; LFS sync
built the same day; Phase 8 Settings page built — all 6 pages
done; Schedules + Repositories + Synchronization + Activity +
Storage pages done the same day; Phase 6 / Phase 7 fixes
verified + live end-to-end run on this VM — see "Phase 6 /
Phase 7 fixes + re-verification" and "Live end-to-end run" below)

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
**Phase 8 — Desktop UI — started 2026-11-21, complete 2026-11-23:
all six pages built (Schedules, Repositories, Synchronization,
Activity, Storage, and Settings) plus the three remaining items —
notifications, frontend tests, and true list virtualization — and
tray-menu polish (see the "Phase 8 — Completion" section below).**
**Phase 9 — Advanced Features — started 2026-11-21: LFS sync +
verification built, then submodule sync + Both-mode completion,
then storage management + orphan detection, then diagnostics
export + DB backup (see the "Phase 9" sections below).**
The desktop Rust wiring is verified by `cargo fmt` + review
only (the gdk blocker); the real desktop compile check is the
user's Windows build. Remaining honest gaps are listed in the
fixes section. Work is on `main` going forward.

## Production audit + Phase 1 unified lifecycle (2026-11-25)

The user ordered a full A-to-Z production audit first, then a
report, then a plan, fixes only after the plan — and set the
governing principle: RepoNyx runs as ONE system (broker + agent +
desktop start together and stop together, in dev AND in the
build); the fix is unified lifecycle, NOT degrading functions so
a page works without a missing service. Full report:
`~/workspace/goals/reponyx-development/files/reponyx-production-audit-report.md`.
Verdict: not production-ready — 10 unique blockers, ~30 majors,
~40 minors. Plan approved by the user 2026-11-25 02:20 MST;
Phase 1 (unified lifecycle) built the same day, uncommitted at
the time of writing (local `main` at merge `4927b1a`).

Phase 1 delivered: desktop starts agent + broker at startup and
stops both on quit (new `broker.rs` + `exe.rs` in the desktop
crate; `RunEvent::ExitRequested`/`Exit` handler; agent spawn
keeps the `Child` so an early exit is reported, not waited out);
broker already-running detection via `/health` pre-bind
(second `pnpm broker` exits 0); agent lock-first startup
(`agent_lock.rs` in `crates/ipc`, incl. the `pid_alive(u32::MAX)`
`kill(-1, 0)` fix); IPC 5 s request timeout; `agent_health`
reports a wedged agent as unreachable; autostart registers the
desktop on Windows (`RepoNyx`, legacy agent entries removed);
installer bundles agent + broker (`bundle.resources`, release
binaries built before the tauri-action step); Accounts page
broker-down safety net (accounts still render + broker warning).
Verified 2026-11-25: `cargo fmt --check` OK, clippy (workspace
excl. desktop) clean, `cargo test --workspace --exclude
reponyx-desktop` 433 passed / 0 failed (76 binaries; the broker
flake did not recur), typecheck clean, frontend tests 73 passed /
0 failed, `pnpm -r lint` clean, vite build OK, Semgrep (exact CI
command) 0 findings. Honest limits: desktop Rust is
compile-checked only by CI windows-build; installer bundling is
proven only by a real Windows `tauri build` / the release
workflow; a mid-job quit can still orphan git children until
Phase 2's process-group kill (G2). Next: Phase 2 crash recovery
& data integrity (incl. the broker flake test fix).

## Phase 2 crash recovery & data integrity (2026-11-25)

Built the same day Phase 1 was pushed, on the user's one-word
"phase 2". Closes blockers B5–B10: partial clones move aside and
re-clone (never deleted) with stale-lock cleanup and working-clone
verification; the queue sweeps stranded jobs every 60 s and
recovery is per-job fault-isolated (one bad job no longer aborts
the pass; a startup recovery failure no longer takes the agent
down); cancelling a sync kills the whole git process tree
(process groups via `nix` on Unix, `taskkill /T` on Windows);
`open_database` writes a `VACUUM INTO` pre-migration backup
(newest 3) and runs `PRAGMA integrity_check` before migrating,
failing loudly with the path and the restore instruction; the
Accounts command requires the DB (`require_db`) and list
skips bad rows; dashboard counts propagate errors; one
data-directory rule lives in `reponyx-database` and all three
processes delegate to it; `storage.root` changes move the
archives under the repo lock with a summary; a fetch against a
missing archive enqueues the re-clone; missing git-lfs surfaces
as a `job_warning` on the Synchronization page.
Verified 2026-11-25: `cargo fmt --check` OK, clippy (workspace
excl. desktop) clean, `cargo test --workspace --exclude
reponyx-desktop` 449 passed / 0 failed (64 suites; the broker
flake recurred once in a full-suite run — see below), typecheck
clean, frontend tests 73 passed / 0 failed, `pnpm -r lint`
clean, vite build OK, Semgrep (exact CI command) 0 findings,
`cargo audit` clean (nix 0.32 added for the process-group kill).
Honest limits: desktop Rust is compile-checked only by CI
windows-build; the storage migration and the LFS warning are
verified by unit/integration tests only, not on Windows yet.
Correction: during Phase 2 verification the agent deleted the
whole `crates/database/tests/` directory (13 integration-test
files) while removing a probe test file — caught by the test
count dropping 433 → 369, restored from HEAD, count back to 449. The broker flake test (`broker_logs_never_contain_a_token`)
still failed once in a full-suite run after the first
(`current_thread`) fix, so the test was rewritten: the capture
is now the process-global default and the mint uses a unique
installation id — deterministic by construction, not by thread
luck; it must survive repeated full-suite runs before the flake
is called fixed.

## Phase 3 security hardening (2026-11-25)

Built the same day Phase 2 was pushed, on the user's "now start
phase 3". Closes audit A1–A4, A7, B1–B3, B5, B6, C2, C3 and F2
(pipe ACL). Device tokens now expire after 90 days
(`device_token_ttl`, migration 0003 backfills pre-existing
tokens with creation + 90 days — verified with sqlite3 on a
scratch DB: 2026-11-01 → 2027-01-30) and re-pairing revokes the
device's previous tokens (one live token per device; the mint
gate test was rewritten to the new semantics — the token
confirmed for the LATEST pairing mints for any installation
paired to the same device, the revoked one 401s). Re-installing
the GitHub App preserves pairing (`record_installation` carries
`paired_device` forward). Setup-token auth locks out after 10
consecutive failures for 15 minutes (429 + Retry-After; in
memory only — a broker restart resets it; failures are not
audited, only the lockout transition); a non-loopback bind
refuses to start without `REPONYX_BROKER_ALLOW_NON_LOOPBACK=true`
(verified live with a throwaway key: 0.0.0.0 refused, opt-in
listens, 127.0.0.1 unaffected). The install callback is
rate-limited per IP before any audit write and the installation
is verified against GitHub's `GET /app/installations` for this
App before recording (forged callbacks 400; GitHub failure fails
closed 502). `/setup/state` is rate-limited per IP; pairing
status is POST-only (code in the JSON body, never a URL query).
The agent's Windows named pipe is created with a DACL granting
access only to the current user and SYSTEM (fails loudly if the
descriptor cannot be built — never a world-accessible fallback);
a Windows test asserts exactly those two trustees. Setup tokens
and IPC tokens are compared in constant time (SHA-256 digest +
`subtle::ConstantTimeEq`); broker secrets live in `SecretString`
(redacted Debug, zeroized on drop — moved to `reponyx-core` so
the broker does not depend on the credential-store crate);
`reponyx_logging` registers exact secret values for redaction
(the broker registers its setup token + private key), so a
prefix-less secret appearing bare in a log line is redacted;
the broker warns on Unix when its `.env` is group/other
readable. IPC accepts at most 16 connections and closes idle
connections after 10 minutes (the desktop client is
request-scoped, so legitimate use is unaffected).
Verified 2026-11-25: `cargo fmt --check` OK, clippy (workspace
excl. desktop) clean, clippy for `reponyx-ipc` on the Windows
GNU target clean (MinGW cross-check — see AGENTS.md; the MSVC
target cannot run on this VM), broker suite 25 passed / 0
failed (8 new Phase 3 tests; the 2 failures on the first run
were test bugs — mint-request counting now filters by path, and
the multi-installation test follows A1 revocation), full
workspace suite **460 passed / 0 failed in 3 consecutive runs**
(baseline 449; +12 new tests incl. the 8 broker ones), typecheck
clean, frontend tests 73 passed / 0 failed, `pnpm -r lint`
clean, vite build OK, `cargo audit` clean (0 vulnerabilities,
7 no-fix warnings). **Semgrep caught the new file first:**
`pipe_security.rs` drew 24 blocking findings — one per `unsafe`
block, the repo's SAST gate flags every `unsafe`. Win32 FFI has
no safe wrapper for these calls, so the file now carries a
per-call `// nosemgrep:
rust.lang.security.unsafe-usage.unsafe-usage` on each of the 24
sites (the repo's established convention, same as exe.rs), with
a module-level note explaining why; re-run is **0 findings**.
Honest limits: the pipe-DACL test is Windows-only and has not
run anywhere yet (only the GNU cross-check + CI's `cargo check`
on the desktop crate cover it); the setup-token lockout is
in-memory and resets on restart; B4 (pairing codes never expire)
is a documented accepted trade-off and was NOT changed; the
user's live device token (created before this change) expires 90
days after its creation — re-pairing is the renewal path.

## Phase 4 release readiness (2026-11-25)

Built the same day Phase 3 was pushed, on the user's "start next
phase 4". Closes audit E2 and E3; CHANGELOG v0.1.0 prepared; E4
(signing secrets) is NOT verifiable from code and still needs the
user's confirmation before the first release.

- **CI test gate on tags (E2):** `ci.yml` now runs on `v*` tags
  as well as pushes and pull requests, and `release.yml` has a
  `test` job (Windows: full Rust suite, frontend suite, desktop
  crate compile) that gates the `release` job — a tag can no
  longer ship untested code, even if CI was skipped or is still
  running.
- **CI installer + portable ZIP build:** a new `windows-installer`
  job in CI runs the full `tauri build` (NSIS + MSI) and builds
  the portable ZIP, uploading both plus `latest.json` as
  artifacts. It signs with a throwaway keypair generated in the
  job (`tauri signer generate --ci`, empty password); the real
  key never reaches CI. That proves packaging and updater-artifact
  generation, not the update flow — the throwaway key does not
  match the public key baked into the app.
- **Updater wired (E3):** Settings → Advanced → Updates now
  checks for updates (`check()` from `@tauri-apps/plugin-updater`),
  shows the release notes, and downloads + installs (`download()`
  then `install()`; installing restarts the app). Before this,
  the plugin was configured (endpoint + minisign public key) and
  registered in the backend, but nothing ever called it. The
  public key in `tauri.conf.json` was verified to decode as a
  well-formed minisign Ed25519 key (key id `d54eef67f4ed1092`).
  There is deliberately no automatic check on startup yet: an
  update found at startup would have nowhere visible to go (no
  global notice surface — that is Phase 5), so checks are manual
  until then. The end-to-end procedure (install v0.1.0 → publish
  v0.1.1 → update in place) is documented in
  `docs/qa/updater-e2e.md`; it needs a real Windows machine and
  the real signing key, so until it passes the updater is "wired,
  not proven" and the release notes say so.
- **`storage.root` validated against the disk:** the Settings
  page now rejects a root that is relative, missing, not a
  directory, not writable (a probe file is created and removed —
  permissions bits alone lie on Windows), a drive root, or the
  data directory / containing it / inside it
  (`validate_storage_root` in `crates/database/src/config.rs`,
  checks on the canonical path). The config import keeps the
  portable absolute-path check on purpose — a config file is
  cross-machine, and its root may not exist on the importing
  machine. Re-saving the same folder under a different spelling
  (symlink, `..`, trailing slash) no longer triggers a pointless
  migration (canonical comparison in `set_setting_command`).
  Clearing the field no longer silently saves the data directory
  (that value is now rejected) — a "Reset to default" button
  clears the setting via a new `clear_setting_command` instead,
  migrating archives back to the data directory.
- **CHANGELOG v0.1.0:** the whole `[Unreleased]` history is the
  first release, so it is now `## [0.1.0]` (date set when the
  `v0.1.0` tag is cut), with a fresh empty `[Unreleased]` above
  it and the Phase 4 entries at the top of the 0.1.0 section.
- **Not in this phase (Phase 5):** E5–E8 (agent/broker build
  ordering in the release draft, SBOM/attestation, the unused
  CSP origin `https://api.reponyx.com`, CHANGELOG-tag check) and
  the remaining polish minors.

Verified 2026-11-25: `cargo fmt --check` OK, clippy (workspace
excl. desktop) clean, full workspace suite **462 passed / 0
failed** (baseline 460; +2 new storage-root tests incl. a
Unix-only symlink test), typecheck clean, frontend tests **78
passed / 0 failed** (baseline 73; +4 updater tests, +1
clearSetting invoke test), `pnpm -r lint` clean, vite build OK,
Semgrep (exact CI command) **0 findings**, `cargo audit` clean
(0 vulnerabilities, 7 no-fix warnings). The desktop crate
(`settings.rs` changes + the new command) is compile-checked
only by CI's windows-build job. Honest limits: the updater E2E
has not run anywhere yet (needs the first release + Windows);
the CI installer job has not run yet either (first run is on the
push of this commit); E4 is open until the user confirms the
repo secrets.

## Phase 5 polish (2026-11-26)

Built on the user's "Haan, Phase 5 shuru karo". Closes the audit's
Phase 5 line: loading states, stale banners, swallowed catches,
autostart validation, spawn-path version check, and the remaining
minors (DB-m, ST-m, Q6–Q10, G6–G10, E5–E8, F11–F16).

- **Frontend (F9/F12/F13):** Agent page badge shows "Checking…"
  until the first poll settles and keeps last-known health on a
  failed poll; Overview shows "…" while counts load, has a Retry
  button that clears the error on success, and its stale "Phase 1
  foundation" / "Phase 8" text is gone; Synchronization and
  Activity poll loops clear their error on the next success.
  (F10 Accounts loading state and F11 swallowed catches were
  already done — verified, no change.)
- **Queue (Q6–Q10):** status predicates in the UPDATEs,
  single-statement pending cancel, uncapped priority aging, an
  overall job deadline (a slow job fails `OperationTimedOut` and
  retries), and a 60s rate-limit defer sleep.
- **Git (G7–G10):** diagnostic-line-only failure
  classification, partial submodule sync fails the job, canonical
  lock-file paths, and HEAD-before-index.lock health ordering
  with move-aside advice. (G6 token-in-env is an accepted
  same-user boundary, no change.)
- **DB (DB-m1–m6):** migration `0005_phase5_indexes`; the
  DB-m2 repair backfills unrecorded migration records from the
  embedded checksums; NOCASE rename clash; `filename()` instead
  of `format!` SQLite URLs (production code and tests); the
  `datetime(started_at)` 7-day fix; and the interrupted-restore
  recovery in the desktop startup path. Enum-column CHECK
  constraints are impossible without a table rebuild in SQLite —
  recorded as a limitation, enforced by the typed readers.
- **Storage (ST-m):** root-anchored `parse_archive_path`,
  recursive archive scan with a depth cap, real symlink checks
  in `validate_repository_path`, normalized storage root,
  canonical relocation comparison, and a missing-root
  create-or-clear-error in the runner. Relocation locks are a
  documented accepted trade-off (same audit note as B4): the
  fetch re-acquires the same in-process lock the move held, so
  holding the guards through the fetch would self-block; the
  race needs two concurrent different-type jobs for one repo,
  which the enqueue rules make near-impossible.
- **Agent & IPC (AG-F):** accept-error backoff, client-side
  protocol-version validation in `ping()`/`status()`, biased
  scheduler select + post-shutdown Enqueue refusal, Windows
  ctrl_close/ctrl_shutdown/ctrl_logoff signals, a warnings-only
  startup self-check, and autostart validate/repair on all three
  platforms.
- **Release (E5–E8, F14–F16):** E5 was already done (release.yml
  builds agent/broker before tauri-action). E7 removes the unused
  CSP origin; E8 adds the CHANGELOG-tag CI job; F14 asserts the
  ConfirmDialog CSS classes in the built output; F15 lists the
  MSI; E6 adds an SPDX SBOM upload; F16 untracks the two 120KB
  Tauri-generated schema files (capabilities.json and
  acl-manifests.json stay tracked).

Verified 2026-11-26: `cargo fmt --check` OK, clippy (workspace
excl. desktop) clean, full workspace suite **472 passed / 0
failed** (baseline 462; +10 new tests), typecheck clean,
frontend tests **84 passed / 0 failed** (baseline 78; +6 new:
2 Agent badge, 4 Overview), `pnpm -r lint` clean, vite build OK
(the F14 classes `.fixed`, `.inset-0`, `.z-50`, `.bg-black\/50`
are all present in the built CSS), Semgrep (exact CI command)
**0 findings** (one `current-exe` finding on the new
autostart code was suppressed per-site with the repo's existing
nosemgrep convention — the path is only compared for status),
`cargo audit` clean (0 vulnerabilities, 7 no-fix warnings).
MinGW GNU cross-check passes for `reponyx-ipc` and
`reponyx-background-agent` (the Windows shutdown-signal code
compiles for the Windows target). Two verification catches, both
fixed: the missing-root check first failed the `both_live`
test (a fresh root is normal — the runner now creates it and
only errors when creation fails), and the Overview lint failed
on a `react-hooks/exhaustive-deps` disable comment for a rule
that is not installed (restructured to `useCallback`, no disable
needed). Honest limits: the desktop crate changes (`agent.rs`
autostart, `storage.rs` parse, `diagnostics.rs` recovery) are
compile-checked only by CI's windows-build job; the updater E2E
is still unproven (needs the first release + Windows); the
installer bundle globs are still unproven until the ZIP fix
lands and CI runs; the SBOM action (`anchore/sbom-action@v0`)
has not run anywhere yet.

## Phase 6 Windows verification + first release (2026-11-26, in progress)

The audit plan had only 5 phases, so Phase 6 is the old Phase 10
remainder: run the Windows verification checklist
(`docs/qa/windows-verification.md` §1–§5) on a real Windows
machine, sync the remaining repos end to end, and cut the first
release (v0.1.0). Started on the user's "start phase 6", confirmed
as this scope.

- **CI fully green on the Phase 5 push** (`d814813`, checked
  2026-11-26 ~03:34 MST): CI, Lint, CodeQL, Security audit all
  success — including the `windows-installer` job, so the
  portable-ZIP path fix and the installer bundle globs are now
  proven. The one-time CI-check cron set for the push was removed
  after this check (no duplicate report).
- **Labels:** all 7 Labeler labels now exist on GitHub
  (`backend`, `agent`, `broker`, `tests` created via web UI
  2026-11-26 ~03:36 MST, verified via API; 19 labels total).
  Dependabot PR #2 was already closed. Version is `0.1.0` in
  Cargo.toml + tauri.conf.json; CHANGELOG has the `## [0.1.0]`
  section (date set at tag time).
- **dev:all failure on a fresh clone (2026-11-26 ~03:47 MST):**
  the user's Windows run of `pnpm dev:all` failed at build.rs —
  `resource path ..\..\..\target\release\reponyx-agent.exe
doesn't exist`. Root cause: `tauri.conf.json` lists the
  release agent/broker exes as bundle resources, but `dev:all`
  only ran the debug binaries (`cargo run`) and never built the
  release ones — the same failure CI run #80 hit. Immediate
  unblock given to the user: `pnpm agent:build` +
  `pnpm broker:build`, then `pnpm dev:all` again. Durable fix in
  local commit `ad0a47b` (root package.json): `dev:all` now runs
  `agent:build && broker:build` first (cargo release builds are
  cached after the first run, so later starts stay fast).
  **Not yet pushed** — rides with the next approved push.
  Note: `pnpm tauri:dev` alone still fails the same way until
  the release binaries exist; `dev:all` is the entry point.
  (Update 2026-11-26 ~04:12 MST: pushed as snapshot `fbf0e78`,
  sync 61; local main synced via merge `1fc2fcc`.)
- **App identifier** `com.reponyx.app` → `com.reponyx.desktop`
  (user's request 2026-11-26 ~04:12 MST): the `.app` suffix
  collides with the macOS application-bundle extension and
  caused macOS/Linux build warnings. One line in
  `tauri.conf.json`; no other live references existed. Local
  commit `2b61165`, pushed 2026-11-26 ~04:14 MST as snapshot
  `c3c5e8d`, sync 62; local main synced via merge `1ce7a6f`.
- **User pushed `757078b` "update gen" themselves** (2026-11-26
  ~04:15 MST, parent `c3c5e8d`): regenerated
  `gen/schemas/capabilities.json` (+`core:event:default`,
  `dialog:default`, `notification:default`) and
  `acl-manifests.json`. Benign — local main synced via merge
  `6629e2b`.
- **v0.1.0 tag created by the user** (2026-11-26 ~04:30 MST,
  points at `757078b`) — the Release workflow ran on it
  (run 45810288008) and **FAILED in the `test` job**: the
  `disk_full` test in `crates/git-engine/tests/disk_full.rs`
  uses `RLIMIT_FSIZE` (`libc::rlimit`/`getrlimit`/`setrlimit`),
  which does not exist on Windows — 12 compile errors. The
  `release` job was skipped, so no draft release exists.
  Root cause: the Windows `cargo test` gate was never run
  before — CI's `rust` job runs on Linux and `windows-build`
  only `cargo check`s the desktop crate, so no Windows test
  target was ever compiled.
- **Fix (local commit `817b95e`, not yet pushed):**
  `disk_full.rs` is now `#![cfg(unix)]` (the error-mapping
  half is still covered on every platform by the
  `maps_disk_full` unit test); the same MinGW cross-check
  found a second ungated Unix-only test —
  `symlinked_directory_escaping_the_root_is_rejected` in
  `paths.rs` used `std::os::unix::fs::symlink` — now portable
  (`symlink_dir` on Windows) and skips gracefully when the
  environment cannot create a symlink. The mirror read-only
  and health stale-lock tests were already `#[cfg(unix)]`.
  Verified: `cargo test` 472/0, clippy clean, Semgrep (CI
  command) 0 findings, and `cargo check --tests --workspace`
  for `x86_64-pc-windows-gnu` now passes — every test target
  compiles for Windows. **The tag must move to the fixed
  commit before the release can run** (it points at the
  broken `757078b`); the re-tagged run is the real proof, a
  Windows `cargo test` run has never passed yet.
- **Installed-app broker gap → in-app setup (2026-11-26 ~05:00
  MST):** the user reported the installed app still shows
  "cannot reach the Auth Broker" while dev works, and rightly
  called the hand-made `%APPDATA%\RepoNyx\.env` a workaround,
  not a professional solution. Root cause as diagnosed: the
  broker needs App id + private key + setup token, and the
  installed desktop only knew the repo-root `.env`. Fix (local,
  not yet pushed): new settings `broker.app_id` (digits-only)
  - `broker.private_key_path` (disk-checked at save: absolute,
    existing file; contents never read — the key never passes
    through the desktop), setup token generated in-app (only when
    none is configured and no `.env` file supplies one) or pasted,
    stored in the OS credential store; the desktop passes all three
    to the spawned broker as env vars (explicit vars win over any
    `.env` file); a repo `.env` or hand-made `<data_dir>\.env`
    still works. New commands
    `generate_broker_setup_token_command` +
    `pick_broker_key_file_command` (native dialog,
    `blocking_pick_file` on a blocking thread); status reports
    `appIdConfigured` / `privateKeyConfigured` /
    `envFileConfigured` and the Accounts page shows a setup form
    only when the broker has no config at all. Verified:
    `cargo test` 475/0 (+3 database validation tests), frontend
    94/94 (+4 Accounts tests), Semgrep 0 findings; desktop crate
    compile-checked by CI windows-build only. The user's
    `%APPDATA%\RepoNyx\.env` (created on the earlier advice) still
    works as an override and needs no change.
- **Schedules cadence controls (user's request 2026-11-26
  ~05:00 MST):** the New schedule form had one text box for all
  cadence types — "6" sat in Daily/Weekly where it means nothing.
  Now: Every N hours = dropdown 1–24; Daily = time picker;
  Weekly = weekday toggles by full name + time picker; switching
  type resets the value ("6" / "02:30" / "1 02:30" / "90"); a
  stored hours value above 24 is kept as an option when editing;
  a weekly schedule with no weekday is refused client-side
  (backend still validates). Same controls in the edit form.
  Frontend tests 94/94 (+6 Schedules tests).
- **Storage root `\\?\` prefix (user's report 2026-11-26
  ~05:02 MST):** the Storage page showed
  `\\?\D:\github\zb\articles`. Root cause: the overview DTO
  returned `effective_storage_root`, which canonicalizes, and
  Windows canonicalization adds the verbatim prefix. Fix: new
  `display_path` helper strips `\\?\` (and `\\?\UNC\` →
  `\\`) for display only, applied to the overview root, orphan
  paths and the diagnostics export; filesystem operations and
  comparisons keep the canonical form. Deleting an orphan still
  works: the backend re-canonicalizes the path the UI sends back.
- **Agent page "(see ADR-006)" removed (user's request
  2026-11-26 ~05:05 MST):** an internal architecture-record
  citation with no meaning to a user. Only user-facing ADR
  reference in the frontend; the ADR document itself is
  unchanged. Agent tests 10/10.
- **Console windows during syncs + menu changes (user's videos
  2026-11-26 ~05:15/05:18 MST):** every sync and every Overview /
  Agent page load flashed a black cmd window open and closed. Root
  cause verified in code: `git.exe`, `git-lfs`, `schtasks`, `reg`,
  `taskkill`, `tasklist` were spawned without `CREATE_NO_WINDOW`
  — the agent and broker spawns already had it, these did not.
  The video even caught the proof: the Agent page's title bar
  went inactive (white) for a frame while a console window stole
  focus. Fix (local, not yet pushed): the flag on every one of
  those spawns — git-engine `run_git_command` / streaming (via
  `spawn_in_own_group`), `detect_git` + `detect_lfs`, the
  `taskkill` in `kill_process_tree`, the IPC `tasklist` liveness
  checks, and all 12 `schtasks`/`reg`/`taskkill` calls in the
  desktop's `agent.rs` (one `hidden_command` helper). The
  `rundll32`/`explorer` "open" calls are GUI programs and need no
  flag. Verified: `cargo test` 475/0, MinGW Windows check passes,
  Semgrep 0 findings; desktop-crate half by CI windows-build.
  The user still has to confirm on their machine that no window
  flashes on a sync or a menu change.
- **Release run on the moved tag FAILED again (run 45815018729,
  2026-11-26 ~05:11 MST):** the user moved v0.1.0 to `03bf01a`
  and the Release workflow ran — the `test` job failed, but NOT
  at compile time: the code compiled and the tests ran. Only
  `reponyx-database --test config` failed (2 tests:
  `export_parse_round_trip`, `apply_to_fresh_db`), both panicking
  on "storage.root must be an absolute path". Root cause: the
  fixture used `/tmp/reponyx-test-root` — absolute on Linux, but
  Rust's Windows `is_absolute` needs a drive prefix. Fix (local):
  the fixture builds its root from `temp_dir()` (absolute on both
  platforms). Verified: `cargo test` 475/0 incl. the config
  binary 7/7. The tag must now move to the new head after the
  push — the release has still never run green.
- **Setup token automatic (user's request 2026-11-26 ~05:22 MST):**
  the user pointed out the token field had to be typed from
  `.env`, and that every user/team member would face the same
  step — the field must not exist at all. Fix (local, not yet
  pushed): the token resolves itself — a broker `.env` file
  first (dev, or a hand-made file; only the token line is read,
  never the key), else the OS store, else a fresh per-install
  token generated and stored on first use. The "Broker setup
  token" field, the Generate button and "Save setup token" are
  gone from the Accounts page (both the setup form and the old
  token row); the setup form is now App id + key file only, and
  `configured` means "the broker can start" (`.env` file, or App
  id + key path saved). Deliberately NOT baked into the build: a
  token in the binary is the same for every user and readable
  from the installer (anyone could pair to anyone's broker), and
  the private key can never be baked in either (it is the master
  key of the GitHub App — anyone with the installer could mint
  tokens for every installation). Verified: frontend 94/94,
  typecheck + lint + vite build clean, Semgrep 0 findings (one
  finding on the first run — `temp_dir()` in the new dotenv
  test — fixed by using `tempfile::tempdir()`); desktop-crate
  half by CI windows-build.
- **CI on `b25bdcc` FAILED in two places (2026-11-26 ~05:50 MST,
  browser check of the Actions page):** Lint + Security audit
  failed the `check-no-private-key.sh` scan — the new broker-setup
  code passes `REPONYX_BROKER_PRIVATE_KEY_PATH` to the spawned
  broker, and the script's substring match banned any mention of
  the key variable. CI's `rust` job failed Clippy on
  `crates/ipc`: the new `hidden_command` helper is only called
  from Windows-only code, so on Linux it was dead code (and its
  `mut` unused) — an error under `-D warnings`. CodeQL passed;
  Semgrep passed; frontend passed (13 files / 94 tests). The
  windows-build + windows-installer jobs were still running when
  checked; the `rust` failure already decided the run. Fixes
  (local, not yet pushed): the scan now bans the bare key variable
  only (regex excluding the `_PATH` suffix, with the reason
  documented in the script) + the broker.rs comment reworded;
  `hidden_command` is now `#[cfg(windows)]`. Verified with the
  exact CI commands (`cargo clippy ... -- -D warnings`, the scan
  script): both pass. The local Clippy runs before this push had
  omitted `-D warnings`, so the warnings passed silently — the
  push-verification skill now says to always use CI's exact
  command.
- **CI #100 (run 45823043121, on `b25bdcc`) finished Failure
  (2026-11-26 ~06:08 MST, browser check):** rust failed on the
  Clippy issue above (already fixed), frontend passed (94
  tests), and BOTH Windows jobs failed with the same compile
  error — `error[E0599]: no method named 'state' found for
struct 'tauri::AppHandle'` at `auth.rs:479` (the browser
  report mis-transcribed the path as `commands/auth.rs` and the
  line as `handle.state::<State>().db.clone()`; the real file is
  `src/auth.rs`, the real line is the
  `app.state::<Option<Database>>()` call in
  `get_broker_status_command`). Root cause: `AppHandle::state`
  needs the `tauri::Manager` trait in scope; `auth.rs` never
  imported it. The call arrived in `6d19792` (in-app broker
  setup) and no local check could compile the desktop crate,
  so CI windows-build was the first to see it. Fix (local):
  one-line import (`use tauri::{AppHandle, Manager, State};`).
  Verified: fmt clean; every file calling `.state()` now
  imports `Manager`; a MinGW Windows check of the desktop crate
  was attempted (if the VM cannot run it, CI windows-build on
  the next push is the check, as always). CI #101 (on
  `11122ab`) carries the same `auth.rs`, so its Windows jobs
  will fail the same way — the 06:18 scheduled check of #101
  was cancelled as superseded; the fix rides with the next push.

Still open (user-side): the Windows verification checklist §1–§5,
the one-agent setup test, syncing sendcms + reponyx, then tag
v0.1.0 (needs the user's approval — publishing a release is
gated), draft release → publish, and the updater E2E via v0.1.1.
PUSHED 2026-11-26 ~06:34 MST as snapshot `d15ff3e`, sync 66,
294 files (Manager-import fix + docs + central-broker +
web-platform plans; local main merged via `25f34ff`). CI on
`d15ff3e`: Lint / Security audit / CodeQL GREEN, CI run #102 in
progress at 06:36 MST (frontend passed, rust + windows jobs
running; final check scheduled 06:48 MST). The tag must then
move to that new head (it still points at `03bf01a`, whose
release run failed on the config tests). The user also asked for
a plan to restructure the Accounts page (separate "Add account"
vs "Connected accounts", for 10–50 accounts) — see
`ai/phase6/accounts-page-plan.md`, awaiting their approval
before implementation. The web platform plan
(`ai/phase6/web-platform-plan.md`) supersedes the central-broker
plan and will be implemented on a 2nd branch when approved, so
main and the new work stay distinct.

## Windows first-run bugs (2026-11-23)

First real Windows run (`target\debug\reponyx-desktop.exe`):
every DB-backed page showed "DatabaseError: database is
unavailable", Add PAT failed with `missing required key
accountType`, and the Agent page showed "[object Object]".

- **Invoke args are camelCase, not snake_case.** Tauri 2
  defaults to `ArgumentCase::Camel` (proven in
  `tauri-macros-2.7.6` source and by the live error); the
  api-client sent snake_case keys, so all multi-word-arg
  commands were broken. Fixed in `packages/api-client` +
  page call sites; `api-client.test.ts` asserts the keys
  against a mocked `invoke`.
- **Agent page "[object Object]"**: it was the only page
  without the `commandMessage` helper. Added it (covered by
  `Agent.test.tsx`).
- **Database failure**: the user's log showed `migration 1
was previously applied but has been modified`. The only
  difference is the comment rename `-- Gitora V1` →
  `-- RepoNyx V1` in `0001_init.sql` (`49a44bf` vs `main`);
  sqlx checksums the whole file including comments, and the
  user's DB was created by a pre-rename build. Machine fix:
  delete `%APPDATA%\RepoNyx\data\reponyx.sqlite*` (DB empty)
  and restart. `database_unavailable()` now points at the app
  log. New rule: never modify an applied migration.
- **Agent not running**: a desktop-only build never builds
  `reponyx-agent.exe`, so `resolve_agent_exe` finds nothing
  next to the desktop exe. Source-checkout fix: `cargo build
-p reponyx-background-agent`. Release-workflow bundling of
  the agent is still an open packaging gap.

## Auth Broker runs with the app; release ships all binaries (2026-11-23)

The broker had no way to run together with the app and no place
in a GitHub release: root `package.json` had no broker script,
and `release.yml` shipped only the desktop, so GitHub App auth
could not work from a release install.

- Root scripts added: `broker` (`cargo run -p
reponyx-auth-broker`), `broker:build`, `agent:build`, and
  `dev:all` (concurrently 10.0.7 runs broker + `tauri:dev`
  together, `-k` so one exiting stops the other). The broker
  needs its env vars in the shell (`REPONYX_BROKER_APP_ID`,
  private key via `REPONYX_BROKER_PRIVATE_KEY_PATH`, setup
  token); it exits with a clear error when one is missing.
- Follow-up the same day (user's request): the broker now
  also reads a `.env` file from the working directory
  (`dotenvy` 0.15, `dotenv().ok()` before config load, so
  `pnpm broker` / `pnpm dev:all` work without exported
  variables; OS env vars always win, `.env` is gitignored).
  `dev:all` now runs the agent too (`broker` + `agent` +
  desktop) — the desktop's `ensure_agent_running_command`
  checks health first, so it never spawns a duplicate. New
  `build:all` runs `broker:build && agent:build &&
tauri:build` in sequence (one cargo target-dir lock).
  Proven by a live smoke test: `.env`-only start serves
  `/health`, an OS var beats a bad `.env` value, and no
  `.env` + no vars still exits 2 with the same error.
- Release workflow: builds agent + broker in release mode,
  puts all three binaries in the portable ZIP, and uploads
  `reponyx-agent-x64.zip` / `reponyx-auth-broker-x64.zip` as
  their own assets; release notes list them.
- `.env.example` documents the broker variables; README has a
  "Running the Auth Broker" section.
- Still open: the NSIS installer contains only the desktop.
  Bundling agent/broker into it needs `bundle.resources` +
  a resource-dir lookup in `resolve_agent_exe`.

## Completed tasks

- **Phase 9 — LFS sync (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the first Phase 9 task.
  A mirror holds LFS pointers, never objects, so the LFS step is
  separate from the mirror sync. New `lfs` module in
  `reponyx-git-engine` (`crates/git-engine/src/lfs.rs`):
  `lfs_fetch` runs `git lfs fetch --all` under the same
  per-repository lock as mirror clone/fetch, with the credential
  helper installed so HTTP remotes authenticate like a normal
  fetch, then verifies every object; `verify_lfs_objects` checks
  each pointer oid against the object file under
  `<git-dir>/lfs/objects/` and its SHA-256 (the LFS oid _is_ the
  content hash, streamed so large objects are never fully in
  memory). A corrupt object is deleted and re-downloaded —
  `git lfs fetch` silently skips a file that merely exists, so
  without that the corruption would never heal. `has_lfs_pointers`
  gates the step. The runner (`crates/sync-queue/src/git_runner.rs`)
  now runs `LfsFetch` jobs and, after any successful clone/fetch,
  enqueues a separate `LfsFetch` job when the repo has pointers
  (dedupe key prevents duplicates; best-effort — it never fails
  the job that just succeeded). Fetch failures are re-coded to
  `GitLfsFailed` (retryable) unless the underlying error is more
  specific (auth, not-found, network). New tests, all real
  git-lfs 3.8.1 fixtures (installed on this VM from the official
  release tarball — `apt-get install git-lfs` found no package,
  and `apt-get update` hung): 3 engine tests (fetch + verify,
  missing/corrupt reporting with self-heal, failure when the
  remote lacks the object) and 2 runner tests (auto-enqueue +
  verify end to end, no LFS job for a repo without pointers). The
  old `git_runner_rejects_lfs_job_type` test is now a submodule
  rejection test plus an LFS-without-local-clone test
  (`RepositoryNotFound`). Two real behaviours found by the tests
  and fixed: `git lfs ls-files` prints a 10-character abbreviated
  oid by default — the builder now passes `--long`; local-path
  remotes hardlink LFS objects, so the corruption test writes a
  new file instead of editing in place (an in-place rewrite
  corrupted the remote's copy too). Verified: fmt pass, clippy 0
  warnings, `cargo test` 352 passed / 0 failed (was 338, +14),
  typecheck + desktop build (1.87 s) + eslint pass. No UI change
  in this step — the Synchronization page already lists LfsFetch
  jobs and the Activity page already shows `lfs_fetch` operations.
- **Phase 9 — Submodule sync + Both-mode completion (actual,
  2026-11-21, assistant, uncommitted at the time of writing):**
  the second and third Phase 9 tasks, in one commit. A mirror of
  a superproject holds gitlinks but not submodule content, so the
  submodule step is separate from the mirror sync, like LFS. New
  `submodule` module in `reponyx-git-engine`
  (`crates/git-engine/src/submodule.rs`): `list_submodules`
  reads gitlinks (`ls-tree -r HEAD`, mode 160000) joined with the
  `.gitmodules` blob at HEAD; `sync_submodules_mirror` fetches
  each gitlink commit into the mirror's object database (`git
fetch --no-tags <url> <sha>`, verified with `cat-file -e`,
  objects already present are skipped) and `sync_submodules_working`
  runs `git submodule update --init --recursive` per submodule —
  one at a time so an inaccessible (e.g. private) submodule never
  blocks the others. Failures are per-submodule and reported in
  the summary; the runner fails the job only when nothing at all
  synced, and logs a warning for partial success. Relative
  submodule URLs (`../sub.git`, the common GitHub layout) are
  resolved against the mirror's origin URL with git's own rule
  (the full origin URL is the base: `../x` consumes the
  superproject's last segment, `./x` does not — verified against
  real git behaviour, unit-tested for https/ssh/plain paths).
  The runner now runs `SubmoduleSync` jobs (both sides when they
  exist) and, after any successful clone/fetch, auto-enqueues one
  when either local clone has submodules (dedupe-guarded,
  best-effort). **Both-mode completion:** the scheduler still
  creates one job per due repo, but the runner now completes the
  mode — after a `MirrorClone` of a Both repo it enqueues a
  `WorkingClone` job, and a `Fetch` for a Both repo fetches each
  side that exists and enqueues the clone job for any missing
  side (so a mirror-only Both repo, or one whose working clone
  was deleted, heals on the next fetch). The LFS step now prefers
  the mirror for Mirror/Both repos, so LFS objects land in the
  archive even when `local_path` currently points at the working
  clone. New tests, all real local fixtures: 4 engine tests
  (mirror fetch + idempotence, isolation of an inaccessible
  submodule, working checkout, no-submodule no-op), 3 runner
  tests (mirror + working auto-enqueue end to end, no job for a
  repo without submodules), 2 Both-mode runner tests (fresh Both
  repo gets both clones; mirror-only Both repo gets its working
  clone on fetch, and a second fetch creates no second clone
  job). Two real behaviours found by the tests and fixed:
  relative submodule URLs must be resolved against the origin
  (a raw `git fetch ../sub.git` resolves against the process cwd
  instead), and the working-clone layout is
  `<root>/working-clones/<account>/<owner>/<name>` — no `github`
  segment, unlike the mirror path. Verified: fmt pass, clippy 0
  warnings, `cargo test` 373 passed / 0 failed (was 352, +21),
  typecheck + desktop build (1.68 s) + eslint pass. No UI change
  in this step — the Synchronization page already lists
  SubmoduleSync/WorkingClone jobs and the Activity page already
  shows `submodule_sync` operations.
  Still open in Phase 9: performance pass (now done — see the
  entry below).

- **Phase 10 — Security audit, first pass (actual, 2026-11-22,
  assistant, uncommitted at the time of writing):** Phase 10
  started. Survey first: much of the plan was already in place
  and was verified rather than rebuilt. CI pipeline: `ci.yml`
  (fmt, clippy, tests, typecheck, lint, windows-build), `lint.yml`
  (private-key scan), `security-audit.yml` (Semgrep), `codeql.yml`,
  and `scheduled-maintenance.yml` (weekly `cargo audit` + `pnpm
audit`, also runnable on demand) — all green on the last four
  pushes. Release workflow: tag-triggered, Windows NSIS + MSI,
  signed updater artifacts; it will fail at signing until
  `TAURI_SIGNING_PRIVATE_KEY` (+ password) are set in repo
  secrets (user-side, still unknown). Performance test + report:
  done in Phase 9 (measured at 1,000 repos / 10,000 jobs).
  Failure-recovery tests: the coverage already existed scattered
  across crates (expired/revoked tokens, network loss, rate
  limits, real disk-full via `RLIMIT_FSIZE`, lease crash recovery,
  corrupt repos incl. LFS, renames, remote deletion, cancel
  mid-flight, concurrent writes) — now inventoried in
  `docs/qa/failure-recovery.md`, which also names the three gaps
  honestly (no real process-kill test, no mid-transfer network
  loss, no Windows failure tests). What this pass built:
  (1) **Dependency audits, run for the first time** — `pnpm audit`:
  no known vulnerabilities. `cargo audit` (v0.23.0, 661 crates)
  found one real vulnerability — `url` 2.5.8, RUSTSEC-2026-0592
  (stack overflow in `Url::origin()` on nested `blob:` URLs,
  reachable via reqwest in the broker + GitHub provider) — fixed
  the same day by bumping `url` to 2.5.9 (`form_urlencoded`
  1.2.2 → 1.2.3 alongside; Cargo.lock diff is 5 lines). Re-run:
  0 vulnerabilities, 7 warnings with no available fix (chrono +
  chrono-tz unmaintained, proc-macro-error unmaintained,
  zbus_names/zvariant/zvariant_derive unmaintained, glib 0.18.5
  unsound — its fix needs glib ≥ 0.20, blocked by Tauri's pinned
  gtk 0.18 chain, and the affected `VariantStrIter` API is not
  used by RepoNyx code). Note: the yanked-status checks timed out
  against the registry in both runs, so yanked status was not
  verified — the advisory scan itself completed. The weekly CI
  job has never run yet (first scheduled run is Monday
  2026-11-23). (2) **Threat-model review** —
  `docs/security/threat-model.md` re-checked mitigation by
  mitigation against the real code; three findings recorded:
  F1 broker state is in-memory only (a restart loses
  installations, pairing codes, device tokens, and the audit log —
  devices must re-pair; durable broker storage is the open fix),
  F2 the Windows named pipe has no ACL (the token check, not the
  pipe, is the gate; same-user boundary bounds the impact), F3 no
  key-rotation / incident runbook exists yet. Two doc overstatements
  corrected: the IPC protection is the per-install shared secret
  (`crates/ipc/src/auth.rs`), not "named-pipe ACLs", and the
  protocol has no delete operation at all, so deletes are UI-only.
  (3) **Migration upgrade tests** —
  `crates/database/tests/migrations.rs`: a V1 database (built from
  the real 0001 file, seeded with V1-shape rows) upgrades to V4
  with data intact, and a V3 database upgrades to V4 with the new
  dedupe index working on new rows. One real behaviour documented
  by the test: pre-upgrade rows carry a NULL `dedupe_key`, so the
  index does not retroactively dedupe them — the scheduler's
  status-based check is what prevents those duplicates.
  (4) **README rewritten** — it still said "Phase 1" and listed the
  broker/agent/crates as "not yet created"; now current, with a
  user guide (first run, data locations, backup before upgrade)
  and the security notes users must read (BitLocker, hooks in
  working clones, broker restarts lose pairing). (5) **Secret
  scan re-run locally**: `scripts/check-no-private-key.sh` passed;
  the binary half is skipped on this VM (no built desktop binary)
  and remains a Windows item. Verified: fmt pass, clippy 0
  warnings, `cargo test` 406 passed / 0 failed (was 404, +2),
  typecheck + desktop build (3.12 s) + `pnpm lint` pass. Phase 10
  remaining at that point: the Windows verification items
  (installer, Task Scheduler, Credential Manager, binary secret
  scan, real process-kill test), durable broker storage (F1), the
  key-rotation runbook (F3), and the first release itself.

- **Phase 10 — Durable broker storage, F1 (actual, 2026-11-22,
  assistant):** the broker no longer loses everything on restart.
  New `apps/auth-broker/src/store.rs` (`BrokerStore`, sqlx SQLite,
  WAL, own migration `apps/auth-broker/migrations/
0001_broker_store.sql` — 5 tables: installations, pairing codes,
  setup states, device tokens, audit events) plus `REPONYX_BROKER_
DATA_DIR` (default `broker-data`, `broker.sqlite` inside).
  `BrokerState::open` prunes expired codes/states + audit events
  older than 90 days, then loads every record into the memory maps;
  every mutation writes through to the store first, then updates
  the map (a failed persist leaves the map untouched; a retry after
  a partial multi-step persist converges because every step is
  idempotent). Only hashes are stored — never raw codes/tokens and
  never the private key. The minted-token cache and the
  rate-limiter buckets stay in-memory (a restart re-mints tokens
  and resets the buckets). Real bug found while building: a
  `std::sync::RwLock` guard held across an `.await` makes the
  handler future non-`Send`, so axum refused the two routes that
  await the new state methods — the maps moved to
  `tokio::sync::RwLock` and the reader methods (`installation`,
  `list_installations`, `pairing_status`, `device_for_token`)
  became async. `BrokerState::new` (pure in-memory) is unchanged
  for tests. New `apps/auth-broker/tests/store_persistence.rs`
  (6 tests: pairing flow, revocation, single-use state,
  wrong-device attempts, expiry pruning, store round-trip — each
  drops the state and re-opens it on the same file). Also verified
  live: the real binary started, created `broker.sqlite`, served
  `/health`, was killed and restarted on the same store. Verified:
  fmt pass, clippy 0 warnings, `cargo test` 412 passed / 0 failed
  (was 406, +6), typecheck + desktop build (1.65 s) + `pnpm lint`
  pass. Phase 10 remaining: the Windows verification items, the
  key-rotation runbook (F3), and the first release itself.

- **Phase 10 — Windows verification, VM part (actual, 2026-11-23,
  assistant, uncommitted at the time of writing):** survey found
  the Task Scheduler integration IS wired end to end (Agent page
  buttons → api-client → registered Tauri commands → schtasks;
  buttons landed in the user's Phase 6/7 code, `b24acb1`) — the
  README line saying it was "not yet wired" was stale since then
  and is fixed. The agent's duplicate `windows_task.rs` had zero
  callers and is deleted (main.rs doc now points at the desktop's
  autostart commands). Secret scan re-run on this VM: PASSED on
  the source half, binary half skipped (no built binary here).
  Wrote `docs/qa/windows-verification.md`: executable checklist
  for the Windows machine (installer, autostart + logoff/logon,
  Credential Manager, full binary secret scan, real process-kill
  recovery). Still needs a real Windows run; then only the first
  release remains.
  the scheduled-maintenance workflow ran for the first time
  2026-11-23 ~11:47 MST (run #1) and failed on the Rust audit
  job; a manual re-run (run #2, 13:29 MST) failed the same way,
  this time with a real finding: `cookie` 0.18.2,
  RUSTSEC-2026-0594 ("Private cookie nonces can repeat in
  processes that fork", severity 6.5 medium, dated 2026-11-23).
  Fixed with `cargo update -p cookie` → 0.18.3 (transitive via
  Tauri/wry; `Cargo.lock` only, 3 lines). `cargo audit` clean
  again (0 vulnerabilities, same 7 known warnings); `cargo test`
  still 415 passed / 0 failed. Phase 10 remaining: the Windows
  verification items and the first release itself.
  `docs/security/key-rotation-runbook.md` closes finding F3.
  Written against the real code (broker config env vars,
  `POST /pairing/revoke`, IPC service/account names, OS credential
  store locations — each verified by reading the source before
  writing). Covers routine rotation of all five secrets (GitHub
  App private key with the generate → restart → verify → delete-old
  order, broker setup token, per-device tokens, IPC shared secret
  via delete + agent restart, fine-grained PATs), a verify-after-
  rotation checklist, and incident response for a suspected broker,
  desktop/agent, or key compromise, with the audit queries
  (`broker_audit_events` + desktop `audit_logs`) that prove what
  happened. Threat-model F3 marked fixed. Phase 10 remaining: the
  Windows verification items and the first release itself.

- **Phase 9 — Performance pass (actual, 2026-11-22, assistant,
  uncommitted at the time of writing):** the seventh and last
  Phase 9 task. Measured first, then fixed, at a stress scale of
  1,000 repositories / 10,000 jobs / 5,000 history rows
  (`crates/database/tests/performance.rs`, a new harness that
  seeds with bound multi-row INSERTs and prints medians of three
  runs — timings are printed, not asserted, because CI runners
  are too noisy for hard thresholds). Before: a full repository
  list took ~78 ms, pending jobs ~56 ms, completed jobs ~244 ms,
  all jobs ~356 ms — and the Synchronization page paid the
  pending+running variant on every 5-second poll, while the
  Repositories page paid the full list on every search keystroke
  (debounced) and filter change; both grew linearly with the
  tables, which only grow. After: `RepositoryFilter` and
  `JobFilter` take a `limit` (clamped 1..=500 in the database
  layer) and repositories also an `offset`; the Tauri commands
  page at 100 rows (jobs 1..=200) and `list_repositories_command`
  returns `{ items, total }` so the page header shows the real
  total. The Repositories page loads pages of 100 and appends
  them on "Show more" (same UX, server-side now); the
  Synchronization page passes limit 100 on every poll. Both pages
  used to load the whole repository table just to resolve
  repository names for job/history rows — a new bounded lookup,
  `repository_names` (IN list, capped at 500 ids) behind a new
  `get_repository_names_command`, fetches names only for the rows
  actually on screen. After numbers on the same scale: a
  repository page ~14 ms, pending jobs ~5 ms, completed jobs
  ~10 ms — and a page stays that fast whatever the table size,
  because it is always at most 100 rows. Already-bounded queries
  were left alone (`list_recent_history(100)` ~21 ms,
  `dashboard_counts` ~1 ms, `count_repositories` ~0.2 ms).
  Pagination correctness is asserted (pages cover the table
  exactly once, limit clamps 0 → 1 and 9,999 → 500, offset
  without a limit is ignored, the names lookup returns only
  asked-for ids). Internal callers that genuinely need every row
  (config export, scheduler materialize, discovery, recovery)
  keep `limit: None` and were only touched where the new field
  broke their struct literals (`..Default::default()` added —
  note the base must be last and takes no trailing comma, which
  the first patch script got wrong in 25+ places and the
  compiler caught). No new migration: the existing indexes
  already cover the paged queries. Verified: fmt pass, clippy 0
  warnings, `cargo test` 404 passed / 0 failed (was 402, +2),
  typecheck + desktop build (2.16 s) + eslint pass. Desktop Rust
  is fmt + review only (gdk blocker), as usual — the new
  commands' signatures were checked against the real helpers by
  review, and the windows-build job is their first real compile.
  Phase 9 is now complete; remaining is Phase 10.

- **Phase 9 — Import/export (actual, 2026-11-22, assistant,
  uncommitted at the time of writing):** the sixth Phase 9 task.
  A versioned JSON config file (`reponyx-config`, version 1)
  moves a setup to another machine: the four non-secret settings,
  each repository's sync selection (mode + enabled), and the
  schedules. It never holds credentials, tokens, local paths, or
  archive state. Identity across machines uses provider-stable
  ids, never local UUIDs — repositories match by
  `provider_repository_id`, accounts by (`provider`,
  `provider_account_id`); `next_run_at` is never stored, it is
  recomputed from the cadence on import the same way creating a
  schedule computes it. Import is validated before anything is
  written (format marker, version, settings values, schedule
  shapes, every schedule's cadence/timezone), so a malformed file
  refuses the whole import; entries whose repository or account
  does not exist on this machine are skipped and reported, not
  errors. Both commands open a native file dialog
  (`tauri-plugin-dialog` v2, `dialog:default` capability — Save
  for export, Open for import; cancelling returns nothing). The
  format, export, parse, and apply logic lives in a new
  `reponyx-database` module (`config.rs`, 7 new tests incl. a
  fresh-DB round-trip, skip reporting, and an atomicity test
  proving a bad cadence writes nothing). The settings value
  validator moved there too, as the single source of truth, and
  the desktop `validate_setting` now delegates to it. The
  `next_run` computation lives in `reponyx-scheduler`, which the
  database crate cannot depend on, so apply takes it as a
  callback — the desktop passes the real `next_run`, tests pass a
  stub. The Settings page has a new Import & export section with
  the report (applied counts + skipped entries); after an import
  the form reloads and the theme applies right away. Verified:
  fmt pass, clippy 0 warnings, `cargo test` 402 passed / 0
  failed (was 395, +7), typecheck + desktop build (2.79 s) +
  eslint pass. Desktop Rust is fmt + review only (gdk blocker),
  as usual; the dialog plugin's Rust API
  (`blocking_pick_file` / `blocking_save_file`, `FilePath`
  variants) was checked against the published docs before
  writing, and the capability + plugin wiring follows the
  updater's pattern exactly.

- **Phase 9 — Diagnostics export + DB backup (actual,
  2026-11-21, assistant, uncommitted at the time of writing):**
  the fifth Phase 9 task. Two support tools, both on the
  Settings page. **Diagnostics export:** one JSON file under
  `<data_dir>/diagnostics/` with the app version, OS, git/LFS
  versions, database counts, storage sizes, the four non-secret
  settings, a 100-line log tail and the last 50 error lines.
  Every free-text field goes through `redact_secrets`, and the
  finished file is checked by a new `verify_no_secrets` scan
  before it is written — a failed scan writes nothing (the scan
  accepts the `[REDACTED]` marker and documented placeholders
  like `C:/path/...`, and fails on raw PAT shapes, live
  Authorization headers, PEM private-key blocks, credential
  `key=value` pairs, and credentials in URLs). **Database
  backup:** consistent snapshots via `VACUUM INTO` (never a
  file copy of a live WAL database) under `<data_dir>/backups/`,
  listed newest-first with size and date, deletable. **Restore:**
  the file is validated first (SQLite header +
  `PRAGMA integrity_check` + `settings` table, on a throwaway
  read-only connection), the background agent is stopped over
  IPC (`ClientMessage::Shutdown` — it holds the database open
  and is re-spawned on demand), and the file is staged as
  `reponyx.sqlite.restore-pending`. The swap happens at the
  next startup, before the database opens: the header is
  checked again, the live database is moved aside as a
  `pre-restore-<timestamp>` fallback, stale WAL sidecars are
  dropped, and any failure leaves the live database untouched
  (a failed swap tries to put the live file back). Paths from
  the UI for delete/restore are canonicalized and must stay
  under the backups folder. New testable pieces live in the
  library crates: the scan + `tail_lines` in `reponyx-logging`
  (10 new tests), `diagnostics_counts` + `backup_database` +
  `validate_backup_file` + the settings allowlist in
  `reponyx-database` (7 new tests, incl. a backup round-trip
  that re-opens the snapshot and counts rows). One real sqlx
  behaviour found while building: dynamic SQL needs
  `AssertSqlSafe` — `VACUUM INTO` takes no bound parameters,
  so the escaped path is interpolated with an audit comment.
  Verified: fmt pass, clippy 0 warnings (3 `question_mark`
  nits fixed), `cargo test` 395 passed / 0 failed (was 378,
  +17), typecheck + desktop build (1.88 s) + eslint pass.
  Desktop Rust is fmt + review only (gdk blocker), as usual;
  the new commands' signatures were checked against the real
  helpers by review.

- **Phase 9 — Storage management + orphan detection (actual,
  2026-11-21, assistant, uncommitted at the time of writing):**
  the fourth Phase 9 task. The Storage page was read-only; it now
  manages the archive. New engine pieces in
  `crates/git-engine/src/storage.rs`: `ArchiveKind` (Mirror /
  Working) + `ArchiveDir`, `scan_archive_dirs` (mirrors need a
  HEAD file + objects dir, working clones need a `.git` entry;
  stray directories are ignored) and `delete_archive_dir`
  (canonicalizes both paths, refuses anything outside the root,
  the root itself, or a non-archive directory, returns the bytes
  freed). New shared DB helper `set_local_path` in
  `crates/database/src/repositories.rs` — the runner's local SQL
  copy of it (38 lines) is deleted; the runner now calls the
  helper. The overview command now also reports orphans (scanned
  dirs whose canonical path matches no row), missing archives
  (local_path set but the path is gone) and renamed archives
  (path exists but its basename no longer matches the row's
  current name). Three new Tauri commands:
  `delete_orphan_command` (re-scans at delete time, refuses a
  path any row still points at), `delete_local_archive_command`
  (deletes both sides for a Both repo, then clears `local_path`
  and returns the status to Pending — the row stays, the next
  sync re-clones), `move_archive_for_rename_command` (renames
  each side on disk, never over an existing destination, rolls
  back moved sides on failure, updates `local_path` only after
  every move succeeded). The Storage page gains the three
  sections (orphans with kind badge + Delete, renamed with
  current → expected + "Move archive", missing with the
  re-clone note), a per-repo "Delete archive" button, a freed-bytes
  notice after each action, and a low-space warning below 10%
  free. New tests: 4 engine storage tests + 1 DB
  `set_local_path` test. Verified: fmt pass, clippy 0 warnings,
  `cargo test` 378 passed / 0 failed (was 373, +5 — one earlier
  full run showed a single failure in a 15-test suite that did
  not reproduce in 2 subsequent full runs, same transient pattern
  as the Settings commit), typecheck + desktop build (1.85 s) +
  eslint pass. Desktop Rust is fmt + review only (gdk blocker),
  as usual; the new commands' signatures were checked against
  the real helpers (`set_local_path`, `set_archive_status`,
  `get_account`, `CommandError` fields) by review.

- **Phase 8 — Completion (actual, 2026-11-23, assistant,
  uncommitted at the time of writing):** the three remaining
  Phase 8 items, built after the six pages — Phase 8 is now
  100% per its plan. **Notifications:** `tauri-plugin-notification`
  v2 (Rust crate + JS package, `notification:default` capability,
  registered in `lib.rs`) with a `useSyncNotifications` hook
  mounted in the app layout — it polls finished jobs every 30 s,
  seeds silently on the first poll (old jobs are history, not
  news), then notifies once per newly completed/failed job naming
  the repository and the job type, with the error message on
  failures. A new fifth writable setting `notifications.enabled`
  (validator arm in `reponyx-database::config`, `SettingsDto` /
  shared type + schema, toggle on the Settings page, on unless
  explicitly `false`, re-read on every poll so the toggle takes
  effect without a restart); permission is requested once and a
  denial stops polling. Nothing in the hook may break the app —
  every failure is swallowed. **Frontend tests:** Vitest 5.1.1 +
  Testing Library 16 + jsdom 31, `pnpm --filter @reponyx/desktop
test`, 41 tests in 6 files (theme, zod schemas, shared UI
  components, Settings page incl. validation + error state,
  notification hook incl. fake timers, virtualized Repositories
  list); CI frontend job now runs the suite (the old "no JS unit
  test suite" note in `ci.yml` is gone). Testing lessons: RTL
  needs manual `cleanup()` with Vitest `globals: false`;
  `waitFor` deadlocks under fake timers (flush with
  `advanceTimersByTimeAsync` instead); jsdom has no layout, so
  the Repositories test mocks `offsetHeight`/`clientHeight`/
  `clientWidth` (the virtualizer reads exactly those). **True
  virtualization:** the Repositories list is now a
  `@tanstack/react-virtual` 3.14.28 virtualizer in a 640px scroll
  container with dynamic row measurement (`measureElement`,
  190px estimate, overscan 5, `initialRect` seeds the first
  paint); server pagination ("Show more") is unchanged underneath.
  **Tray menu:** new "Sync due now" item (ensures the agent is
  running, materializes due schedules, reports the result as a
  system notification), a separator before Quit, and Pause/Resume
  are now state-aware — a background task polls agent health
  every 30 s and enables only the action that applies (neither
  when the agent is down). Unused `Placeholder.tsx` deleted.
  Verification: fmt pass, clippy 0 warnings, `cargo test` 413
  passed / 0 failed (was 412, +1 validator test), typecheck +
  desktop build (1.92 s) + eslint pass, frontend tests 41
  passed / 0 failed, local Semgrep (exact CI command) 0 findings.
  Desktop Rust is fmt + review only (gdk blocker), as usual; the
  new tray/notification wiring compiles in the CI windows-build
  job. **Live progress bars (added the same day at the user's
  request, second commit):** the gap above is closed — the agent
  forwards the queue's progress broadcast to IPC subscribers as
  JSON `ServerMessage::Progress` frames (one forwarder task per
  connection; replies and pushes share the write half behind a
  mutex so a push never waits for the next request), the desktop
  subscribes on one long-lived connection and re-emits
  `sync-progress` Tauri events (10 s retry loop, so progress
  resumes when the agent restarts), and the Synchronization page
  renders a determinate bar when git reports a percent — new
  `percent: Option<u8>` on `SyncProgressEvent::JobProgress`,
  filled from the git progress parser (counting objects from
  received/total, the rest from the parsed percent) — and an
  indeterminate bar otherwise; terminal events clear the bar and
  the 5 s poll shows the final status. Proven by 2 new IPC tests
  (`crates/ipc/tests/progress_forwarding.rs`: event reaches a
  subscriber while ping still works; requests work with no source
  attached) + 4 new page tests (determinate, indeterminate,
  cleared on completion, malformed events ignored). Totals after
  the second commit: cargo test 415 passed / 0 failed, frontend
  tests 45 passed / 0 failed. Phase 8 is now 100% with no open
  items.

- **Phase 8 — Settings page (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the sixth and last
  Phase 8 page — all sections are now real. General: theme
  (System/Light/Dark, `ui.theme`, applied at startup via the
  class-based dark variant already in `styles/index.css`). Sync:
  default sync mode for newly discovered repositories and
  concurrent syncs 1–10. Storage: archive root, with a warning
  that existing archives stay put and the change applies after
  the agent restarts. Advanced is read-only (app version, data
  directory, paired device identifier). Two new Tauri commands
  in `apps/desktop/src-tauri/src/settings.rs`; only four keys
  are writable and each value is validated before it is stored.
  Two settings that nothing consumed are now real: discovery
  reads `sync.default_mode` for new rows (existing rows keep
  theirs — new test
  `discovery_new_rows_use_default_sync_mode_setting`), and the
  frontend applies `ui.theme` on startup. The page also exposed
  a real bug (fixed in the same commit): pairing stored the
  per-machine id only under `device_identifier`, but the agent's
  credential resolver reads `device.identifier` — a key nothing
  ever wrote — so GitHub App accounts failed every job in the
  agent even after pairing succeeded. `complete_pairing_command`
  now persists `device.identifier` too. Verified: fmt pass,
  clippy 0 warnings, `cargo test` 338 passed / 0 failed (was
  337, +1 discovery test; one earlier full run showed a single
  transient failure that did not reproduce in 3 subsequent full
  runs or 3 runs of the two known timing-sensitive git-engine
  suites), typecheck + desktop build + eslint pass. The new
  desktop Rust is verified by `cargo fmt` + review only (gdk
  blocker) — the CI windows-build job is its first real compile.

- **Phase 8 — Storage page (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the fifth Phase 8 page.
  It needed a backend first — no disk-usage code existed
  anywhere. New `storage` module in `reponyx-git-engine`
  (`dir_size`: recursive, never follows symlinks, missing paths
  are 0; `disk_usage` via `fs2`) with 5 unit tests, and two new
  Tauri commands in `apps/desktop/src-tauri/src/storage.rs`
  (`get_storage_overview_command`, `open_storage_root_command`;
  sizes computed on a blocking thread, off the async runtime).
  The page shows the storage root (setting or default data dir)
  with an Open folder button, a volume usage bar (used/total/
  free), breakdown cards (mirrors, database file, logs, stored
  total), and a per-repository list sorted by size with account
  name, last synced, local path, and Open folder. Sizes are
  measured on demand (Refresh button, no polling). Read-only in
  v1: no delete action — deleting a local archive is destructive
  and needs its own confirm flow. Two real bugs were caught by
  review before committing (the desktop crate cannot compile on
  this VM, so review is the check): `sqlx` is not a direct
  desktop dependency (helper now takes `&Database`), and the
  storage root was moved into the `spawn_blocking` closure and
  then used again (now cloned for the scan). Verified: fmt pass,
  clippy 0 warnings, `cargo test` 337 passed / 0 failed (was
  332, +5 storage tests), typecheck + desktop build + eslint
  pass. The new Rust commands are verified by `cargo fmt` +
  review only (gdk blocker) — the CI windows-build job is their
  first real compile.

- **CI run #34 failure fixed (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** run #34 on commit
  `0cfc96d` failed only in the `rust` job — the git-engine health
  test `corrupt_object_is_caught_by_fsck` panicked with
  `PermissionDenied` at `health.rs:150`. Git stores object files
  read-only (0444), so overwriting one fails with EACCES for a
  non-root user; this VM runs as root, so the bug never showed
  here. The test now removes the object file first (deleting
  needs directory write permission, not file permission) and
  writes the corrupted bytes back. Root cause proven by running
  the test binary as user `nobody` (like the runner): exact CI
  panic before the fix, 6/6 health tests green after it (3 runs).
  Run #34's annotations list only the `rust` error — the
  `windows-build` job passed, confirming the run #32
  desktop-compile fix in CI. Verification: fmt pass, clippy 0
  warnings, `cargo test` 332 passed / 0 failed. Test-only change.

- **CI run #32 failures fixed (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** CI run #32 on commit
  `65d4c2f` failed in two jobs. (1) `rust`: the git-engine
  cancellation test
  `cancel_mirror_clone_leaves_diagnosable_destination` failed on
  the runner — diagnosis `Corrupt` after a cancelled clone. The
  300 ms cancel can land while git is still writing its skeleton,
  so torn metadata is a legitimate outcome; the test now also
  accepts `Corrupt` (test-only change; the no-HEAD invariant is
  unchanged). Reproduced as stable locally (15 single runs +
  3 full git-engine runs green) — the flake needs a loaded
  runner. (2) `windows-build`: `cargo check -p reponyx-desktop`
  failed with two E0308 errors in `synchronization.rs` —
  `SyncJobDto` declares `priority`/`attempt_count` as `i64`
  while domain `SyncJob` has them as `i32`; fixed with lossless
  `as i64` casts. This was the first real desktop-crate compile
  error caught by CI. Verification: fmt pass, clippy 0 warnings,
  `cargo test` 332 passed / 0 failed. The desktop crate still
  cannot compile on this VM (gdk blocker); the CI windows-build
  job is the compile check.

- **Phase 8 — Activity page (actual, 2026-11-21, assistant,
  uncommitted at the time of writing):** the fourth Phase 8 page.
  It needed one new database query — `list_recent_history` in
  `crates/database/src/history.rs` (newest first, limit clamped to
  1..=500; a global recent-history query did not exist, only
  per-repository and per-job lists) — plus a new integration test
  file `crates/database/tests/history.rs`. New Tauri commands in
  `apps/desktop/src-tauri/src/synchronization.rs` (history reads
  belong with the sync module): `list_recent_history_command`
  (default limit 100) and
  `list_history_for_repository_command`. No migration, no new Rust
  dependencies.
  - **What it does:** the sync-history record — what ran, when,
    how long it took, whether it worked. Repository filter
    (dropdown) and result filter (success/failed/cancelled);
    per-entry repository name (from one repository list load),
    operation label, result badge, started/finished times,
    duration, bytes received; 10-second auto-refresh plus a manual
    Refresh button. History is append-only; the page never edits
    it.
  - **Honest limits:** the new Rust commands are verified by
    `cargo fmt` + review only (gdk blocker) — the Windows build is
    their first real compile; the page shows the latest 100
    entries, no paging past them; no frontend tests yet.
  - **Verification (actual, 2026-11-21):** `cargo fmt --check`
    pass; `cargo clippy --workspace --exclude reponyx-desktop
--all-targets` pass, **0 warnings**; `cargo test --workspace
--exclude reponyx-desktop` **332 passed, 0 failed** (was 331 —
    the +1 is the new `recent_history_is_newest_first_and_limited`
    test, run on its own too); `pnpm -r typecheck` pass; `pnpm
--filter @reponyx/desktop build` pass (1.61 s); desktop eslint
    pass on the touched files (exit 0). One typo fixed during
    verification (`.map_error` → `.map_err` in `history.rs`) — the
    first full run caught it as a compile error; the suite was
    re-run clean after the fix.

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
    - updater plugin config with a minisign pubkey; icon list
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

| Check                                                                                | Result                                                                                                                                       |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| cargo fmt --check                                                                    | pass (after `cargo fmt`)                                                                                                                     |
| cargo clippy -p core,logging,git-engine,provider-abstractions,database --all-targets | pass, 0 warnings                                                                                                                             |
| cargo test (same 5 crates)                                                           | **36 passed, 0 failed** — core 7, database migrations 3, git-engine unit 21 + mirror-clone integration 1, logging 4, provider-abstractions 0 |
| pnpm install                                                                         | pass (151 packages; pnpm-lock.yaml committed)                                                                                                |
| pnpm -r typecheck                                                                    | pass (shared, ui, api-client, desktop)                                                                                                       |
| pnpm --filter @reponyx/desktop lint (eslint)                                         | pass                                                                                                                                         |
| pnpm --filter @reponyx/desktop build (vite)                                          | pass (1844 modules, 1.03 s)                                                                                                                  |
| cargo check -p reponyx-desktop                                                       | **FAIL — blocked**: `gdk-sys v0.18.2` build script: `Package gdk-3.0 was not found in the pkg-config search path`                            |
| cargo tauri build (Linux)                                                            | **not run to completion — blocked** by the same missing system libraries; see below                                                          |

### Phase 5 — 2026-11-18 (Authentication, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check                                                            | Result                                                                                                                                                                                                                                                                                                                                      |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| cargo fmt --check                                                | pass                                                                                                                                                                                                                                                                                                                                        |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings (4 collapsible-`if` warnings found and fixed during the run)                                                                                                                                                                                                                                                               |
| cargo test --workspace --exclude reponyx-desktop                 | **262 passed, 0 failed** — auth-broker 22 (lib 7, broker integration 15), core 11, credential-store 20 (lib 6, mixed_auth 1, pat_flow 10, store 3), database 55, git-engine 87, github-provider 63 (lib 26, broker_source 5, discovery 8, errors 8, installations 4, rate_limit 4, redaction 6, sync 2), logging 4, provider-abstractions 0 |
| scripts/check-no-private-key.sh                                  | **pass** on desktop sources + dependency tree; **binary part SKIPPED** — no desktop binary can be built on this VM (WebViewGTK blocker)                                                                                                                                                                                                     |
| pnpm -r typecheck                                                | pass (shared, ui, api-client, desktop)                                                                                                                                                                                                                                                                                                      |
| pnpm --filter @reponyx/desktop build (vite)                      | pass (2306 modules, 1.95 s)                                                                                                                                                                                                                                                                                                                 |
| pnpm --filter @reponyx/desktop lint (eslint)                     | pass                                                                                                                                                                                                                                                                                                                                        |
| cargo check -p reponyx-desktop                                   | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1 (desktop auth commands verified by `cargo fmt` + review only)                                                                                                                                                                                           |

One transient failure during verification: the untouched Phase 3
test `healthy_mirror_and_working_clone` failed once under full
parallel load and passed 3/3 in isolation and on the full-suite
re-run (262 passed, 0 failed) — recorded as a flake, not fixed here.

### Phase 7 partial — 2026-11-19 (scheduler crate only, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check                                                            | Result                                                                                                                                                                                                                                  |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| cargo fmt --check                                                | pass                                                                                                                                                                                                                                    |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings (1 collapsible-`if` + 1 unused-workspace-dependency warning found and fixed during the run)                                                                                                                            |
| cargo test --workspace --exclude reponyx-desktop                 | **302 passed, 0 failed** — scheduler 40 new (lib 8, dst 5, materialize 10, next_run 14, precedence 3); auth-broker 22, core 11, credential-store 20, database 55, git-engine 87, github-provider 63, logging 4, provider-abstractions 0 |
| cargo tree -p reponyx-scheduler                                  | core + database only (chrono, chrono-tz, serde, serde_json, sqlx, uuid) — no git-engine, no provider, no queue                                                                                                                          |
| pnpm -r typecheck                                                | pass (shared, ui, api-client, desktop)                                                                                                                                                                                                  |
| pnpm --filter @reponyx/desktop build (vite)                      | pass                                                                                                                                                                                                                                    |
| pnpm --filter @reponyx/desktop lint (eslint)                     | pass                                                                                                                                                                                                                                    |
| cargo check -p reponyx-desktop                                   | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1 (desktop schedule commands verified by `cargo fmt` + review only)                                                                                   |

No flakes observed in the full-suite runs during this work.

### Phase 4 — 2026-11-18 (GitHub integration, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check                                                            | Result                                                                                                                                                                                                                       |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| cargo fmt --check                                                | pass                                                                                                                                                                                                                         |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings                                                                                                                                                                                                             |
| cargo test --workspace --exclude reponyx-desktop                 | **215 passed, 0 failed** — core 11, database 55 (installations 8), git-engine 87, github-provider 58 (lib 26, discovery 8, errors 8, installations 4, rate_limit 4, redaction 6, sync 2), logging 4, provider-abstractions 0 |
| pnpm -r typecheck                                                | pass (shared, ui, api-client, desktop)                                                                                                                                                                                       |
| pnpm --filter @reponyx/desktop build (vite)                      | pass (2305 modules, 1.56 s)                                                                                                                                                                                                  |
| cargo check -p reponyx-desktop                                   | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1                                                                                                                                          |

### Phase 3 — 2026-11-18 (Git engine, on `main`)

All commands actually run. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0,
Git 2.43.0 (git-lfs not installed on this VM).

| Check                                                            | Result                                                                                                                                                                                                                  |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| cargo fmt --check                                                | pass                                                                                                                                                                                                                    |
| cargo clippy --workspace --exclude reponyx-desktop --all-targets | pass, 0 warnings                                                                                                                                                                                                        |
| cargo test --workspace --exclude reponyx-desktop                 | **156 passed, 0 failed** — core 11, database 54, git-engine 87 (lib 58, cancellation 3, credentials 4, disk_full 1, health 6, locks 3, mirror 6, mirror_clone 1, refs 1, working 4), logging 4, provider-abstractions 0 |
| cargo test -p reponyx-git-engine (3 consecutive runs)            | 87 passed, 0 failed each run                                                                                                                                                                                            |
| pnpm -r typecheck                                                | pass (shared, ui, api-client, desktop)                                                                                                                                                                                  |
| pnpm --filter @reponyx/desktop build (vite)                      | pass (2305 modules, 1.82 s)                                                                                                                                                                                             |
| cargo check -p reponyx-desktop                                   | **FAIL — blocked**: same missing `gdk-3.0` / WebViewGTK system libraries as Phase 1                                                                                                                                     |

### Re-check 2026-11-18 (user's 8 commits + dependency updates, on `main`)

All commands actually run, after the merge (`d3adc8c`) and the fixes
listed above. Node 26.13.1, pnpm 12.21.0, Rust 1.100.0.

| Check                                                                                | Result                                                                                                                                                                                                            |
| ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| cargo fmt --check                                                                    | pass (after `cargo fmt` on `build.rs`)                                                                                                                                                                            |
| cargo clippy -p core,logging,git-engine,provider-abstractions,database --all-targets | pass, 0 warnings                                                                                                                                                                                                  |
| cargo test (same 5 crates)                                                           | **36 passed, 0 failed** — core 7, database migrations 3, git-engine unit 21 + mirror-clone integration 1, logging 4, provider-abstractions 0                                                                      |
| cargo update                                                                         | pass — sqlx 0.8.6 → 0.9.1, tauri-plugin-updater 2.14.1 added, tauri 2.12.7, tauri-build 2.7.6, all others already latest stable                                                                                   |
| pnpm install                                                                         | pass (Node 26.13.1; `pnpm peers check`: no peer issues after TS pinned to 6.0.3)                                                                                                                                  |
| pnpm -r typecheck                                                                    | pass (shared, ui, api-client, desktop) — also passed under TS 7.0.2 before the eslint-driven pin-back                                                                                                             |
| pnpm --filter @reponyx/desktop lint (eslint)                                         | pass                                                                                                                                                                                                              |
| pnpm -r lint                                                                         | pass                                                                                                                                                                                                              |
| pnpm --filter @reponyx/desktop build (vite)                                          | pass (2305 modules, ~1 s)                                                                                                                                                                                         |
| cargo check -p reponyx-desktop                                                       | **FAIL — blocked**: `gdk-sys v0.18.2` + `gdk-pixbuf-sys v0.18.0`: `Package gdk-3.0 was not found in the pkg-config search path`. No tauri-build, capability, icon, or updater error appeared before this failure. |

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

## Web platform — W1 code-complete (2026-11-26, branch `feature/admin-control-plane`)

Plan: `ai/phase6/web-platform-plan.md` (approved 2026-11-26 ~07:34 MST).
W1 built on the 2nd branch, main untouched.

- `apps/web` (Next.js 16.4.6): auth API (register / verify / login /
  logout / reset + sessions + devices + limits), dashboard shell,
  en + ur i18n (6 web domains, 85 keys).
- `packages/auth`: Argon2id via hash-wasm (pure WASM — also runs on
  Cloudflare Workers), tokens as SHA-256 hashes.
- `packages/email`: Resend (fetch) / SMTP (nodemailer), selected by
  `EMAIL_PROVIDER=resend|smtp`; both credential sets can live in
  `.env`; admin sees only whether each is configured.
- `packages/database`: Neon HTTP driver, `0001_init` schema +
  `0002_seed_packages` (Free 2/2, Bronze 10, Silver 20, Gold 50,
  Diamond 100), migrate CLI (`pnpm --filter @reponyx/database migrate`).
- Abuse limits enforced server-side: registration 1/IP/24h,
  account-switch 1h device-first/IP-fallback; structured errors
  (code + data, retry-at) localized by the client.
- Verified: package tests 25/25, web auth-flow tests 5/5 against real
  Postgres (PGlite — full register→verify→login→limits→devices→logout
  flow, both abuse limits, password reset, duplicate email), i18n
  validate 0 failures, all typechecks clean, `next build` clean
  (18 routes).
- Still open: the live Neon project (`DATABASE_URL`) + the first
  Vercel deploy. Local Postgres could not be installed on this VM
  (apt lock held by a system process) — migration SQL was verified
  against PGlite instead.

## Web platform — database moved to Prisma Postgres (2026-11-27, branch `feature/admin-control-plane`)

- The user replaced Neon with their own Prisma Postgres database
  (hosted at db.prisma.io) as the web's database; `DATABASE_URL` on
  the Vercel project now points at it (the Neon `reponyx-db` is
  unused and still exists).
- The Neon HTTP driver (`@neondatabase/serverless`) cannot talk to
  Prisma Postgres — it POSTs to `https://<host>/sql` and Prisma
  answers HTTP 404 — so `@reponyx/database` is now dual-driver:
  `*.neon.tech` hosts use the Neon HTTP driver, any other Postgres
  uses `pg` 8.23.1 (TCP, SSL from the URL's `sslmode`). The Neon
  path stays, so the Cloudflare Workers portability goal is intact.
- Migrations applied to the live Prisma DB (`0001_init`,
  `0002_seed_packages`) and verified live: 12 tables, 5 packages
  seeded, 0 users. `DbClient` gained an optional `close()`; the
  migrate CLI calls it so a `pg` run terminates.
- Verified after the switch: package tests 25/25, web auth-flow
  tests 5/5, i18n validate 0 failures, typechecks clean,
  `next build` clean.
- Still open: email provider (Resend key / SMTP) + first deploy.

## Web platform — first deploy (2026-11-27, branch `feature/admin-control-plane`)

- The user set all remaining env vars in the Vercel dashboard
  themselves: Google/GitHub OAuth client IDs + secrets, Resend key
  - sender, EMAIL_PROVIDER, SMTP host/port/user/pass/from, and the
    GitHub App id + private key. Email and OAuth are now configured.
- The user changed the Production branch tracking from `main` to
  `feature/admin-control-plane`.
- Push snapshot `47c52b2` (sync 68) auto-triggered a production
  deployment that FAILED (`NEXT_NO_VERSION` — Root Directory was
  `./`, so Vercel could not find Next.js at the repo root). Fixed:
  Root Directory `apps/web`, Output Directory `.next` (build command
  unchanged: `pnpm --filter @reponyx/web build`).
- Redeploy `dpl_Ci5vDkCKQxpATSNNzqVCg27sHB2K` is READY at
  `reponyx-qf643fyqp-stylies.vercel.app` (branch alias only — NOT
  reponyx.bond). Login page verified live (200). The deployment is
  behind Vercel Authentication (SSO), so curl checks hit the
  protection, not the app.
- Still open: a successful production deployment to reponyx.bond +
  a live register→verify→login test with a real email.

- Browser login-form smoke test PASSED 2026-11-27 ~01:09 MST:
  nonexistent email → "Email or password is incorrect", stayed on
  the login page, no 500 — the live app reaches the Prisma database
  and handles login normally.

## Web platform — production deploy live (2026-11-27)

- Production deployment `dpl_6YcHm1iZZVuUmFfNqxjRa9NKyRBK`
  (commit `47c52b2`) is READY, target production, aliases include
  `reponyx.bond`. Verified live: home 200, login page 200, login API
  with a nonexistent email returns the app's own
  `invalid_credentials` — the production app reaches the Prisma
  database. Custom domains are exempt from the Vercel SSO wall, so
  reponyx.bond is publicly reachable.
- Still open: a live register→verify→login test with a real email
  (tests the Resend/SMTP path end to end). W2 (OAuth) has not
  started.

## Web platform — live register→verify→login PASSED (2026-11-27)

- The user registered, verified and logged in themselves on
  reponyx.bond (owner account admin@csofts.com). Dashboard shows
  Package Free, Linked accounts 0/2, Devices 0/2, Expires: No
  expiry, Joined 11/27/2026. The email (Resend/SMTP) path works end
  to end. Devices 0/2 is expected — a device registers only on
  desktop-app login (W3 scope). W1 is now fully proven live.

## Web platform — W2 OAuth + sendcms design (2026-11-27, local only)

- **Formatting fixed (user's call-out):** the 12 locale JSON files were
  single-line dumps. Prettier 3.10.1 + prettier-plugin-tailwindcss
  adopted (root `.prettierrc`, `pnpm format` / `format:check`,
  scoped to the web platform); all locale files are now
  pretty-printed (2-space, keys alphabetical, final newline) and
  `format:check` passes. i18n validate: 6 domains, 95 keys, 0 failures.
- **Design from sendcms:** Tailwind v4 + the sendcms dashboard design
  tokens (OKLCH teal/emerald, light + dark) + Geist + next-themes.
  Proper pages: AuthShell (two-column brand panel + form), styled
  forms (shared `ui.ts` classes), dashboard cards + sticky header,
  centered home page. `apps/web/public/` holds favicon.ico,
  favicon-16/32, apple-touch-icon, icon-192/512, logo.png,
  og-image.png (1200x630) and site.webmanifest — all generated from
  the desktop app icon (1024px) with exact token colors.
- **W2 OAuth DONE:** Google + GitHub login, callbacks at the exact
  registered URLs, HMAC-signed state (SESSION_SECRET, 10 min),
  no auto-create, provider-verified email satisfies verification,
  same account-switch limit + 2FA step as password login. Tests:
  web 14/14 (5 auth-flow + 9 OAuth), packages 7/7 + 10/10 + 8/8;
  typecheck clean; `next build` clean (22 routes). Local smoke test
  on `next start`: home 200, login 200 (both OAuth buttons render),
  start routes 307 to the providers, bad callback → login
  `oauth_failed`, favicon/og/manifest 200.
- **Skills brought from sendcms:** `next-best-practices` (19 files,
  verbatim) + a RepoNyx-adapted `reponyx-project-guidelines` skill
  (SKILL.md + 7 references from RepoNyx's own conventions), both in
  `~/workspace/custom-skills/`.
- Still open: push + preview/production deploy of this batch (needs
  user approval); a live Google/GitHub click-through on reponyx.bond.

## Web platform — W2 pushed + deployed (2026-11-27 ~04:13 MST)

- Snapshot push **sync 69**: `0cf5fff` (401 files) — W2 batch on
  `feature/admin-control-plane`. Verified on GitHub (oauth.ts,
  .prettierrc, site.webmanifest present).
- Binary public assets uploaded via the web UI: `cd010f2`
  (8 files: favicon.ico, favicon-16/32, apple-touch-icon, icon-192/512,
  logo.png, og-image.png) — recorded as sync 70. The snapshot flow
  cannot carry binary files; web-UI upload is the path for them.
- Vercel auto-deployed both commits; latest **dpl_6JifbvLvG1krhkb2U5CZHMXtn68c**
  READY (production target, commit cd010f2). Live smoke test on
  reponyx.bond: home 200, login 200 (both OAuth buttons, styled),
  all assets 200, Google/GitHub start routes 307 to the providers
  with the exact registered callback URLs, bad callback →
  login `oauth_failed` (no 500).
- Still open: a live Google/GitHub click-through on reponyx.bond
  (needs the user in a browser — the consent step cannot be tested
  headless); W3 desktop login next.

## Web platform — W2 polish (2026-11-27, pushed + deployed 2026-11-27 ~04:47 MST)

User asked (screenshots of RepoNyx vs sendcms): OAuth buttons on the
register page too, full name mandatory, buttons above the form like
sendcms, a Terms of Service line + page, clean URLs.

- **OAuth register intent:** the signed state carries `login` |
  `register`. Login still refuses unregistered users; the register
  intent creates the account (provider-verified email + profile name,
  same 1/IP/24h limit, existing email → `email_taken`). OAuth users
  get an unusable password hash until they set one via reset.
- **Full name:** migration `0003_add_user_name` (`users.name`); register
  API + form require it; dashboard welcome + `/api/me/limits` return it
  (email fallback when null).
- **Buttons on top:** login + register both render OAuth buttons first,
  divider "or continue with email", then the form.
- **Terms:** `/terms` (English legal text) + agreement footer on
  login/register (en + ur).
- **Clean URLs:** `/login`, `/register`, `/reset`, `/reset/confirm`,
  `/verify`; `/auth/*` redirects with the query preserved; email links
  use the new URLs.
- Verified: format clean, i18n 99 keys 0 failures, typechecks clean,
  database 8/8, web 19/19 (5 auth-flow + 14 OAuth), `next build` clean
  (27 routes), local smoke test green (button order, /terms 200, old
  URLs 307 with query, register without name → 400).
- Pushed + deployed 2026-11-27 ~04:47 MST: snapshot `64e716f6`
  (sync 71, 419 files — covers this batch AND part 2); migrations
  0003 + 0004 applied to the live Prisma DB BEFORE the push (verified:
  `users.name` + `users.has_password` present, `linked_providers`
  exists, 4 migrations applied, 1 user); Vercel `dpl_HYVWVdcJzLnk5nBhpZHbds8BhZc2`
  READY (production target, commit 64e716f6, aliases incl.
  reponyx.bond). Live smoke test on reponyx.bond green: /login +
  /register 200 with buttons on top, Full name field, Terms footer;
  /terms 200; /auth/* → 307 to clean URLs; OAuth starts 307 with the
  exact registered callback URLs; /register/complete renders with the
  invalid-token note; /api/me/linked 401 without a session.
- Still open: the CI-on-feature-branch question (user has not
  answered yet — branch CI has never run). Live Google/GitHub
  click-through DONE 2026-11-27 ~04:56 MST (user linked GitHub +
  Google to the muhammaddomains65@gmail.com account; dashboard shows
  both linked, same email, success banner, name saved).

## Web platform — W2 polish, part 2: linked sign-in methods + name required (2026-11-27, pushed + deployed 2026-11-27 ~04:47 MST, same snapshot `64e716f6`, sync 71)

User asked: even with the same Gmail/GitHub/Google — all one account;
a linked option inside the profile, so any of them can sign in. Then
clarified: first-time accounts are still made from the register page;
Google/GitHub registration works but the name is mandatory, so the
profile is complete before login is allowed.

- **Linked providers:** migration `0004_linked_providers` +
  `users.has_password`; OAuth login/register auto-links the provider;
  link-first resolution (provider identity beats email); dashboard
  Sign-in methods card with Link (intent=link, email must match,
  already-linked refused) + Unlink (last-method guard, 409
  `last_login_method`).
- **Name required:** OAuth register without a provider name →
  `/register/complete` with a signed 10-min pending token; `POST
/api/auth/register/complete` creates the account under the 1/IP/24h
  limit. Profile name edit via `PATCH /api/me` (owner account can
  complete its profile).
- Verified: database 9/9, web 28/28 (5 auth-flow + 19 OAuth + 4
  linked), i18n 120 keys 0 failures, typechecks clean, `next build`
  clean (32 routes), local smoke test green.
- Pushed + deployed with the W2 polish batch (same snapshot
  `64e716f6`, sync 71; migration 0004 applied to the live Prisma DB
  before the push — see above). Live smoke test green (see above).
- Still open: the CI-on-feature-branch question (unanswered). Live
  Google/GitHub click-through DONE 2026-11-27 ~04:56 MST (user linked
  GitHub + Google to the muhammaddomains65@gmail.com account;
  dashboard shows both linked, same email, success banner).

## Web platform — W3 desktop login (2026-11-27, branch `feature/admin-control-plane`, local only)

- Web: pending-login token (HMAC, 10-min) + `POST /api/auth/2fa/verify`;
  OAuth state `client` flag (`web`|`desktop`); desktop callback →
  `reponyx://auth?code=<desktopCode>` (HMAC, never the raw session
  token); `POST /api/auth/desktop/exchange`; desktop 2FA →
  `reponyx://auth?two_factor_required=1&pending=…`; desktop
  name-required → `reponyx://auth?register_complete=…`; TOTP in
  `@reponyx/auth` (`otpauth` 9.6.0). No new web DB migration.
- Desktop Rust: `web_auth.rs` (9 commands), `reponyx:web`/`session`
  in the OS store only, `web.account` cache for the offline rule,
  `ui.locale` is the 8th settings key, deep-link 2.6.3 +
  single-instance 2.5.5 (single-instance FIRST), scheme `reponyx`,
  `CommandError.retry_at` added (30+ literals).
- Desktop frontend: `Login.tsx` (outside the layout), `App.tsx`
  auth gate + deep-link listener, `LocaleProvider` + `useT`,
  Settings Account section + Language switcher, shared web types +
  zod schemas, api-client `web*` functions.
- Verified: web 36/36, auth 14/14, cargo test (excl. desktop)
  476/476, desktop frontend 114/114, typecheck clean, lint clean,
  vite build clean, i18n 155 keys 0 failures, fmt clean,
  check-no-private-key PASSED. **MinGW `cargo check -p
  reponyx-desktop --target x86_64-pc-windows-gnu` PASSES** (recipe
  in `AGENTS.md` — build agent+broker for the GNU target, copy the
  two `.exe` files to `target/release/`; the copies were deleted
  afterwards). It caught 4 real errors. MinGW clippy on the
  desktop: 11 pre-existing findings, none in W3 code, none gated
  by CI. Semgrep: 1 pre-existing blocking finding
  (`packages/database/src/client.ts`, `sslmode=require` →
  `rejectUnauthorized: false`; live DB uses `verify-full`) — left
  for the user's decision.
- **Pushed + deployed 2026-11-27 ~05:50 MST: snapshot `7a79050`
  (sync 72, 429 files); Vercel `dpl_DKp7eR9ZhNTqDY1dKht3a3nKcVrP`
  READY (production target, aliases incl. reponyx.bond).** Live
  smoke test on reponyx.bond green: home/login/register/terms 200,
  dashboard 307; Google + GitHub desktop OAuth starts 307 with the
  exact callback URLs and the `client=desktop` state flag;
  `POST /api/auth/2fa/verify` and `POST /api/auth/desktop/exchange`
  with bad tokens → the app's own 404 `invalid_token` (both new W3
  endpoints live); `/api/me/linked` 401 without a session. (Note:
  the deploy-watch poll loop silently failed — `get_deployment`
  needs `idOrUrl`, not `id` — and the state was confirmed with a
  direct call instead.)
- Still open: the live desktop login flow on the user's Windows
  machine (needs the build + install); the CI-on-feature-branch
  question (unanswered).

## CI now runs on the web-platform branch (2026-11-27, branch `feature/admin-control-plane`)

- The user approved running CI on the branch (the question from
  2026-11-27 ~04:20 MST). `ci.yml`, `lint.yml`,
  `security-audit.yml`, `codeql.yml` now trigger on push and pull
  request to `main` AND `feature/admin-control-plane`.
- Workflow files cannot be written by the GitHub App (403), so
  they were uploaded via the GitHub web UI as commit `8086892`
  (4 files, 11 insertions, 10 deletions — verified by reading
  `ci.yml` back at that commit). Local copy committed as
  `af518cf` to keep the working tree in sync.
- The upload commit should have triggered the first CI runs on
  the branch — **checked 2026-11-27 ~06:30 MST: Lint #75 PASS,
  CodeQL #101 PASS, Security audit #74 FAIL, CI #103 still
  running.** The security-audit failure is Semgrep, 1 blocking
  finding — the KNOWN pre-existing one: `packages/database/src/
  client.ts:42` `return { rejectUnauthorized: false }` for
  `sslmode=require|prefer` (the documented Postgres `require`
  semantic; live DB uses `verify-full`). Verified against the
  raw job log (semgrep 1.187.0 on CI, same result as local
  1.186.0). **Correction:** the browser's first check misread
  the log and reported a different rule (`unsafe-formatstring`
  with a PrismaClient snippet) — that finding does not exist in
  the job; a search of the raw log returns zero matches. The
  fix/suppress/leave decision is the user's (security call).
- **CI #103 FINAL (2026-11-27 ~06:40 MST): FAILURE — only the
  rust job failed; frontend, windows-build, windows-installer
  all green.** Verified against the raw job log: the rust
  failure is ONE test, `hundred_jobs_complete_with_bounded_
  concurrency` in `crates/sync-queue/tests/queue_integration.
  rs:184` — the 30 s queue-drain wait expired with 3 jobs
  still running (`pending=0 running=3`). Same code passed CI
  #102 on main and passes locally 3/3 in ~0.3 s, so it reads
  as a timing flake on a loaded runner, not a regression.
  **Correction:** the browser's first CI #103 read also
  invented a second failure (a `reponyx-core` test in a
  `job_queue.rs` that never existed in the repo) — disregard
  it. **Rust job re-run (user approved 2026-11-27 ~06:43
  MST):** failed jobs re-run via the GitHub UI as attempt #2 —
  **rust job PASSED in 3m 13s, run Success ~06:47 MST; the
  flaky queue-drain failure did not recur — flake confirmed.**
  (Checked 06:54 MST; log: `hidden_files/ci-check-8086892-
  attempt2.md`.)
- **Sync 73 pushed + deployed 2026-11-27 ~06:51 MST (snapshot
  `cd2e10c`, 433 files):** Vercel `dpl_GCXFybH5766WjnGwPfKYk1S7qd3e`
  READY (production, aliases incl. reponyx.bond); live smoke
  test green (home + /login 200, re-checked 07:11 MST). CI on
  the new commit ALL GREEN: CI #104 Success (rust 3m 46s,
  frontend 33s — Vitest 15/15 files, 124/124 tests — windows-
  build 6m 30s, windows-installer 13m 43s, artifact 56.6 MB),
  Lint #76 Success, CodeQL #102 Success, Security audit #75
  Success (Semgrep 0 findings — the verify fix clears it).
  Warnings only: Node.js 20 deprecation on the windows jobs,
  CodeQL Action v3 deprecation coming December 2026.
- **Semgrep decision (user, 2026-11-27 ~06:43 MST): VERIFY,
  not suppress.** `packages/database/src/client.ts`: the
  `require`/`prefer` → `rejectUnauthorized: false` branch is
  removed; all remote hosts now verify the certificate
  (deliberately stricter than Postgres `require`). Local
  commit `41425af` (NOT pushed — push needs its own
  approval). Verified: local Semgrep (exact CI command, 1.186.0)
  **0 findings**, database tests 9/9, typecheck + Prettier
  clean. The live DB already uses `verify-full`, so nothing
  changes for it; a future DB with an unverifiable cert under
  `require`/`prefer` would now fail to connect instead of
  silently skipping verification.
- **Auth fixes batch (user-approved 2026-11-27 ~07:45 MST):
  pushed + deployed 2026-11-27 ~07:55 MST (snapshot
  `42e662a`, sync 74):** Vercel production deployment READY
  (reponyx.bond; home + /login 200; `/reset/confirm?token=
  doesnotexist` shows the invalid-link message, proving the
  page-level validation is live — the "Change password" button
  itself only renders with a real valid token, so it needs a
  real reset link to verify). CI #105 ALL GREEN (12m 3s): rust
  3m 5s, frontend 44s (126/126 tests), windows-build 11m 40s,
  windows-installer 11m 59s (56.6 MB), Lint #77, CodeQL #103,
  Security audit #76 (Semgrep clean). Warnings only: Node.js 20
  deprecation.
  `/login` + `/register` redirect a signed-in user to
  `/dashboard`; desktop clears a stale deep-link error on a new
  attempt and maps `internal_error` / `invalid_body` /
  `registration_rate_limited` / `session_expired` to real
  messages; `/reset/confirm` button is "Change password" with
  its own subtitle, the page validates the token on load (a used
  link shows the dead-link message, not the form), and the
  server claim is atomic (`consumeResetToken` returns whether
  it claimed the token; route consumes before setting the
  password). Verified: i18n 157 keys 0 failures, typecheck
  clean, database 9/9, web 36/36, desktop frontend 15 files
  126/126, lint + vite + next build clean, Prettier clean,
  local Semgrep 0 findings. **Open:** the user's desktop
  OAuth attempt (installed from `pnpm build:all`, so the
  `reponyx://` protocol is registered) reached the Google
  start route once but never the callback or the exchange, with
  no server error in Vercel logs — the user rebuilt from the
  pulled sync-74 code and is retesting; the real error should
  now show instead of the generic banner. **ROOT CAUSE FOUND
  2026-11-27 ~08:18 MST (user's app log + Vercel logs + code):
  the OAuth flow never failed — the app's database would not
  open** (`failed to open database` 3× at startup; the old
  instance is still running and holds the SQLite lock —
  the agent exited with code 1, "another agent is probably
  already running"). The app runs degraded by design
  (`db_state = None`); login AND the desktop-code exchange
  both call `require_db` FIRST (web_auth.rs:459, :634), so both
  die before any server call — code `DatabaseError`, which the
  frontend does NOT map, so it renders the generic
  "Something went wrong". Server proof: Google start 15:16:12Z
  + GitHub start 15:16:17Z hit the NEW sync-74 deployment,
  zero callbacks, zero exchanges, zero runtime errors. The
  broker "not configured" warning in the same log is a
  knock-on (broker settings live in the unreadable DB). **User
  action:** fully quit the old RepoNyx (tray → Quit; Task
  Manager: end RepoNyx.exe + reponyx-agent.exe if present),
  then launch the new build and retry. **Follow-up fix
  (user-approved 2026-11-27 ~08:25 MST, local only):**
  `DatabaseError` maps to a new `databaseUnavailable` message
  (en+ur) in `errorMessageKey`, so a dead DB shows "Database
  unavailable — close other RepoNyx windows and try again"
  instead of the generic banner. Verified: i18n 158 keys 0
  failures, typecheck clean, desktop frontend 15 files 127/127,
  lint + vite build clean, Prettier clean, local Semgrep 0
  findings. **Pushed + deployed 2026-11-27 ~08:30 MST
  (snapshot `5845f0c`, sync 75):** Vercel production
  deployment READY (reponyx.bond; home + /login 200). CI #106
  ALL GREEN (11m 11s): rust 4m 1s, frontend 43s (127/127),
  windows-build 10m 22s, windows-installer 11m 8s (56.6 MB),
  Lint, CodeQL, Security audit (Semgrep clean) — the last
  three are separate workflow runs, reported Success at 08:42.

**Update 2026-11-28 ~01:06 MST — the lock theory is ruled out:**
the user restarted the system and ran `pnpm dev:all` — Task
Manager shows no RepoNyx process — and the agent still failed
with the same generic `[DatabaseError] database operation
failed`, 12 ms after the pre-migration backup was written.
Backup (VACUUM INTO) succeeding means the file is a readable
SQLite database, and `integrity_check` returns a named
`InvalidInput` error, so the failure is in the migration step
(`repair_unrecorded_migrations` or `migrator.run()`), failing
fast — not a lock (a lock would wait up to the 5 s busy
timeout). **Fix (user-approved 2026-11-28 ~01:07 MST, local
commit `f73c2dc`):** `RepoNyxError` Display now renders
`details` when present, so the next run prints the exact SQLite
reason instead of the generic message. Verified: fmt clean,
clippy `-D warnings` clean, cargo test (excl. desktop) 477
passed, 0 failed (was 476, +1 new test), local Semgrep 0
findings. Push approved as sync 76.

**Resolved 2026-11-28 ~01:32 MST — the cause was CRLF line
endings, not the comment rename:** with the details fix live,
the agent printed `migration 1 was previously applied but has
been modified`. sqlx checksums each migration file (SHA-384
over its bytes). The user's working tree had CRLF in all 142
lines of `0001_init.sql`; the DB's `_sqlx_migrations` held LF
hashes for v1–v4 (v1 `EEA96A7C…` matches the LF file exactly)
and a CRLF hash for v5 (`3B1DD982…` == SHA-384 of the CRLF
0005, recorded from a CRLF build during Phase 5). Fix: the 5
migration files converted to LF, the stored v5 checksum updated
to the LF hash `E41BCF61…` (file content unchanged apart from
line endings, so the applied schema already matches), and
`git config core.autocrlf false`. **Lesson (own goal): the
first conversion one-liner
`open(f,'wb').write(open(f,'rb').read()…)` emptied all 5 files
— `open(f,'wb')` truncates before the `read()` runs (reproduced
locally: 0 bytes vs 4 bytes with read-first). The files were
restored with `git checkout -- migrations/sqlite/` (verified
`CRLF: 0 bytes: 5849`). Agent then printed `database opened`
and runs: scheduler materialized 3 jobs. **New open issues
from the same run:** (1) the desktop build fails in
`tauri-build` (build.rs:35) — `The process cannot access the
file because it is being used by another process. (os error
32)` — a stale cargo/rustc from a killed run likely still
holds `target/debug/build/reponyx-desktop/`; fix is to end
leftover cargo/rustc processes and retry, or
`cargo clean -p reponyx-desktop`. (2) All 3 fresh sync jobs
failed permanently with `AuthenticationExpired` right after
the scheduler created them — which accounts/repos is not in
the log; check the Accounts page (Validate) once the app is
up. **Prevention (local commit `5bd2f68`, not yet pushed):**
repo `.gitattributes` pins `*.sql text eol=lf` so no future
clone can check out CRLF migrations.

**Update 2026-11-28 ~02:25 MST — the build-lock cause is
found, and it is NOT a stale process (the user confirmed no
process is running):** `tauri.conf.json` `bundle.resources`
copies `target/release/reponyx-agent.exe` and
`target/release/reponyx-auth-broker.exe` into the build's
target dir (`target/debug` under `tauri dev`). But `pnpm
broker` / `pnpm agent` ran `cargo run` in dev profile, which
executes those very `target/debug/*.exe` files — and Windows
cannot overwrite a running executable. The desktop build
script runs minutes after the agent/broker have started (they
compile in ~19 s), so its `fs::copy` fails with os error 32
every time. The `dev:all` structure is fine: the release
binaries are already built first (`agent:build &&
broker:build`). **Fix (local, not yet pushed):** `pnpm
broker` / `pnpm agent` now run `cargo run --release -p …`,
so the running binaries are the `target/release` ones and
the desktop build script writes to `target/debug/*.exe`,
which nothing runs. Trade-off: agent/broker Rust changes
rebuild in release profile instead of dev. **Separate
finding from the same run:** the agent log at 09:19 shows a
fresh empty database was created (`no existing database
found … existed=false`, `storage.root was not configured`,
workers back to the default 5) — the old database file is
gone from `%APPDATA%\RepoNyx\data\reponyx.sqlite`. All
accounts, repos and schedules are in the pre-migration
backups (`data/backups/reponyx-pre-migrate-*.sqlite`, latest
20261128-083226 from the run where the database opened);
restore by copying the latest backup over `reponyx.sqlite`
while nothing runs. Whether the user deleted the database
themselves is not yet confirmed.

## Accounts page restructure — DONE (2026-11-27, branch `feature/admin-control-plane`, local only)

- The user approved the plan ("1 page pe 2 tab wala ... sai
  hai"). One sidebar entry "Accounts", two route-based tabs:
  `/accounts` = Connected accounts (default), `/accounts/add` =
  Add account.
- `routes/Accounts.tsx` is now the tab shell + Connected list:
  summary line ("1 connected · 1 revoked"), search by username,
  kind filter (All / GitHub App / PAT), status filter (All /
  Connected / Revoked), empty states (no accounts → button to
  the Add tab; no match → Clear filters). Account rows unchanged
  (Validate, Reconnect, Replace PAT, Remove, Delete).
- `routes/AddAccount.tsx` (new): PAT card + GitHub App card.
  `components/BrokerSetup.tsx` (new): broker status, setup form,
  Install + Refresh, installations, pairing — self-contained
  with its own errors/notices. `components/AccountsTabs.tsx`
  (new): the shared tab bar. Router: `/accounts/add` added.
- Tests split by tab: `Accounts.test.tsx` (filters, summary,
  empty states, navigation, delete) + `AddAccount.test.tsx`
  (PAT add, install, pairing, broker setup, broker down).
- Verified: desktop frontend tests 15 files, 124/124 passed
  (was 114); typecheck clean; lint clean; vite build clean.
  (Desktop files are not prettier-formatted — pre-existing repo
  state, confirmed against untouched files; not reformatted.)
- Not pushed yet (needs approval). The Accounts work rides with
  the next snapshot push; the workflow upload `8086892` is
  already on GitHub and is NOT part of the snapshot (`.github/`
  is excluded from manifests).

## 2026-11-28 bug batch — OAuth return leg, already-installed discovery, sync InvalidInput (pushed as snapshot `f355c40`, sync 79, 2026-11-28 ~03:49 MST; deploy+CI check ran 04:10 MST)

User reports 2026-11-28 03:14–03:26 MST, all on the fresh DB (deleted
twice; agent recreated it 02:58 MST; accounts re-added + paired 03:17+;
repos re-discovered; syncs queued 03:15–03:27 MST).

- **Desktop OAuth never completes (GitHub/Google):** email+password
  login works. Vercel logs: Google start hit once, zero callback
  hits. Root causes in the desktop: (1) `reponyx://` is registered
  by the NSIS installer only — `tauri dev` never registers it, so
  the browser cannot hand the OAuth result back to the running dev
  app. Fix: `lib.rs` setup calls `DeepLinkExt::register_all()` on
  Windows, best-effort (warn on failure). (2) A deep link arriving
  while the first session check is still running was dropped;
  `App.tsx` now queues it and flushes once the state resolves to
  signed-out (discarded if signed in). Retest still open: user
  retries OAuth and reports what the browser shows after
  authorizing.
- **Already-installed GitHub App invisible:** the broker recorded
  installations only via its setup callback, so an App already
  installed on GitHub never appeared (user had to uninstall +
  reinstall; video 03:23 shows the dance for 185410556 → new
  186770686). Fix: `GET /installations` now records any GitHub-
  listed installation missing from the store (same call it already
  made for logins; pairing stays empty; per-item failures ignored).
  BrokerSetup notice updated. Test
  `callback_rejects_installation_not_in_github_list` updated: the
  rejected callback still records nothing for 777, but the listed
  999 now appears with its login. **REVERTED in the sync-80 batch
  below (2026-11-28 ~04:07 MST):** listing-based discovery pulls
  in every installation of the shared App — the user proved it by
  installing two accounts (ZohanZaheer 186785209,
  ahmaddomains7-hue 186784678) straight from
  `github.com/apps/reponyx` and watching them appear on Refresh
  with no desktop Install click, and noted that with 100 users
  every desktop would see every account. Only the install
  callback records installations now.
- **Syncs fail permanently with InvalidInput:** six jobs,
  10:15–10:27Z, all `code="InvalidInput"` (user's log
  `reponyx.log-ef2fccbd.2026-11-28`). Root cause: the agent builds
  its credential resolver ONCE at startup, snapshotting
  `device.identifier`; the agent started 02:58 MST, pairing
  completed 03:17+, so every GitHub App job failed with "GitHub
  App sync requires a paired device identifier" — permanent,
  because `classify_error` maps everything non-retryable
  (incl. InvalidInput) to Permanent. Fix: `resolve_github_app`
  reads `device.identifier` from settings on every job; the
  snapshot field is removed from `DefaultCredentialResolver`.
  `storage.root` was ruled out (the agent persists its default
  into settings at startup). Executor failure logs now include
  the error message, not just the code. New test
  `github_app_reads_device_identifier_paired_after_resolver_built`.
  Workaround until this ships: restart the agent once so it picks
  up the pairing, then Sync now works.
- **Web "Linked accounts 0/2" (user, 03:26 MST):** NOT a bug in
  this batch — the web has no way to know about desktop-linked
  accounts yet. The web count moves only in W4 (broker↔web
  linking: the broker reports installations to the web user).
  The desktop Settings card mirrors the web number; the
  desktop's own count is Overview / Connected accounts (2).
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test
  (excl. desktop) 478 passed, 0 failed (was 477, +1 new test);
  desktop frontend 15 files, 127/127; typecheck clean; lint
  clean; vite build clean; local Semgrep 0 findings;
  `check-no-private-key.sh` passed. MinGW desktop check now
  PASSES: mingw-w64 was reinstalled on this VM (`apt-get update`
  + `apt-get install gcc-mingw-w64-x86-64` 13.2.0; `cargo check
  -p reponyx-desktop --target x86_64-pc-windows-gnu` clean in
  17.66s).

## 2026-11-28 live-refresh batch + discovery revert (pushed as snapshot `e217f4b5`, sync 80, 2026-11-28 ~04:30 MST; deploy+CI check ran 04:50 MST — all green, CI #111)

User reports 2026-11-28 04:07–04:10 MST, after sync 79 shipped:
syncs now complete (both repos synced 03:54 + 03:56 MST) and
Google/GitHub login both work — but the UI does not refresh
itself.

- **Repositories page (04:07 MST):** the green "Sync queued for
  X (job …)" notice stays until the page is changed, and the
  "Open folder" button stays disabled until then too — both only
  update on a page change. Root cause: the page loads on mount
  (and after its own actions) only; nothing listens for the
  job's completion, even though the app already forwards the
  agent's progress as `sync-progress` Tauri events (the
  Synchronization page listens). Fix: `Repositories.tsx`
  subscribes to `sync-progress`; a terminal event
  (`job_completed` / `job_failed` / `job_cancelled`) for a repo
  this page queued clears that repo's entry, and when the last
  one finishes the queued notice goes away (only if the current
  notice is still a queued one — another action's notice is never
  cleared); a failed job shows its message as the error instead.
  The list reloads silently on a terminal event for any visible
  repo, so the row's badge, last-synced time and "Open folder"
  update in place — including scheduler jobs finishing while the
  page is open. Bulk sync tracks each queued repo the same way
  (and no queued notice is shown when nothing queued). New
  tests: queued notice clears + row refreshes on `job_completed`
  (Open folder becomes enabled); failure shows the message +
  clears the notice.
- **Broker card (04:07 MST):** after installing on a new account
  the account does not show until Refresh is pressed. Fix:
  `BrokerSetup.tsx` polls `listBrokerInstallations` every 2 s
  for up to 90 s after the Install click and stops the moment a
  new installation appears, naming it in the notice; on timeout
  it says to press Refresh. The poll is a background interval
  (not `run()` — no busy state, no notice wipe), cleaned up on
  unmount. Install notice rewritten: the page watches for the
  installation you choose; no Refresh needed. New test: the
  installation appears after the click via the poll (fake
  timers).
- **Discovery reverted (user, 04:07 + 04:09 + 04:10 MST):** the
  sync-79 `GET /installations` recording loop is removed. The
  user installed two accounts (ZohanZaheer 186785209,
  ahmaddomains7-hue 186784678) straight from
  `github.com/apps/reponyx` — no desktop Install click, no
  callback — and both appeared on Refresh; and pointed out that
  with 100 users on the shared App every desktop would see every
  account. Only the install callback records installations now:
  it is the only path with proof the user owns it (it verifies
  the installation belongs to this App and consumes the one-time
  setup state from the browser flow). The callback already
  records `setup_action=update` too, so an already-installed
  account picked on the GitHub install page arrives via the
  callback + the poll — no uninstall/reinstall, no Refresh, and
  no other account pulled in. The already-recorded rows stay in
  the broker store (the 6 installations currently listed,
  including the 2 unpaired ones, can still be paired from the
  list); the revert only stops future over-fetching. Broker test
  `callback_rejects_installation_not_in_github_list` reverted to
  asserting an empty list.
- **Same mount-only pattern on other pages:** the user notes the
  refresh problem is on almost every page. This batch fixes the
  two named spots (Repositories sync, BrokerSetup install); the
  other pages still load on mount only. A pass over all pages
  with the same listener pattern is a separate batch if the user
  wants it.
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test
  (excl. desktop) 478 passed, 0 failed; desktop frontend 15
  files, 130/130 (was 127, +3 new tests); typecheck clean; lint
  clean; vite build clean (only the known
  INEFFECTIVE_DYNAMIC_IMPORT warning); local Semgrep 0 findings;
  `check-no-private-key.sh` passed. MinGW desktop check passes
  (`cargo check -p reponyx-desktop --target
  x86_64-pc-windows-gnu` clean; desktop Rust unchanged in this
  batch). Two test-authoring mistakes caught by the run and
  fixed: the listener refs were declared before the `refresh`
  callback they mirror (TDZ — whole Repositories file failed),
  and `act` was imported from `vitest` instead of
  `@testing-library/react`.

## Web platform — W4 broker ↔ web (2026-11-28, branch `feature/admin-control-plane`, local only, not pushed)

- **Plan approved 2026-11-28 05:02:37 MST** (user: "Haan, W4 plan
  approve — banao" + "bs theek hai ye b 2no kr do"). All three open
  questions decided YES: Free limit 6 (applied by direct DB update
  2026-11-28 ~05:00 MST, verified BEFORE 2/2 → AFTER 6/2),
  BrokerSetup form removal, App-sync-needs-web behaviour.
- **Web side:** `POST /api/broker/mint` (jose RS256; session →
  linked-to-user → GitHub lists it → device not revoked → rank
  within package limit, oldest-first; audit `broker.mint`),
  `GET /api/broker/installations/[id]`, `POST/DELETE
  /api/me/github-accounts` (app: verify via App key, 409
  `already_linked`, idempotent; pat: login only), `DELETE
  /api/devices/[id]` + dashboard Revoke button. New queries
  (`findInstallationByGithubId`, `findLinkedAccountById`,
  `findLinkedAccountByProviderAccount`, `linkInstallation`,
  `linkPatAccount`, `unlinkLinkedAccount`, `findDeviceByKey`,
  `linkedAccountRank`). i18n keys `installationNotLinked` /
  `deviceRevoked` / `limitExceeded` (all 4 errors.json files) +
  dashboard `revoke` / `revokedSuccess`. Tests:
  `apps/web/test/broker.test.ts`, 12 tests (link+mints, mint
  without link 404, another user's mint 404, verify 200/404,
  mint after GitHub-side uninstall 404, revoked device 403,
  limit 403 at Free=2 in the test DB, idempotent link, rank
  oldest-mints/newest-refused, PAT link+unlink, unlink App
  removes the link, mint without App config 503).
- **Broker side:** no key anymore. `tokens.rs` is a web client
  (`mint_installation_token_via_web`, `verify_installation_via_web`);
  signing functions + `jsonwebtoken` dep deleted. `BrokerConfig`
  drops app_id / private_key_pem / github_api_base, gains
  `web_base_url` (`REPONYX_WEB_URL`, default reponyx.bond).
  Session read per web call from the credential store
  (`BrokerState::new_with_store` test seam, memory store in
  tests). Mint cache + device gate unchanged. `WebSessionExpired`
  error code added (`crates/core`). Broker tests rewritten
  (wiremock now mocks the WEB, not GitHub; +2 new tests:
  mint/callback without a session fail closed 401).
- **Desktop side:** spawn passes only `REPONYX_WEB_URL` + setup
  token (`BrokerSpawnConfig` drops App id / key path;
  `is_complete` = token or env file). Setup form removed
  (`BrokerSetup.tsx`, `pick_broker_key_file_command` deleted);
  `broker.app_id` / `broker.private_key_path` settings stay
  stored but ignored. New commands `web_link_github_account_command`,
  `web_link_pat_account_command`,
  `web_unlink_pat_account_command` (+ api-client + shared
  `WebLinkedAccount`). Linking: after pairing, after PAT add, and
  on every Accounts-page load (reconcile covers paired
  installations AND PAT accounts — a PAT link that failed while
  offline is retried on the next load). PAT remove/delete unlinks
  best-effort. `already_linked` added to desktop
  `errorMessageKey`; `web_error` gains the four new codes.
- **Behaviour change (by design):** App sync needs the web
  reachable after the cached token expires (~1 h grace); PAT sync
  untouched. The web never sees repo contents, PATs, or tokens.
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test
  (excl. desktop) 479 passed, 0 failed (was 478: −1 deleted JWT
  unit test, +2 new no-session tests); web tests 48/48; i18n
  validate 166 keys, 0 failures; desktop frontend 15 files,
  138/138 (was 130, +8 new tests); typecheck clean; lint clean;
  vite + next build clean; local Semgrep 0 findings;
  `check-no-private-key.sh` passed; MinGW desktop check passes.
  Prettier: the 10 flagged desktop/shared/api-client files are
  pre-existing state (dirty at HEAD too) and outside the repo's
  `format:check` scope — left alone, not reformatted.
- **Not pushed yet (needs approval).** Live test after the push:
  user pulls, rebuilds, logs in, Accounts page links the 6
  installations, dashboard 0/6 → 6/6, Sync now on a web-minted
  token.
