# W4-full plan — real-time two-way app ↔ web sync

Status: IMPLEMENTED — built + verified 2026-11-28 ~08:15 MST, committed locally (see CHANGELOG + PROJECT_STATUS); push pending (sync 85, needs approval). W5 shipped first on the user's say-so (sync 84, `5706323e`), so W4-full rides on top of it.

## User requirement (2026-11-28 ~07:45 MST)

"app or web apas mai b sync hon. app web se data la k es user k ye package
ye limit hai. and same web app se real time la. 2no real time mai sunc ho r chla"

- The app fetches this user's package + limits from the web — continuously,
  not only at login.
- The web gets the app's data in real time (an account added in the app is
  on the web at that moment).
- Both stay in sync in real time, while running.

Order from the user: **W4 full → W5 → W6 → W7.** W5 is already built
locally (`d35043a`, not pushed); its push rides with/after W4-full — a
snapshot push carries the whole tree, so W4-full + W5 ship in one
snapshot (sync 84).

## Gaps found in the code (survey 2026-11-28)

1. **Web → app is one-shot.** `web_session_status_command`
   (`web_auth.rs`) fetches `/api/me/limits` (package + limits + usage) and
   heartbeats the device — but the frontend calls it only once at app
   startup (`App.tsx` `refreshStatus` in a mount-only effect) and at
   login (`finish_login`). An admin package/limit change never reaches a
   running app; the Settings Account card (`webAccount` from App state)
   goes stale until restart.
2. **App → web has no push on change.** The link happens on
   pairing-complete / PAT-add / Accounts-page mount reconcile — but the
   app's own displayed usage does not refresh after a link/unlink; nothing
   re-fetches the account state after those actions.
3. **Web dashboard is load-only.** `apps/web/src/app/dashboard/page.tsx`
   is a server component that reads counts from the DB on page load. A link
   from the app does not show until the user reloads the page.
4. **Device identity is per-DB, not per-machine (the 2/2 bug).**
   `device_identifier()` (`auth.rs`) reads the settings table FIRST
   (per-database), the OS credential store second. Two DBs/instances on
   one machine (dev build + installed exe, or a stale settings value) can
   use different ids — and the sync-83 re-activation makes each an active
   web row, so one machine shows Devices 2/2. The user (2026-11-28
   ~07:45): on one system it must be 1/2; a 2nd device means a different
   system running dev or the installed .exe.

## Implementation

### 1. Device identity: OS store first (Rust, `auth.rs`)

`device_identifier()` reads the OS credential store FIRST — that is the
machine identity. If present, write it through to both settings keys
(`device_identifier` and `device.identifier`, the agent's cache) and
return it. Else fall back to the settings value (migrate old installs into
the OS store), else generate a fresh UUID and store it in all three
places. `fresh_device_identifier_if_revoked` is unchanged (it already
writes all three). One machine = one device id, even with two DBs.
The stale duplicate active row already on the web is a one-time cleanup:
the user revokes the extra row once from the dashboard; after that the
count stays 1/2.

### 2. Desktop: periodic web sync (frontend, `App.tsx`)

While signed in, `refreshStatus()` runs every 60s (interval) and on window
focus — the same call the startup effect already makes, so each tick
re-fetches `/api/me/limits` (package + limits + usage) and heartbeats the
device (web last_seen stays fresh). Guarded so ticks never overlap an
in-flight call; the interval is cleared when the user signs out. The
offline rule is unchanged: a failed fetch keeps the session and returns
the last-known cached account.

### 3. Desktop: refresh on change (frontend)

After a successful link/unlink (pairing complete, PAT add, PAT remove,
account delete), the app dispatches a `reponyx:account-changed` window
event; `App.tsx` listens and runs `refreshStatus()` at once, so the
Settings card and the app's counts update at that moment instead of at
the next 60s tick. The Accounts page also re-runs its reconcile after an
add, so a link made while the page is open lands immediately.

### 4. Web dashboard: live refresh (Next.js)

New client component `DashboardLiveRefresh`
(`apps/web/src/components/auth/DashboardLiveRefresh.tsx`): `router.refresh()`
every 30s and on window focus, plus a small "Live · HH:MM:SS" indicator
so the user can see the dashboard is live. Rendered at the top of the
dashboard page. No new API route — the page re-reads the DB on each
refresh, so linked accounts / devices / counts move without a reload.

### 5. No new API routes, no migration

`/api/me/limits`, `/api/devices` (heartbeat), the link/unlink routes and
the dashboard queries already exist. W4-full is wiring + the identity fix.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --exclude reponyx-desktop -- -D warnings`
- `cargo test --workspace --exclude reponyx-desktop` (479)
- Desktop frontend tests (138) + new test: the account-changed event is
  dispatched after a successful link
- `pnpm -r typecheck`, `pnpm -r lint`, `pnpm --filter reponyx-desktop run build` (vite)
- `pnpm --filter @reponyx/web run build` (next), i18n validate (232 keys + 2 new)
- Semgrep (exact CI command), `bash scripts/check-no-private-key.sh`
- MinGW `cargo check -p reponyx-desktop` + clippy (Windows-only change)
- Docs: CHANGELOG.md + `ai/memory/PROJECT_STATUS.md`; commit; push offer
  (sync 84, snapshot carries W5 + W4-full)
