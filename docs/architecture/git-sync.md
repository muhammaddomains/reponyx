# Git Synchronization Architecture

Status: Accepted (Phase 1) · Engine: Phase 3

## Modes

- **A — Mirror (default).** `git clone --mirror <url>` into
  `<root>/repositories/github/<account>/<owner>/<name>.git/`; sync with
  `git --git-dir=<path> fetch --prune`. Preserves objects, branches,
  tags, remote refs, full history. A bare repo is never treated as a
  working directory.
- **B — Working clone.** `git clone <url> <dest>`; sync fetches all
  remotes + tags and prunes, without overwriting uncommitted local
  changes. Divergence and dirty state are detected; fast-forward pull
  is opt-in. Never auto-reset or discard user modifications.
- **C — Both.** One mirror + one working clone, separate locations.

## Execution model (`crates/git-engine`)

- Git CLI is the execution engine (ADR-002). No shell: argument
  arrays only; controlled environment; per-operation timeout;
  cancellation token; structured, redacted output.
- Credentials are never in clone URLs or argv. A short-lived
  credential helper / controlled environment injects them per
  operation; secrets are redacted from all output and logs.
- Per-repository lock: one operation at a time per local path.
- Error mapping: exit codes + stderr are classified into stable
  `RepoNyxError` codes (`RepositoryNotFound`, `AuthenticationRevoked`,
  `NetworkUnavailable`, `DiskSpaceInsufficient`, `RepositoryLocked`,
  `RepositoryCorrupted`, …) with retryability + suggested action.

## LFS

A mirror does **not** guarantee every LFS object is archived. LFS sync
is a separate step (Phase 9): availability check, `lfs fetch`,
object verification where practical, failure reporting, storage
accounting. LFS failures never silently pass as success.

## Submodules

Optional recursive submodule sync (Phase 9). Submodule URLs are
detected; inaccessible private submodules fail gracefully per-repo.
A parent's authentication is never assumed to cover submodules.

## Storage layout

```text
<root>/                      (default D:\RepoNyx\ on Windows)
  repositories/github/<account>/<owner>/<name>.git/
  working-clones/<account>/<owner>/<name>/
  lfs/ logs/ exports/ backups/
  data/reponyx.sqlite  data/database-backups/
```

Path components (account/owner/name) are validated before use
(`git-engine::paths`): no separators, no `..`, no Windows-reserved
names, no illegal characters. Name collisions never silently overwrite
an unrelated local repository.

## Archival guarantees

A mirror is a Git history archive — not a complete backup of external
assets. LFS objects, submodules, GitHub releases, issues, wikis, and
Actions artifacts need their own handling (documented in Phase 9).
Local repos are never deleted when a remote is deleted, an account
disconnects, an installation is revoked, a PAT expires, or a repo is
deselected: the repo is marked `disconnected` / `remote_deleted` and
deletion requires explicit user confirmation.

## Scheduler (`crates/scheduler`, Phase 7 — scheduler part only)

The scheduler decides *when* a sync should happen. It creates
sync-job rows for due schedules and advances them; it never runs a
sync — executing jobs is the Phase 6 sync queue's job, and this
crate has no dependency that could run one (core + database only).

- **Storage:** schedules live in the V1 `schedules` table
  (`interval_type`, `interval_value`, `timezone`, `enabled`,
  `next_run_at`). All stored times are UTC. There is no
  `last_run_at` column: advancing `next_run_at` to the next computed
  run is the progress marker that stops a due schedule being
  reported due forever.
- **Cadence (`interval_value`):** plain or JSON form. Plain:
  `"6"` (hours), `"02:30"` (daily `HH:MM`), `"1,3,5 02:30"`
  (weekly, ISO weekday numbers 1 = Monday .. 7 = Sunday), `"90"`
  (custom minutes). JSON: `{"hours": 6}`, `{"time": "02:30"}`,
  `{"minutes": 90}`, plus optional `"weekdays": [1, 2, 3, 4, 5]` and
  `"window": {"start": "01:00", "end": "05:00"}`. A window is
  `[start, end)` in local time; `end <= start` crosses midnight;
  `start == end` is rejected. A daily/weekly time outside its own
  window never fires (the preview shows no runs).
- **Next-run calculation:** the first run strictly after "now".
  Interval schedules are absolute-time cadences anchored on the
  stored `next_run_at` (or `created_at`); daily/weekly are
  wall-clock times in the schedule's IANA timezone. DST rules: a
  local time that does not exist (spring-forward gap) fires at the
  first valid time after the gap; a local time that occurs twice
  (fall back) fires once, at the first occurrence. A disabled
  schedule has no next run; an unparsable cadence/timezone is
  `InvalidInput`, never a fake time.
- **Precedence:** repository > account > global, resolved by Phase
  2's `effective_schedule_for` — not reimplemented here.
- **Due + materialize:** due = enabled with stored `next_run_at`
  passed. `materialize_due` primes schedules whose `next_run_at` is
  `NULL`, then per due schedule creates one job per governed
  repository (global: all; account: its own; repository: that one),
  skipping sync-disabled repositories and repositories with a
  pending/running job (scheduling-time dedup only), then advances
  the schedule. Job type: `fetch` when the repo already has a local
  path, else a clone job for its sync mode. Per-schedule failures
  are recorded in the report, never aborting the run; a schedule
  whose cadence no longer parses is left un-advanced so it stays
  visible. A due run outside the current window creates no job and
  is advanced to the next valid run.
- **Not in this crate:** the background agent, IPC, Task Scheduler
  registration, tray, and job execution — the rest of Phase 7.
  Code for all of it landed on `main` 2026-11-21 (user commits),
  but it is not verified complete: the new crates are not workspace
  members and cannot be built, the agent does not compile, and
  nothing is wired into the desktop. See
  `ai/memory/PROJECT_STATUS.md` ("Phase 6 / Phase 7 verification").
