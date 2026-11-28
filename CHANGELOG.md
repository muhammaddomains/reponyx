# Changelog

All notable changes to RepoNyx are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed — dashboard Devices are a presence record again (2026-11-28, branch `feature/admin-control-plane`, local only)

- **Devices (user decision 2026-11-28 ~07:00 MST):** the dashboard showed 0/2 with all three `DESKTOP-FBFSK3A` rows revoked — the user had revoked the current device too while cleaning up the duplicates, and `upsertDevice` never cleared `revoked_at`, so re-login could not restore the count. The user decided the dashboard Devices list is a presence record, not a revocation record: when the desktop runs it shows with time/date. Fix: `upsertDevice` now clears `revoked_at` on every re-registration (login + session-status heartbeat), so a revoked device becomes active again the moment the desktop runs. The web Revoke button now only removes the device until it next runs; the strong cut-off for a device is revoking its pairing in the desktop (the broker then refuses to mint for it). No migration (query change only). New test pins the SQL; live-DB proof on the real revoked row (`c4a5de34`, revoked 13:12:20Z) in a rolled-back transaction: `revoked_at` → NULL, last_seen fresh, active count 1.
- **Verified:** database package tests **10/10** (was 9, +1 new); web tests **48/48**; typecheck clean (database + web); lint clean; prettier clean; local Semgrep 0 findings (85 rules, 206 files). No Rust changes.
- **Not pushed yet (needs approval).**

### Fixed — W4 live test: pairing could never complete + one machine counted as several devices (2026-11-28, branch `feature/admin-control-plane`, local only)

- **Pairing (reported 2026-11-28 ~06:08 MST):** "Pair this device" → "Complete pairing" failed with `InvalidInput: GitHub App installation is not linked to your web account`, and the retry failed with `InvalidInput: pairing code already used`. Root cause, read from code: `complete_pairing_command` validates the installation by minting a token through the web, but the web only mints for installations already linked to the requesting user — and the link was only created *after* pairing completed. So pairing could never succeed for an unlinked installation, and the first attempt had already spent the single-use code. Fix: `complete_pairing_command` now links the installation to the signed-in web account (new `link_github_app_installation` helper in `web_auth.rs`, also used by `web_link_github_account_command`) *before* validation. The link is idempotent, and the frontend still links after pairing as a safety net. Pairing a GitHub App account now requires being logged in on the web; without a session it fails with `unauthorized` instead of the misleading "not linked" error.
- **Devices (reported 2026-11-28 ~06:11 MST):** the dashboard showed three `DESKTOP-FBFSK3A` rows (last seen 3:58, 6:01, 6:11 AM) and counted 2/2 for one machine. Root cause: `device_identifier` lived only in the settings table, so every fresh database minted a new UUID — a new web device row and an orphaned broker pairing each time. Fix: the OS credential store is now the source of truth (`reponyx:device` / `identifier`; the identifier is not a secret, the store is just the storage that survives a deleted database), with the settings table as a write-through cache the agent still reads. Existing installs migrate on first use (settings value persisted to the store); a fresh database reuses the stored id; revoking mints a fresh id in both places (`fresh_device_identifier_if_revoked`). No new duplicate rows after this ships; the stale rows already on the web must be revoked from the dashboard (the user already revoked the 3:58 AM row; the 6:01 AM row is the other stale one — the 6:11 AM row is the current device).
- **Linking (reported 2026-11-28 ~06:09 MST, RESOLVED 2026-11-28 ~07:03–07:08 MST):** the Accounts-page reconcile failed all 6 links with `internal_error`. Narrowed from evidence, not fixed in code: the W4 deploy is READY on `reponyx.bond` (production `dpl_7smdsqZLBAxFJi72KhW1azbB8rzh`), the session and database work (`/api/me/limits` answers), the link SQL is test-proven, and the only step that touches something never exercised in production before is the web's GitHub App verification (`getAppInstallation` → App JWT). Both env vars exist for production (`REPONYX_GITHUB_APP_ID`, `REPONYX_GITHUB_APP_PRIVATE_KEY`, set 2026-11-27 ~00:56–00:57 MST), so the value of one of them is wrong — a bad paste, a truncated key, surrounding quotes, or the wrong App id — and every GitHub App call on the web fails with `internal_error`. **Root cause, found 2026-11-28 ~07:03 MST: the `.pem` file is PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`), but the web's jose `importPKCS8` accepts only PKCS#8 (`-----BEGIN PRIVATE KEY-----`) — proven with a throwaway key. The user had pasted the file exactly, so the values were "correct" and the format was the bug.** Fix (user gave me the App id `5896712` + the `.pem` file): the same key converted to PKCS#8 (public key verified identical; converted key parses with the web's jose), both Vercel vars set, then a proper production deployment (`dpl_FAfm5HB2mkCc16KB6svJqWetfbrx`, target production, READY on `reponyx.bond`) — the first bare redeploy had target None and showed as Preview, so a production redeploy was needed for the domain. Key file deleted from the workspace after use. Live retest pending: user retries pairing with a fresh code (old codes are spent). Product follow-up: the web should accept PKCS#1 keys server-side (GitHub generates PKCS#1 `.pem` files — every user would hit this).
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test (excl. desktop) **479 passed, 0 failed** (one timing flake in `retry_budget_binds_across_retries` on the first run — passes in isolation and on re-run, same class as CI #103; unrelated to this change, no sync-queue code touched); desktop frontend 15 files, **138/138**; typecheck clean; lint clean; vite build clean; local Semgrep 0 findings; `check-no-private-key.sh` passed; MinGW desktop check passes (clippy 9 pre-existing warnings, none in this change).
- **Not pushed yet (needs approval).**

### Added — W4: broker ↔ web (2026-11-28, branch `feature/admin-control-plane`, pushed as snapshot `b9f3549`, sync 81)

- **Web mint endpoint `POST /api/broker/mint`:** the web now mints installation tokens with the GitHub App key (held server-side only, `jose` RS256). Checks in order: session valid → installation linked to this user (else 404 `installation_not_linked`) → GitHub still lists the installation (else 404) → linked row exists → device not revoked (else 403 `device_revoked`) → package limit by rank, oldest-first (else 403 `limit_exceeded` with `{max}`). Returns `{token, expiresAt}`; audit row `broker.mint`; the token is never logged or stored.
- **Verify endpoint `GET /api/broker/installations/[id]`:** session auth; returns `{installationId, accountId, accountLogin}` or 404. Used by the broker install callback.
- **Link/unlink routes:** `POST /api/me/github-accounts` (`{authKind:"app", installationId}` — verifies via the App key, 409 `already_linked` when another user owns it, limit check, idempotent; `{authKind:"pat", login}` — records the login only, no token ever sent) and `DELETE /api/me/github-accounts/[id]` (deletes the linked row + the installations row for App links). `DELETE /api/devices/[id]` revokes a device (owner only); the dashboard Devices list has a Revoke button, and the mint check makes revocation bite broker-side.
- **Broker holds no key anymore:** `tokens.rs` is now a web client — `mint_installation_token_via_web` (POST `{web}/api/broker/mint`, Bearer session from the OS store, 401 → `WebSessionExpired`, 403 `device_revoked` → `AuthenticationRevoked`, 403 `limit_exceeded` → `InsufficientPermissions`, 404 → `InvalidInput`, 503 → `InternalError`) and `verify_installation_via_web`. The signing functions (`app_jwt`, `mint_installation_token`, `list_app_installations`, `verify_app_installation`, `fetch_app_installations`) and the `jsonwebtoken` dependency are deleted. `BrokerConfig` drops `app_id` / `private_key_pem` / `github_api_base`, gains `web_base_url` (`REPONYX_WEB_URL`, default `https://reponyx.bond`). The broker reads the session per web call from the credential store (test seam: `BrokerState::new_with_store` + memory store); logout deletes the entry, so the broker loses web access automatically. Mint cache + device gate unchanged.
- **Desktop broker spawn is zero-config:** `BrokerSpawnConfig` drops the App id / key path; it passes only `REPONYX_WEB_URL` + the auto-resolved setup token. The Accounts-page setup form is removed (`BrokerSetup.tsx`, `pick_broker_key_file_command` deleted); the old `broker.app_id` / `broker.private_key_path` settings stay stored but are ignored (no migration, nothing deleted). New commands `web_link_github_account_command`, `web_link_pat_account_command`, `web_unlink_pat_account_command` (+ api-client + shared `WebLinkedAccount` type). Linking happens after pairing completes, after a PAT is added, and on every Accounts-page load (reconcile: every paired installation + every PAT account, idempotent; limit failures surface as a notice, offline/expired-session stays silent); PAT remove/delete unlinks best-effort (a web failure leaves the link for the web dashboard, never undoes the local delete). The reconcile also covers PATs, so a PAT link that failed while offline is retried on the next Accounts load.
- **Behaviour change (by design, user-approved):** App sync needs the web reachable after the cached token expires (~1 h grace); PAT sync is untouched and still works offline. The web never sees repo contents, PATs, or GitHub tokens.
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test (excl. desktop) **479 passed, 0 failed** (was 478: −1 deleted JWT unit test, +2 new no-session tests); web tests **48/48**; i18n validate 166 keys, 0 failures; desktop frontend 15 files, **138/138** (was 130, +8 new tests); typecheck clean; lint clean; vite + next build clean; local Semgrep 0 findings; `check-no-private-key.sh` passed; MinGW desktop check passes.
- **Pushed as snapshot `b9f3549` (sync 81), 2026-11-28 ~05:53 MST. Deploy+CI check ran 06:21 MST — all green (CI #112, Lint #84, CodeQL #110, Security audit #83; Vercel production `dpl_7smdsqZLBAxFJi72KhW1azbB8rzh` READY, home + /login 200).** The live test then found the pairing + device bugs fixed in the entry above.

### Fixed — live refresh after sync + install, discovery reverted to callback-only (2026-11-28, branch `feature/admin-control-plane`, pushed as snapshot `e217f4b5`, sync 80)

- **Repositories page (reported 2026-11-28 ~04:07 MST):** the green "Sync queued" notice stayed until the page changed, and the "Open folder" button stayed disabled until then too. The page loaded on mount only and never listened for the job's completion, even though the app already forwards agent progress as `sync-progress` events. It now subscribes: a terminal event (`job_completed` / `job_failed` / `job_cancelled`) for a queued repo clears the queued notice when the last one finishes (only if the current notice is still a queued one), a failed job shows its message as the error, and the list reloads silently on a terminal event for any visible repo — so the row's badge, last-synced time and "Open folder" update in place, including scheduler jobs. Bulk sync is tracked the same way. New tests pin both the completion and the failure paths.
- **Broker card (reported 2026-11-28 ~04:07 MST):** after installing on a new account the account did not show until Refresh was pressed. After the Install click the card now polls the installations list every 2 s for up to 90 s and shows the new installation the moment it appears; on timeout it says to press Refresh. New test pins the poll.
- **Discovery reverted (user, 2026-11-28 ~04:07–04:10 MST):** the sync-79 change that recorded every GitHub-listed installation on list/refresh is removed. The user proved it over-fetches: two accounts installed straight from `github.com/apps/reponyx` (ZohanZaheer, ahmaddomains7-hue) appeared on Refresh with no desktop Install click — and with 100 users on the shared App, every desktop would see every account. Only the install callback records installations now (it verifies the installation belongs to this App and consumes the one-time setup state from the browser flow, so it proves the user owns it); it already records `setup_action=update`, so an already-installed account picked on the GitHub install page arrives via the callback + the poll. Installations already recorded stay in the broker store and can still be paired; only future over-fetching stops.
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test (excl. desktop) 478 passed, 0 failed; desktop frontend 15 files, 130/130 (was 127, +3 new tests); typecheck clean; lint clean; vite build clean; local Semgrep 0 findings; `check-no-private-key.sh` passed; MinGW desktop check passes.
- **Pushed as snapshot `e217f4b5` (sync 80), 2026-11-28 ~04:30 MST. Deploy+CI check ran 04:50 MST — all green (CI #111).**

### Fixed — desktop OAuth return leg, already-installed App discovery, syncs failing with InvalidInput (2026-11-28, branch `feature/admin-control-plane`, pushed as snapshot `f355c40`, sync 79)

- **Desktop OAuth (reported 2026-11-28 ~03:14 MST):** email+password login worked, but GitHub/Google login opened the browser and never completed. Two gaps found: (1) the `reponyx://` scheme is registered by the NSIS installer only — `tauri dev` never registers it, so the browser had nowhere to send the OAuth result back to the running dev app. The desktop now registers the scheme at startup on Windows (`DeepLinkExt::register_all()`, best-effort — a failure only warns). (2) A deep link arriving while the first session check was still running was silently dropped; `App.tsx` now holds such links and processes them once the auth state resolves to signed-out (discarded if already signed in). Vercel logs showed the Google start route hit once with no callback — the user still needs to retry OAuth after this fix and report what the browser shows after authorizing.
- **Already-installed GitHub App (reported 2026-11-28 ~03:17 MST):** the broker only recorded installations that came through its own setup callback, so an App already installed on GitHub never appeared — the user had to uninstall + reinstall it. `GET /installations` now also records any installation GitHub lists that isn't in the store yet (same call it already made to fill in account logins; pairing stays empty until the device pairs; a record failure is ignored and retried on the next refresh). Clicking Install now surfaces an already-installed account immediately; the notice in the UI says so. The broker test `callback_rejects_installation_not_in_github_list` was updated: the rejected callback still records nothing, but the GitHub-listed installation now shows up. **REVERTED in the next entry (sync 80):** listing-based discovery pulls in every installation of the shared App, so only the install callback records installations now.
- **Syncs failing permanently with InvalidInput (reported 2026-11-28 ~03:26 MST; log `reponyx.log-ef2fccbd.2026-11-28`):** six queued syncs failed permanently, code `InvalidInput`, 10:15–10:27Z. Root cause: the agent builds its credential resolver ONCE at startup, snapshotting `device.identifier` — but the agent started (02:58 MST) before the device was paired (03:17+), so every GitHub App job failed with "GitHub App sync requires a paired device identifier" even after pairing completed. The resolver now reads `device.identifier` from settings on every job (the snapshot field is removed). `storage.root` was ruled out: the agent persists its default into settings at startup, so that lookup succeeds. The executor's failure logs now include the error message, not just the code — a bare `InvalidInput` could not be diagnosed from the log alone. New test `github_app_reads_device_identifier_paired_after_resolver_built` pins the regression. **Workaround until this ships:** restart the agent once (Ctrl+C `dev:all` and run it again, or Agent page → Stop/Start) so it picks up the pairing, then Sync now works.
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test (excl. desktop) 478 passed, 0 failed (was 477, +1 new test); desktop frontend 15 files, 127/127; typecheck clean; lint clean; vite build clean; local Semgrep (exact CI command) 0 findings; `check-no-private-key.sh` passed. MinGW desktop check passes (mingw-w64 reinstalled on this VM after the redeploy; `cargo check -p reponyx-desktop --target x86_64-pc-windows-gnu` clean).
- **Pushed as snapshot `f355c40` (sync 79), 2026-11-28 ~03:49 MST. Deploy+CI check ran 04:10 MST.**

### Fixed — desktop build lock in `dev:all`: agent/broker now run the release binaries (2026-11-28, branch `feature/admin-control-plane`, local only)

- **Root cause:** `tauri.conf.json` `bundle.resources` copies `target/release/reponyx-agent.exe` and `target/release/reponyx-auth-broker.exe` into the build's target dir (`target/debug` under `tauri dev`). But `pnpm broker` / `pnpm agent` ran `cargo run` in dev profile, which executes those very `target/debug/*.exe` files — and Windows cannot overwrite a running executable. The desktop build script runs minutes after the agent/broker have started (they compile in ~19 s), so its `fs::copy` failed with os error 32 every time. Not a stale process: no extra process holds the files; the running agent/broker themselves are the lock.
- **Fix:** `pnpm broker` and `pnpm agent` now run `cargo run --release -p …`, i.e. the release binaries that `dev:all` already builds first (`agent:build && broker:build`). The desktop build script then writes to `target/debug/*.exe`, which nothing runs. Trade-off: agent/broker Rust changes rebuild in release profile (slower than a dev rebuild) instead of the dev profile; behaviour is otherwise identical, and the running binaries match what ships in the installer.
- **Still open:** all 3 fresh sync jobs failed permanently with `AuthenticationExpired` (accounts not yet identified); the live database was found to be a fresh empty one in the 09:19 run — see PROJECT_STATUS.

### Fixed — database opens again: migration checksum mismatch was CRLF line endings (2026-11-28, branch `feature/admin-control-plane`, local only)

- **Root cause:** sqlx checksums each migration file (SHA-384 over its bytes), and the user's Windows clone had CRLF in all 142 lines of `0001_init.sql` while `_sqlx_migrations` held LF hashes for v1–v4 and a CRLF hash for v5. The agent failed with `migration 1 was previously applied but has been modified` (surfaced by the sync-76 details fix). The earlier comment-rename theory was wrong: v1's stored checksum matches the current LF file exactly.
- **Fix:** the 5 migration files converted to LF; the stored v5 checksum updated to the LF hash (file content unchanged apart from line endings, so the applied schema already matches); `git config core.autocrlf false`. The first conversion one-liner emptied all 5 files (`open(f,'wb')` truncates before the `read()` runs — reproduced locally); they were restored with `git checkout -- migrations/sqlite/` (verified `CRLF: 0 bytes: 5849`). The agent then printed `database opened` and runs (scheduler materialized 3 jobs).
- **Prevention:** repo `.gitattributes` pins `*.sql text eol=lf` (local commit `5bd2f68`, not yet pushed).
- **Still open from the same run:** the desktop build fails in `tauri-build` with a file lock (os error 32), and all 3 fresh sync jobs failed permanently with `AuthenticationExpired` (accounts not yet identified).

### Fixed — error logs now show the underlying cause (2026-11-28, branch `feature/admin-control-plane`, local only)

- **`RepoNyxError` Display now includes `details`** when present: `[{code}] {message}: {details}` instead of dropping them. The `details` field already carried the sqlx / I/O message (via `map_error`), but the thiserror format string never rendered it — so the agent's 2026-11-28 `dev:all` failure printed only "[DatabaseError] database operation failed" and hid whether the open failed on a lock, a checksum mismatch or a migration query. The `thiserror` dependency is removed from `reponyx-core` (manual `Display` + `Error` impls; `Cargo.lock` updated). New unit test `display_includes_details_when_present`.
- **Verified:** fmt clean; clippy `-D warnings` clean; cargo test (excl. desktop) 477 passed, 0 failed (was 476, +1 new test); local Semgrep (exact CI command) 0 findings.
- **Diagnosis update (2026-11-28 ~01:06 MST):** the user restarted the system and ran `pnpm dev:all` — Task Manager shows no RepoNyx process — so the lock theory is ruled out. The pre-migration backup (VACUUM INTO) succeeded and there is no "database opened" line, and `integrity_check` returns a named `InvalidInput` error, so the failure is in the migration step (`repair_unrecorded_migrations` or `migrator.run()`), failing fast. The next run with this fix will print the exact SQLite reason.

### Fixed — DatabaseError maps to a real message (2026-11-27, branch `feature/admin-control-plane`, local only)

- **`DatabaseError` now maps to `databaseUnavailable`** (new key in desktop `errors.json`, en+ur): "Database unavailable — close other RepoNyx windows and try again". Root-cause follow-up to the OAuth report: with the database down, login and the desktop-code exchange die at `require_db` with code `DatabaseError`, which the frontend did not map — so it rendered the generic "Something went wrong" and blamed login/OAuth for a database problem. **Verified:** i18n validate 158 keys, 0 failures; typecheck clean; desktop frontend 15 files, 127/127 (+1 new Login test: a `DatabaseError` rejection shows the new message); lint + vite build clean; Prettier clean; local Semgrep (exact CI command) 0 findings. **Pushed + deployed 2026-11-27 ~08:30 MST (snapshot `5845f0c`, sync 75):** Vercel production deployment READY (reponyx.bond; home + /login 200). CI #106 ALL GREEN (11m 11s): rust 4m 1s, frontend 43s (127/127), windows-build 10m 22s, windows-installer 11m 8s (56.6 MB, sha256 `6d50dbc0fb2d23f0a36761f2b11ef25b3a08a3f367d8a6053744278abebf3d2b`), Lint, CodeQL, Security audit (Semgrep clean) — the last three are separate workflow runs, reported Success at 08:42.

### Fixed — auth-page redirects, desktop OAuth error display, reset link single-use (2026-11-27, branch `feature/admin-control-plane`, local only)

- **`/login` + `/register` redirect a signed-in user to `/dashboard`** (both pages now check `getSessionFromCookies` first — the dashboard already redirected the other way, so the guard was one-sided; reported 2026-11-27 ~07:38 MST with screenshots).
- **Desktop OAuth error banner:** a stale deep-link error no longer outlives its attempt — `Login` clears it through a new `onClearDeepLinkError` prop on every new login/OAuth attempt (`App` wires it to `setDeepLinkError(null)`). The error map in `locale.tsx` now covers the web codes that used to fall through to the generic message: `internal_error` → `internalError`, `invalid_body` → `invalidBody`, `registration_rate_limited` → `rateLimitedRegister`, `session_expired` → `sessionExpired`. (Background: the user's desktop OAuth attempt showed the generic banner; Vercel logs showed the flow reached the Google start route once but never the callback or the desktop exchange, with no server error — the user runs the app installed from `pnpm build:all`, so the `reponyx://` protocol is registered; the root cause of that stalled attempt is still open and needs a retest after these fixes, when the real error will show instead of the generic one.)
- **`/reset/confirm`:** the submit button now says "Change password" (new `changePasswordButton` key, en+ur) instead of the reused "Send reset link", and the page subtitle is its own `confirmResetSubtitle` instead of the reset-request text. The page now validates the token on load — a used/expired link shows "This link is invalid or expired" instead of the form. Server-side the link was already single-use (`consumed_at`), but the claim is now atomic: `consumeResetToken` returns whether it claimed the token (`UPDATE ... WHERE consumed_at IS NULL RETURNING id`) and the route consumes before setting the password, so two racing requests cannot both use one link.
- **Verified:** i18n validate 157 keys, 0 failures; typecheck clean; database tests 9/9; web tests 36/36 (reset test extended: a second confirm with the same token → 404 `invalid_token`); desktop frontend 15 files, 126/126 (+2 new Login tests: stale error cleared on a new OAuth click, `internal_error` renders the generic message); lint clean; vite + next build clean; Prettier clean; local Semgrep (exact CI command) 0 findings.
- **Pushed + deployed 2026-11-27 ~07:55 MST (snapshot `42e662a`, sync 74):** Vercel production deployment READY (reponyx.bond; home + /login 200; `/reset/confirm?token=doesnotexist` shows the invalid-link message, proving the page-level validation is live). CI #105 ALL GREEN (12m 3s): rust 3m 5s, frontend 44s (126/126), windows-build 11m 40s, windows-installer 11m 59s (56.6 MB), Lint #77, CodeQL #103, Security audit #76 (Semgrep clean). Warnings only: Node.js 20 deprecation.

### Fixed — TLS verification for sslmode=require|prefer (2026-11-27, branch `feature/admin-control-plane`, local only)

- **`packages/database/src/client.ts`:** the `require`/`prefer` → `rejectUnauthorized: false` branch is removed (local commit `41425af`). All remote hosts now verify the server certificate, deliberately stricter than the Postgres meaning of `require` (encrypt without verification) — user decision 2026-11-27 ~06:43 MST (verify, not suppress). Localhost and `sslmode=disable` are unchanged. The live DB already uses `verify-full`, so nothing changes for it; a future DB with an unverifiable certificate under `require`/`prefer` would now fail to connect instead of silently skipping verification.
- **Verified:** local Semgrep (exact CI command, 1.186.0) **0 findings** (was 1 blocking), database tests 9/9, typecheck + Prettier clean.
- **CI on the feature branch:** Lint #75 PASS, CodeQL #101 PASS, Security audit #74 FAIL (the Semgrep finding above), CI #103 FAILURE (only the rust job failed, on the flaky `hundred_jobs_complete_with_bounded_concurrency` queue-drain test; frontend, windows-build, windows-installer green) — the rust job passed on re-run as attempt #2 (3m 13s), flake confirmed. **Pushed + deployed 2026-11-27 ~06:51 MST (snapshot `cd2e10c`, sync 73); Vercel `dpl_GCXFybH5766WjnGwPfKYk1S7qd3e` READY (production, aliases incl. reponyx.bond); live smoke test green (home + /login 200). CI on the new commit: CI #104 Success (rust 3m 46s, frontend 33s — Vitest 15/15 files, 124/124 tests — windows-build 6m 30s, windows-installer 13m 43s, artifact 56.6 MB), Lint #76 Success, CodeQL #102 Success, Security audit #75 Success (Semgrep 0 findings).** Warnings only: Node.js 20 deprecation on the windows jobs, CodeQL Action v3 deprecation coming December 2026.

### Added — Web platform W2: OAuth + sendcms design (2026-11-27, branch `feature/admin-control-plane`, local only)

- **Google + GitHub login:** start routes `/api/auth/oauth/{google,github}` redirect to the provider authorize URLs; callbacks at the exact registered URLs (`/api/google/oauth/callback`, `/api/auth/callback/github`). OAuth state is HMAC-signed with `SESSION_SECRET` (10-minute TTL, provider-bound). **No auto-create** — an unregistered OAuth login is refused with `oauth_no_account`; a provider-verified email satisfies verification for an already-registered account. The same account-switch limit and 2FA step apply as for password login. The login page shows each provider's button only when its credentials are configured.
- **Proper pages (sendcms design):** Tailwind v4 + the sendcms dashboard design tokens (OKLCH teal/emerald, light + dark) + Geist + `next-themes`; two-column `AuthShell` for auth pages, styled forms from shared `ui.ts` classes, dashboard stat cards + sticky header, centered home page.
- **Public folder:** `apps/web/public/` — favicon.ico, favicon-16/32, apple-touch-icon, icon-192/512, logo.png, og-image.png (1200×630), site.webmanifest — all generated from the desktop app icon with the exact token colors; wired into metadata (icons, manifest, Open Graph, theme color).
- **Formatting fixed:** Prettier 3.10.1 + `prettier-plugin-tailwindcss` adopted (root `.prettierrc`, `pnpm format` / `format:check`); the 12 locale JSON files were single-line dumps and are now pretty-printed (2-space, keys alphabetical). Skills brought from sendcms: `next-best-practices` (verbatim) + a RepoNyx-adapted `reponyx-project-guidelines` skill.
- **Verified:** package tests 25/25 (auth 7, email 10, database 8), web tests 14/14 (5 auth-flow + 9 OAuth, stubbed provider fetch, real PGlite + routes), i18n validate 95 keys 0 failures, all typechecks clean, `next build` clean (22 routes). Local `next start` smoke test: home 200, login 200 (both OAuth buttons render), start routes 307 to the providers, bad callback → login `oauth_failed`, favicon/og/manifest 200.

### Added — Web platform W2 polish: OAuth registration, full name, Terms, clean URLs (2026-11-27, branch `feature/admin-control-plane`, pushed + deployed 2026-11-27 ~04:47 MST, snapshot `64e716f6`, sync 71)

- **OAuth on the register page too (sendcms order):** Google/GitHub buttons sit on top of the form on both login and register, with an "or continue with email" divider below them. The OAuth state now carries the intent (`login` | `register`, HMAC-signed): **login still refuses unregistered users** (`oauth_no_account`), but the **register intent creates the account** with the provider-verified email and the profile name (GitHub falls back to the login when no public name), under the same 1-registration-per-IP-per-24h limit as the email form; an existing email on the register intent is refused with `email_taken`. OAuth-created accounts get an unusable password hash (random token nobody knows) until the owner sets one via the reset flow.
- **Full name required at registration:** new migration `0003_add_user_name` (`users.name`, nullable so the pre-existing owner account keeps working); the register API returns 400 without it, the form has a Full name field first, and the dashboard welcome + `/api/me/limits` return the name (falling back to the email when null).
- **Terms of Service:** `/terms` page (English legal text — service, accounts, packages/limits, acceptable use, data, GitHub access, availability, termination, changes, contact) + a "By signing in/up you agree to our Terms of Service." footer on the login and register pages, localized in en + ur.
- **Clean URLs:** auth pages moved from `/auth/*` to top level — `/login`, `/register`, `/reset`, `/reset/confirm`, `/verify`; the old `/auth/*` routes redirect with the query string preserved (so the `?token=` links already sent by email still work), and the verification/reset emails now link to the new URLs.
- **Verified:** package tests 25/25, web tests 19/19 (5 auth-flow + 14 OAuth, incl. 5 new register-intent tests), i18n validate 99 keys 0 failures, all typechecks clean, `next build` clean (27 routes). Local smoke test: login/register 200 with buttons-before-form order verified in the HTML, `/terms` 200, old URLs 307 to the new ones with the query preserved, register API without a name → 400 `invalid_body`.

### Added — Web platform W2 polish, part 2: linked sign-in methods + name required for OAuth registration (2026-11-27, branch `feature/admin-control-plane`, pushed + deployed 2026-11-27 ~04:47 MST, snapshot `64e716f6`, sync 71)

- **One account, many sign-in methods:** the same verified email is one RepoNyx account no matter how you sign in — email/password, Google, or GitHub. New `linked_providers` table (migration `0004_linked_providers`, `UNIQUE(user_id, provider)` + `UNIQUE(provider, provider_user_id)`) records which providers are linked to each account; a login through a provider links it automatically, and the provider identity (Google `sub` / GitHub `id`) resolves to its owner first, so a login still works even if the provider's email changed. The dashboard profile shows a **Sign-in methods** card (Google / GitHub, Linked + email or Not linked) with **Link** (OAuth flow with a new `link` intent — the provider's verified email must match the account's email, otherwise `email_mismatch`; a provider already linked to another account is refused with `already_linked`) and **Unlink** (`DELETE /api/me/linked/{provider}`) — unlinking is refused with `last_login_method` when it would remove the only way in (no password + a single provider); setting a password via the reset flow unlocks it. `GET /api/me/linked` lists the account's providers; `users.has_password` (migration 0004, default true; OAuth-created accounts get false) records whether the account has a usable password.
- **Name mandatory for OAuth registration:** if the provider does not share a name, the account is NOT created yet — the callback redirects to `/register/complete?token=…` with a 10-minute HMAC-signed pending-registration token proving the provider verified the email; the user enters their full name (mandatory) and `POST /api/auth/register/complete` creates the account (verified, no usable password, provider linked) under the same 1/IP/24h limit and logs them in. A tampered/expired token is refused (`invalid_token`). GitHub still falls back to the login name, so the step triggers mainly for Google accounts without a name.
- **Profile name edit:** the dashboard profile card shows the email and an editable Full name (`PATCH /api/me`), so an account created before the name requirement (incl. the owner's) can complete its profile.
- **Verified:** package tests 26/26 (database 9/9 incl. a linked-providers round-trip test), web tests 28/28 (5 auth-flow + 19 OAuth incl. link intent, auto-link, mismatch, no-session, name-required + complete-profile tests + 4 linked-API tests incl. the last-method guard and already-linked), i18n validate 120 keys 0 failures, all typechecks clean, `next build` clean (32 routes). Local smoke test: `/register/complete` 200 with the invalid-token message, `/api/me/linked` 401 without a session, link start routes 307 with the state carrying intent `link`. **Deployed:** migrations 0003 + 0004 applied to the live Prisma DB before the push (verified live); Vercel `dpl_HYVWVdcJzLnk5nBhpZHbds8BhZc2` READY (production, commit 64e716f6); live smoke test on reponyx.bond green (buttons on top, Full name, Terms footer, /terms 200, /auth/* → 307, OAuth starts 307, /register/complete renders, /api/me/linked 401).

### Added — Web platform W3: desktop login (2026-11-27, branch `feature/admin-control-plane`, pushed + deployed 2026-11-27 ~05:50 MST, snapshot `7a79050`, sync 72)

- **Web side:** pending-login token (HMAC-SHA256 with `SESSION_SECRET`, 10-minute TTL) — the password login now returns `{step:"2fa_required", pendingToken}` instead of a session when the account has 2FA on, and `POST /api/auth/2fa/verify` verifies the token + TOTP code, re-checks the account-switch limit, upserts the device, and only then creates the session. OAuth state carries a `client` flag (`web` | `desktop`; old states without it verify as web); start routes accept `?client=desktop` (forces the login intent — the desktop never registers or links). On desktop-client success the callback 307-redirects to `reponyx://auth?code=<desktopCode>` — a 10-minute HMAC code, never the raw session token — and `POST /api/auth/desktop/exchange` verifies it (account-switch check, device upsert) and returns the session; desktop 2FA → `reponyx://auth?two_factor_required=1&pending=…`, desktop name-required → `reponyx://auth?register_complete=…`, desktop failures → `reponyx://auth?error=…` (+`retry_at` when present). TOTP in `@reponyx/auth` (`otpauth` 9.6.0, verify window ±1 step). TOTP enrollment UI is not in W3 (no admin exists yet — W5); the verify path is fully built and tested by setting `users.totp_secret` directly. No new web DB migration — the pending-login/desktop-code design is stateless.
- **Desktop Rust:** `apps/desktop/src-tauri/src/web_auth.rs` (new) — 9 commands: `web_login_command`, `web_verify_2fa_command`, `web_session_status_command`, `web_logout_command`, `web_open_oauth_command`, `web_open_register_command`, `web_open_dashboard_command`, `web_open_url_command` (paths under the web base URL only), `web_exchange_desktop_code_command`. The session token lives only in the OS credential store (`reponyx:web` / `session`); the frontend never sees it. The last-known account is cached in the settings table (`web.account`) so the offline rule works (already-logged-in session keeps working offline). `ui.locale` is the 8th settings key (`en` | `ur`). Deep link: `tauri-plugin-deep-link` 2.6.3 + `tauri-plugin-single-instance` 2.5.5, single-instance registered FIRST (forwards the second launch's URL to the running app and focuses its window), scheme `reponyx` in `tauri.conf.json`. `CommandError` gained `retry_at` (all 30+ struct literals updated). Windows version for the device record comes from the registry (`reg query`, hidden console, no window flash).
- **Desktop frontend:** `Login.tsx` (rendered outside the app layout — no sidebar): OAuth buttons on top, divider, email/password form, 2FA step view, register-complete card, session-expired notice, localized error alerts (web error codes → `errors.json` via `errorMessageKey`, `{retryAt}` formatted medium-date/short-time like the web). `App.tsx` is the auth gate (`webSessionStatus` on mount) + the deep-link listener (`onOpenUrl` + `getCurrent`, parses `reponyx://auth?...`; deep links are ignored while already signed in). `LocaleProvider` + `useT` (`apps/desktop/src/lib/locale.tsx`, static JSON registration, `dir="rtl"` + `lang` on `<html>` for Urdu). Settings gains the Account section (user, package, linked accounts/devices x/max, joined, expiry, offline note, Manage on the web, Log out) + a Language switcher (English/اردو, applies right away). `packages/shared` web types + zod schemas; api-client `web*` functions (camelCase invoke args, asserted by tests).
- **Verified:** web tests 36/36 (was 28 — 7 new desktop-client OAuth tests + 1 new 2FA password-flow test), auth package 14/14, `cargo test --workspace --exclude reponyx-desktop` 476/476, desktop frontend tests 114/114 (was 94 — 8 api-client web tests, 8 Login tests, 4 Settings tests), `pnpm -r typecheck` clean, `pnpm -r lint` clean, vite build clean, i18n validate 155 keys 0 failures, `cargo fmt` clean, `check-no-private-key.sh` PASSED. **MinGW `cargo check -p reponyx-desktop --target x86_64-pc-windows-gnu` PASSES** — the desktop crate now compiles on this VM (build agent + broker for the GNU target first, copy the two `.exe` files to `target/release/`; recipe in `AGENTS.md`); it caught 4 real errors (return-value mismatches, missing `CommandExt` import, `focus()` → `set_focus()`). MinGW clippy on the desktop shows 11 pre-existing findings (none in W3 code, none gated by CI). Semgrep (exact CI command): 1 pre-existing blocking finding in `packages/database/src/client.ts` (`rejectUnauthorized: false` for `sslmode=require` — the documented Postgres `require` semantic; the live DB uses `sslmode=verify-full`, which verifies) — fixed 2026-11-27 by the user's verify decision (local commit `41425af`, see the Fixed entry above), not yet pushed. Pushed + deployed 2026-11-27 ~05:50 MST (snapshot `7a79050`, sync 72); Vercel `dpl_DKp7eR9ZhNTqDY1dKht3a3nKcVrP` READY (production, aliases incl. reponyx.bond); live smoke test on reponyx.bond green (home/login/register/terms 200, dashboard 307, desktop OAuth starts 307 with the exact callback URLs and the `client=desktop` state flag, both new endpoints answer their own 404 `invalid_token` on bad tokens, `/api/me/linked` 401 without a session); no live migration needed.

### Changed — Accounts page: one page, two tabs (2026-11-27, branch `feature/admin-control-plane`, local only)

- **Connected accounts tab (`/accounts`, default):** the accounts list with a summary line ("3 connected · 1 revoked"), search by username, kind filter (All / GitHub App / PAT), status filter (All / Connected / Revoked). Empty states: no accounts → "No accounts yet — add your first account" with a button to the Add tab; no match → "No accounts match this search" with Clear filters. Account rows are unchanged (Validate, Reconnect, Replace PAT, Remove, Delete with confirmation).
- **Add account tab (`/accounts/add`):** the PAT card and the GitHub App card as two stacked cards. The broker card is extracted into `components/BrokerSetup.tsx` (broker status, setup form, Install + Refresh, installations, pairing — self-contained with its own errors and notices). Tab state comes from the route, so refresh and the back button work. The sidebar keeps a single Accounts entry.
- **Verified:** desktop frontend tests 15 files, 124/124 passed (was 114 — tests split by tab, +10 new: filters, summary, empty states, navigation, PAT add); `pnpm -r typecheck` clean, `pnpm -r lint` clean, vite build clean. Frontend-only change: no backend commands, no DB or settings changes, no migration.

### Changed — CI also runs on the web-platform branch (2026-11-27, branch `feature/admin-control-plane`, uploaded as `8086892`)

- `ci.yml`, `lint.yml`, `security-audit.yml`, `codeql.yml` now trigger on push and pull request to `main` AND `feature/admin-control-plane` (previously main only, so the branch's pushes had zero CI runs). Workflow files cannot be written by the GitHub App, so they were uploaded via the GitHub web UI as commit `8086892` (4 files, 11 insertions, 10 deletions — verified by reading `ci.yml` back at that commit); the local copy is committed as `af518cf`. The upload commit should have triggered the first CI runs on the branch — green status still to be confirmed.

### Changed — Web database driver: Prisma Postgres (2026-11-27, branch `feature/admin-control-plane`)

- **The web's database is now the user's own Prisma Postgres, not Neon.** The Neon HTTP driver (`@neondatabase/serverless`) cannot talk to Prisma Postgres — it POSTs to `https://<host>/sql` and Prisma answers HTTP 404 ("Resource Not Found"), proven by the failed migrate run — so `@reponyx/database` gained a second driver: `pg` 8.23.1 (TCP) for any non-Neon Postgres. The driver is picked from the `DATABASE_URL` host (`*.neon.tech` → Neon HTTP, anything else → `pg`); the Neon path stays, so the Cloudflare Workers portability goal is intact. `DbClient` gained an optional `close()` (the migrate CLI calls it, so a `pg` run terminates instead of hanging on the open pool); SSL follows the URL's `sslmode` (`verify-full` → verified TLS).
- **Migrations applied to the live Prisma database** (`0001_init`, `0002_seed_packages`; verified live: 12 tables, 5 packages seeded, 0 users). `DATABASE_URL` on the Vercel project now points at the Prisma database (the Neon `reponyx-db` is unused and still exists).

### Added — Web platform W1 (2026-11-26, branch `feature/admin-control-plane`)

- **Web foundation (code-complete):** `apps/web` (Next.js 16.4.6) + `packages/i18n` (en + ur, 6 web domains, 85 keys), `packages/auth` (Argon2id via hash-wasm — pure WASM, no native binding, so it also runs on Cloudflare Workers — + random tokens stored as SHA-256 hashes), `packages/email` (one send path; `EMAIL_PROVIDER=resend|smtp` selector, both credential sets can live in `.env`; Resend via fetch, SMTP via nodemailer), `packages/database` (Neon HTTP driver — portable to Vercel + Workers; `0001_init` schema + `0002_seed_packages` Free/Bronze/Silver/Gold/Diamond; `pnpm --filter @reponyx/database migrate`).
- **Auth API:** `POST /api/auth/register|verify|login|logout|reset|reset/confirm` (argon2id passwords, 24h verification tokens, 30-day sessions as hashed tokens in an httpOnly cookie + Bearer token for the desktop app, password reset revokes all sessions, dummy-hash login for unknown emails so no user enumeration by timing), `GET /api/me/limits` (package + usage), `GET/POST /api/devices` (device registration). **Abuse limits (§7) enforced server-side:** registration 1/IP/24h, account-switch 1h device-first/IP-fallback; every rejection is a structured error (code + data, retry-at) localized by the client from `errors.json`.
- **Dashboard shell:** package, linked-accounts/devices usage vs limits, expiry, joining date, devices list, linked accounts list; redirects to `/auth/login` without a session.
- **Verified:** package tests 25/25 (auth 7, email 10, database 8), web auth-flow tests 5/5 against real Postgres (PGlite: full register→verify→login→limits→devices→logout flow, both abuse limits, password reset, duplicate email), i18n validate 0 failures, all typechecks clean, `next build` clean (18 routes). **Not yet:** the live Neon project (`DATABASE_URL`) and the first Vercel deploy; local Postgres could not be installed on this VM (apt lock held by a system process), so migration SQL was verified against PGlite instead.

### Changed — Polish (2026-11-26, Phase 5 of the production audit)

- **Loading states (F9/F13):** the Agent page badge shows
  "Checking…" until the first health poll settles and keeps the
  last known health when a later poll fails (it used to flash
  "Not running"); the Overview page shows "…" while counts load
  instead of zeros, its stale "Phase 1 foundation" / "Phase 8"
  text is gone, and a failed load now has a Retry button that
  clears the error on success.
- **Stale errors clear (F12):** the Synchronization and Activity
  poll loops clear their error banner on the next successful
  poll instead of leaving it up forever.
- **Queue (Q6–Q10):** job transitions now guard on the current
  status in the UPDATE itself, so two workers can no longer both
  claim the same transition; cancelling a pending job is one
  atomic statement; priority aging is uncapped (an old low
  priority job can no longer starve behind a fresh priority-100
  one); every job now has an overall deadline on top of the
  per-operation timeout; and a worker waiting out a rate limit
  sleeps up to 60s instead of spinning claim attempts.
- **Git errors (G7/G8):** failure classification reads only
  `fatal:`/`error:`/`remote:` diagnostic lines (a "403" in a
  branch name no longer revokes an account), and a submodule
  sync that only partly succeeds now fails the job with the
  count instead of silently reporting success.
- **Git locks & health (G9/G10):** lock files are built from the
  canonical path, so two spellings of one repository share one
  lock; health checks a missing HEAD before a stale index.lock
  (a killed clone is diagnosed as incomplete, not locked), and
  the advice now says to re-sync — the partial clone is moved
  aside and re-cloned automatically.
- **Database (DB-m1–m6):** new migration `0005_phase5_indexes`
  (missing indexes, a partial UNIQUE index on `local_path`, and
  a case-insensitive unique index on repository names — GitHub
  names are case-insensitive); a startup repair backfills
  `_sqlx_migrations` records for databases whose migrations were
  applied without them (DB-m2); SQLite connections no longer go
  through a `format!` URL string, so paths with `%`, `#` or `?`
  open correctly (DB-m4); the 7-day failed-history count
  compares real timestamps (DB-m5); and a restore interrupted
  mid-swap recovers the previous database on the next startup
  instead of starting from an empty one (DB-m6). Enum-column
  CHECK constraints are still not enforced at the SQL level —
  SQLite cannot add them without a table rebuild — the typed
  readers fail closed per row instead.
- **Storage (ST-m):** archive paths are parsed anchored at the
  storage root (a repo named "working-clones" no longer
  misparses); the archive scan is recursive with a depth cap and
  never descends into a found archive; repository paths are
  validated against symlinks escaping the root; relocation
  compares canonical paths; and a storage root that cannot be
  created fails with a clear message naming Settings → Storage.
  Relocation locks are documented as accepted: the fetch
  re-acquires the same in-process lock the move held, so holding
  the guards through the fetch would self-block.
- **Agent & IPC (AG-F):** the IPC accept loop backs off
  exponentially instead of spinning on a failing listener; the
  desktop now validates the agent's protocol version in `ping()`
  and `status()`; the scheduler cannot materialize a batch after
  shutdown was requested and an Enqueue arriving after shutdown
  is refused; Windows logoff/shutdown/close signals stop the
  agent gracefully (not just Ctrl+C); startup logs a warnings
  only self-check (git, storage-root writability, credential
  store); and autostart status now means "an entry that launches
  this executable" — a stale entry is repaired on Enable, on
  Windows, Linux, and macOS.
- **Release (E6–E8, F14–F16):** the unused
  `https://api.reponyx.com` CSP origin is gone (E7); CI fails a
  tag whose version has no `## [x.y.z]` CHANGELOG entry (E8);
  CI asserts the ConfirmDialog's Tailwind classes are in the
  built CSS (F14); the release notes list the MSI alongside the
  NSIS installer (F15); every release uploads an SPDX SBOM
  (E6); and the two 120KB Tauri-generated schema files are no
  longer tracked — they are regenerated on every build (F16).

## [0.1.0] — 2026-11-26

### Fixed — Installed-app setup (2026-11-26, Phase 6)

- **Auth Broker setup is now in-app, no `.env` file needed:** the
  installed app used to show "cannot reach the Auth Broker"
  because the broker needs its App id, private key and setup
  token, and those lived only in the repo-root `.env` — every user
  had to hand-create `%APPDATA%\RepoNyx\.env`. The Accounts page
  now has a setup form: the GitHub App id and the private-key file
  (native picker) are saved as settings, the setup token is pasted
  or generated in-app and stored in the OS credential store, and
  the desktop passes all three to the broker it spawns as
  environment variables. The private key itself never passes
  through the desktop — only its path. A repo `.env` (dev) or a
  hand-made `<data_dir>\.env` still works as an override.
  Verified: `cargo test` 475/0, frontend 94/94, Semgrep 0
  findings; the desktop crate is compile-checked by CI
  windows-build only.
- **Schedules: proper cadence controls:** the New schedule form
  had one free-text box for all four cadence types, so "6" sat in
  Daily and Weekly where it means nothing. Every N hours is now a
  dropdown of 1–24 hours; Daily is a time picker; Weekly is
  weekday toggles by name (Monday, Thursday, …) plus a time
  picker; switching type resets the value to a sensible default.
  The backend string is unchanged ("6", "02:30", "1,4 02:30"),
  and a stored hours value above 24 is kept as an option when
  editing. Same controls in the edit form.
- **Storage root no longer shows the `\\?\` prefix:** the Storage
  page showed `\\?\D:\github\zb\articles` because the overview
  returned the canonicalized root, and Windows canonicalization
  adds the verbatim prefix. Display paths now strip it
  (`\\?\D:\...` → `D:\...`, `\\?\UNC\...` → `\\...`) in the
  storage overview, orphan paths and the diagnostics export;
  filesystem operations still use the canonical form.
- **Agent page:** removed the "(see ADR-006)" citation from the
  autostart note — an internal architecture-record reference with
  no meaning to a user. The ADR itself is unchanged.

### Fixed — Silent background work + release test (2026-11-26, Phase 6)

- **No more console windows during syncs or menu changes:** every
  sync and every Overview/Agent page load used to flash a black
  cmd window open and closed, because `git.exe`, `git-lfs`,
  `schtasks`, `reg`, `taskkill` and `tasklist` were spawned
  without `CREATE_NO_WINDOW` — from the GUI desktop and from the
  detached agent, neither of which has a console for its children
  to inherit. Every one of those spawns now hides its console
  (git-engine's git runs + `git --version` / `git-lfs version`
  detection, the agent's `taskkill`, the desktop's
  `schtasks`/`reg`/`taskkill` autostart calls, the IPC `tasklist`
  liveness checks). Syncs, page loads and autostart checks now
  happen silently in the background. Verified: `cargo test`
  475/0, MinGW Windows check passes, Semgrep 0 findings; the
  desktop-crate half is compile-checked by CI windows-build only.
- **Setup token is automatic — no field, no typing:** the Accounts
  page no longer has a "Broker setup token" field or a "Save setup
  token" button anywhere. The token resolves itself: from a broker
  `.env` file when one exists (dev, or a hand-made file), else a
  fresh per-install token generated on first use and kept in the
  OS credential store, passed to the broker the desktop spawns.
  It is deliberately NOT baked into the build: a token in the
  binary would be identical for every user and readable from the
  installer, so anyone could pair a device to anyone's local
  broker. The same reason the private key can never go in the
  build — it is the master key of the GitHub App, and baking it
  would let anyone with the installer mint tokens for every
  installation of the App. The App id + key file remain the one
  per-install step (dev reads them from the repo `.env`).
- **Release test job fixed:** the re-tagged v0.1.0 run failed in
  `reponyx-database --test config` (`export_parse_round_trip`,
  `apply_to_fresh_db`) — the fixture used `/tmp/reponyx-test-root`
  as `storage.root`, which is absolute on Linux but not on
  Windows (Rust's `is_absolute` needs a drive prefix there). The
  fixture now builds its root from `temp_dir()`, absolute on both
  platforms. No production code changed.

### Fixed — CI failures on the Phase 6 push (2026-11-26)

- **Lint + Security audit failed the private-key scan:** the
  guard script banned the literal `REPONYX_BROKER_PRIVATE_KEY`
  anywhere in the desktop, and the new broker-setup code
  legitimately passes `REPONYX_BROKER_PRIVATE_KEY_PATH` — the key
  file's path, never the key itself — to the broker it spawns.
  The script now bans the bare variable only (a regex that
  excludes the `_PATH` suffix, with the reason documented in the
  script), and the one comment naming the bare variable was
  reworded. The guarantee is unchanged: no key material, no PEM
  text, no broker dependency in the desktop.
- **CI Clippy failed on `crates/ipc`:** the new `hidden_command`
  helper was only called from Windows-only code, so on Linux it
  was dead code (and its `mut` unused) — an error under
  `-D warnings`. It is now `#[cfg(windows)]`. Lesson recorded:
  local Clippy must run with `-D warnings`, exactly like CI —
  without it, warnings pass silently and the failure only shows
  up on the runner.
- **CI windows-build failed to compile the desktop crate:**
  `get_broker_status_command` calls `app.state::<...>()`, and
  `AppHandle::state` lives on the `tauri::Manager` trait — but
  `auth.rs` imported only `AppHandle` and `State`, never
  `Manager`. The call arrived with the in-app broker setup and
  was never compiled anywhere except CI windows-build (the
  desktop crate cannot build on the Linux VM). One-line fix:
  `use tauri::{AppHandle, Manager, State};`. Every other file
  calling `.state()` already imported `Manager`.

### Added — Release readiness (2026-11-25, Phase 4 of the production audit)

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
  job (the real key never reaches CI). That proves the packaging
  and updater-artifact generation end to end; it does **not**
  prove the update flow, because the throwaway key does not
  match the public key baked into the app.
- **Updater wired (E3):** Settings → Advanced → Updates now
  checks for updates (`check()` from `@tauri-apps/plugin-updater`),
  shows the release notes, and downloads + installs
  (`download()` then `install()`; installing restarts the app).
  Before this, the plugin was configured and registered but
  nothing ever called it. The minisign public key in
  `tauri.conf.json` was verified to decode as a well-formed
  Ed25519 key (key id `d54eef67f4ed1092`). The end-to-end
  procedure (install v0.1.0 → publish v0.1.1 → update in place)
  is documented in `docs/qa/updater-e2e.md`; it needs a real
  Windows machine and the real signing key, so until it passes
  the updater is "wired, not proven" and the release notes say so.
- **`storage.root` validated against the disk:** the Settings
  page now rejects a root that is relative, missing, not a
  directory, not writable (a probe file is created and removed —
  permissions bits alone lie on Windows), a drive root, or the
  data directory / containing it / inside it. The check runs on
  the canonical path, so a symlink or `..` cannot smuggle a bad
  root past it. Re-saving the same folder under a different
  spelling no longer triggers a pointless archive migration.
  Clearing the field no longer silently saves the data directory
  (that value is now rejected) — a "Reset to default" button
  clears the setting instead, migrating archives back to the
  data directory.
- **Signing secrets (E4) still unverified from code:** whether
  `TAURI_SIGNING_PRIVATE_KEY` (+ password) are set in the repo
  secrets can only be confirmed by the user — see the release
  checklist. The first release build will fail at signing if
  they are missing.

### Added — Security hardening (2026-11-25, Phase 3 of the production audit)

- **Device tokens expire (A1):** a token minted by pairing lives
  90 days (`device_token_ttl`), then minting fails with "device
  token expired; pair this device again". Migration 0003 adds
  `broker_device_tokens.expires_at` and backfills pre-existing
  tokens with 90 days from their creation time; expired tokens are
  pruned at startup. **Re-pairing revokes the device's previous
  tokens:** one live token per device, so a leaked token stops
  minting the moment the device pairs again.
- **Re-install preserves pairing (B1):** `record_installation`
  carries the existing `paired_device` (and the recorded login
  when the callback carries none) forward, so reinstalling the
  GitHub App no longer unpairs the device and breaks minting.
- **Setup-token brute-force lockout (B2):** 10 consecutive failed
  setup-token presentations lock setup-token auth out for 15
  minutes (429 with `Retry-After`); a success resets the counter
  and the lockout is never extended by further guesses.
  Individual failures are not audited, so a guessing loop cannot
  flood the audit table — only the lockout transition is.
  **Non-loopback bind needs opt-in:** `REPONYX_BROKER_BIND` set
  to anything but loopback refuses to start unless
  `REPONYX_BROKER_ALLOW_NON_LOOPBACK=true` is also set.
- **Install callback verified + rate-limited (B3):** the callback
  is rate-limited (10/min per IP, before any audit write) and the
  `installation_id` is verified against GitHub's
  `GET /app/installations` for THIS App before recording — a
  forged callback with an observed `state` can no longer record
  an arbitrary installation, and a GitHub failure fails closed
  (502) instead of recording unverified.
- **Setup-state rate limit (B6):** `POST /setup/state` is
  rate-limited (10/min per IP).
- **Pairing status is POST-only (B5):** the pairing code travels
  in the JSON body, never in a URL query (queries land in access
  logs); `GET /pairing/status` now returns 405.
- **Pipe ACL (F2 / Phase 10 F2):** the agent's Windows named pipe
  is created with a DACL granting access only to the current user
  and SYSTEM (via tokio's
  `create_with_security_attributes_raw`, so the pipe is
  restricted from the instant it exists). Any other local user
  gets Access Denied at connect time. If the descriptor cannot be
  built, pipe creation fails loudly — never silently falls back
  to a world-accessible pipe. A Windows test asserts the DACL
  contains exactly those two trustees.
- **Constant-time secret comparison (A2):** the setup-token check
  and the IPC token check compare SHA-256 digests with
  `subtle::ConstantTimeEq` instead of short-circuiting `==`.
- **Secrets as `SecretString` (A3):** the broker's setup token,
  private key, and cached installation tokens are held in
  `SecretString` (redacted `Debug`, zeroized on drop). The type
  moved to `reponyx-core` so the broker can use it without
  depending on the credential-store crate; the credential store
  re-exports it unchanged.
- **Registered-secret redaction (A7):** `reponyx_logging` can
  register exact secret values (`register_secret`); `redact_secrets`
  replaces every occurrence, so a prefix-less secret (hex setup
  token, base64url device token) that appears bare in a log line
  is redacted even though no pattern could match it. The broker
  registers its setup token and private key at startup.
- **`.env` permissions warning (A4):** on Unix the broker warns at
  startup when its `.env` file is readable by other users
  (`chmod 600`); the README documents the permissions guidance.
- **IPC connection limits + idle timeout (C2/C3):** at most 16
  simultaneous IPC connections (further connections are dropped);
  a connection that sends nothing for 10 minutes is closed. The
  desktop client opens one connection per request, so legitimate
  use is unaffected.

### Fixed — CI windows-build compile errors in the desktop crate (2026-11-25, found by run #83 on 7742d28)

- `cancel_kills_the_whole_process_tree` no longer asserts on a
  single `/proc` sample after the cancel: on a loaded runner the
  orphaned `sleep` could still read as running (`R`) in the
  window between SIGKILL delivery and the kernel reaping it (CI
  run #84 caught exactly that). The test now polls the state for
  up to 5 s until the process is gone or a zombie; SIGKILL is
  uncatchable, so the loop only fails if the group kill genuinely
  missed the process.

- `dashboard_counts` now returns `RepoNyxError` (via the crate's
  `map_error` helper) instead of raw `sqlx::Error`, so the desktop
  `get_dashboard_counts` command's `?` converts through the
  existing `From<RepoNyxError> for CommandError`; `lib.rs` now
  imports `require_db` from `auth.rs`. Both errors blocked the
  windows-build job only — the desktop crate does not compile on
  Linux, so no local check catches them.
- `register_agent_autostart_command` no longer warns about an
  unused `app` on Windows (the parameter is only used on
  non-Windows builds); renamed to `_app`.

### Added — Unified lifecycle: desktop starts/stops agent + broker; installer bundles all three (2026-11-25, Phase 1 of the production audit)

- The desktop now brings its services up with it and tears them
  down on quit — one system, not three processes that happen to
  coexist. At startup it ensures the background agent is running
  (spawning it when needed) and then the Auth Broker; on quit
  (`RunEvent::ExitRequested` / `Exit`) it stops the agent (IPC
  shutdown, then the lock-PID kill fallback) and any broker child
  it spawned. A broker the user started by hand (or via
  `dev:all`) is never touched on quit.
- New `apps/desktop/src-tauri/src/exe.rs`: shared exe resolution
  for the agent and the broker — env override, then Tauri
  `resource_dir()` (installer), then `<exe_dir>/resources/`,
  then the exe dir (portable ZIP), then the workspace
  `target/{debug,release}/` walk-up, then PATH.
- New `apps/desktop/src-tauri/src/broker.rs`: broker health
  probe (raw TCP + HTTP GET `/health`, 1 s timeouts), spawn +
  track the desktop-owned broker child, stop it on quit. In dev
  the broker runs with the repo root as CWD (same `.env` and
  relative `broker-data` as `pnpm broker`); for an installed app
  the data dir is the CWD with `REPONYX_BROKER_DATA_DIR` and
  `REPONYX_BROKER_ENV_FILE` pointed at it.
- The broker now detects an already-running instance before
  binding: a raw TCP + GET `/health` probe (1 s timeouts, needs a
  200 status) makes a second `pnpm broker` exit 0 with
  "already running" instead of crashing `dev:all` on a port
  conflict. It also honours `REPONYX_BROKER_ENV_FILE` (checked
  before the CWD `.env`) so an installed app can point it at
  `<data_dir>/.env`.
- The agent now takes the single-instance lock FIRST — before
  binding the IPC pipe, opening the database, or running
  recovery — so two agents can never both think they own the
  pipe. New `crates/ipc/src/agent_lock.rs`: lock read/write/
  remove, `pid_alive` / `pid_is_agent` (fixed: `pid_alive(u32::MAX)`
  returned true on Linux because `kill -0` wraps to `kill(-1, 0)`).
  The IPC transport also keeps a pending `NamedPipeServer`
  listening instance on Windows so the pipe never has a
  no-listener gap.
- IPC requests now time out after 5 s (`IPC_REQUEST_TIMEOUT` in
  `crates/ipc/src/client.rs`; the live progress subscription is
  exempt). `agent_health` reports a wedged agent (pipe open, no
  reply) as `reachable: false` instead of "connected".
- Start-at-login now registers the DESKTOP on Windows (task /
  Run value `RepoNyx`, legacy `RepoNyxBackgroundAgent` entries
  removed on upgrade) so the whole system comes up at logon.
  Linux/macOS still register the agent unit/LaunchAgent.
- `tauri.conf.json` `bundle.resources` now maps
  `target/release/reponyx-agent.exe` and
  `target/release/reponyx-auth-broker.exe` into the installer —
  this closes the open follow-up from 2026-11-23 (the NSIS
  installer contained only the desktop). `broker:build` and
  `agent:build` are now release builds, and the release workflow
  builds them BEFORE the tauri-action step so the files exist
  when the bundle is made.
- Accounts page safety net (audit F1): the broker-installations
  call is now guarded, so a dead broker can no longer hide the
  accounts — the page shows the accounts plus a broker warning
  instead of an empty list. New test: broker-down still renders
  the accounts.

### Fixed — Crash recovery & data integrity (2026-11-25, Phase 2 of the production audit)

- Partial clone is no longer a dead end (audit B8): a mirror or
  working clone that fails midway is moved aside to
  `<dest>.partial-<unix-secs>` (never deleted) and the clone
  retries cleanly; a valid archive still errors with "already
  exists — delete this folder to re-clone, or run a fetch
  instead". Stale `index.lock` / `packed-refs.lock` / `refs/**`
  locks are cleaned up before clone and fetch, and the working
  clone is verified with `rev-parse` before its path is saved.
- Stranded jobs are reclaimed (audit B9): the queue re-runs
  recovery every 60 s (new `sweep_interval`, 0 disables), so a
  job left `running` by a crash is re-queued within a minute
  instead of sitting forever; a startup recovery failure now
  logs a warning and the agent starts anyway (it used to take
  the whole agent down), and one job's recovery failure no
  longer aborts the whole pass — it is logged, counted in
  `RecoveryReport.errors`, and the pass continues.
- Cancelling a sync now kills the whole git process tree
  (audit B10/G2): git children are spawned in their own process
  group (`process_group(0)`) and the group is killed with
  `SIGKILL` via the `nix` crate (Unix) or `taskkill /T /F`
  (Windows) — `child.kill()` alone left `git-remote-https`,
  `index-pack`, and `git-lfs` orphans holding locks. New test:
  cancelling a clone kills the child and its descendants.
- The database now fails loudly instead of silently empty
  (audit B5/B6/B7, DB-B1): `open_database` writes a `VACUUM
INTO` pre-migration backup (newest 3 kept under
  `<db-parent>/backups/`) and runs `PRAGMA integrity_check`
  before migrating — a corrupt file now errors naming the
  database path and the backup to restore, instead of opening
  an empty database; a missing file logs a loud warning. The
  Accounts command uses `require_db` (a dead database errors
  instead of returning an empty list), one unreadable account
  row is skipped with a warning instead of blanking the whole
  list, and dashboard counts propagate errors to the Overview
  page's error banner instead of rendering as all-zero.
- One data-directory rule (audit DB-M3): `crates/database` now
  owns `data_dir()` / `db_path()` / `log_dir()` and the agent,
  the desktop, and the desktop's agent module all delegate to
  it — the three copies can no longer drift into two
  databases. Both processes log the resolved database path on
  every start.
- Changing `storage.root` moves the archives (audit ST-M1): the
  new `migrate_storage_root` renames each existing mirror /
  working side under the new root under the cross-process repo
  lock, updates `local_path` for moved sides, and reports what
  moved, what had no archive, and what failed (failures keep
  the old path — nothing is deleted). The Settings page shows
  the migration summary in its notice.
- A fetch against a missing archive re-clones instead of
  failing forever (audit ST-M2): the runner enqueues the
  matching clone job(s) and fails with "archive missing;
  re-clone enqueued" — the retry policy lets a later fetch
  succeed once the clone lands.
- Missing git-lfs is now a visible warning (audit G5): when a
  repository has LFS pointers but git-lfs is not installed, the
  sync emits a `job_warning` event and the Synchronization page
  shows it in amber under the job ("large files were not
  downloaded — pointer files only") instead of reporting a
  silent success with placeholder files.
- The broker log test is deterministic by construction
  (`broker_logs_never_contain_a_token` installs the capturing
  subscriber as the process-global default, so every thread's
  log lines land in the buffer, and mints for a unique
  installation id so the assertion names this test's own mint
  line; the earlier thread-local capture and the
  `current_thread` fix both still flaked in full-suite runs);
  the process-group kill uses `nix` safe wrappers instead of
  raw `libc` calls, so Semgrep reports 0 findings.

### Added — Auth Broker runs with the app; release ships all binaries (2026-11-23)

- Root `package.json` now has `broker` (`cargo run -p
reponyx-auth-broker`), `broker:build`, `agent:build`, and `dev:all`
  (concurrently 10.0.7: broker + desktop together). Before this the
  broker had no script at all and had to be started by hand in a
  separate shell.
- The release workflow builds the agent and broker in release mode
  and ships them: the portable ZIP now contains
  `reponyx-desktop.exe` + `reponyx-agent.exe` +
  `reponyx-auth-broker.exe`, and the agent/broker ship as their own
  ZIPs too. Before this a GitHub release contained only the desktop
  — no agent, no broker — so GitHub App auth could not work from a
  release install.
- `.env.example` documents the broker variables; README has a
  "Running the Auth Broker" section.
- Closed 2026-11-25 (Phase 1 of the production audit): the NSIS
  installer now bundles the agent and the broker via
  `bundle.resources`, and `resolve_agent_exe` checks Tauri's
  resource directory first (see the entry above).

### Added — Broker reads `.env`; `dev:all` runs the agent; `build:all` (2026-11-23)

- The Auth Broker now reads a `.env` file from the working
  directory at startup (`dotenvy` 0.15, already in the lockfile
  via sqlx — lockfile diff is one line). Real OS environment
  variables always win over the file, and a missing file is
  not an error. `.env` is gitignored.
- `dev:all` now runs broker + agent + desktop together (new
  `agent` script: `cargo run -p reponyx-background-agent`).
  The desktop checks agent health before spawning one, so a
  dev:all agent is never duplicated.
- New `build:all` script: `broker:build && agent:build &&
tauri:build`, sequential because the three builds share one
  cargo target-dir lock.

### Fixed — Windows first-run bugs (2026-11-23)

Found from the user's screenshots + log of the first real Windows
run (`target\debug\reponyx-desktop.exe`):

- Tauri 2 commands take **camelCase** argument names by default
  (`tauri-macros` `ArgumentCase::Camel`); the api-client sent the
  Rust snake_case parameter names, so every command with a
  multi-word argument failed with `missing required key
<camelCase>` (first seen: `add_pat_account_command` missing
  `accountType`). All invoke keys in `packages/api-client` are now
  camelCase, the input types and the page call sites match, and
  the wrong "snake_case, matching the Rust parameter names"
  comments are corrected. Regression tests:
  `apps/desktop/src/api-client.test.ts` mocks `invoke` and
  asserts the keys.
- The Agent page was the only page without the `commandMessage`
  helper, so a backend error rendered as "[object Object]".
  It now formats `{ code, message }` like the other pages
  (`Agent.test.tsx` covers it).
- `database is unavailable` now points at the app log, where the
  startup error is recorded with its real reason.
- Root cause of the database failure on the user's machine:
  sqlx refused to open the DB because migration 1's checksum
  changed — the only difference between the applied migration
  and the current file is the rename of the comment `-- Gitora
V1` → `-- RepoNyx V1` (commit `49a44bf` vs `main`). The user's
  DB was created by a pre-rename build. Fix on that machine:
  delete `%APPDATA%\RepoNyx\data\reponyx.sqlite*` (the DB is
  empty: no accounts, repos, or schedules yet) and restart.
  Rule added to `AGENTS.md`: never modify an applied migration;
  add a new one.
- The agent binary is not built by a desktop-only build, so
  "Start agent" cannot find `reponyx-agent.exe` next to the
  desktop exe. On a source checkout run `cargo build -p
reponyx-background-agent` first. The release workflow does
  not bundle the agent yet — open packaging gap, see
  `docs/qa/windows-verification.md`.

### Fixed — Phase 10 Windows verification, VM part (2026-11-23)

- Deleted the agent's dead duplicate `windows_task.rs` (zero
  callers; the desktop owns autostart registration) and fixed the
  stale README line claiming Task Scheduler integration was "not
  yet wired" — it is wired end to end (Agent page buttons →
  api-client → Tauri commands → schtasks), since the Phase 6/7
  code landed.
- Added `docs/qa/windows-verification.md`: executable checklist
  for the Windows machine (installer, autostart + logoff/logon,
  Credential Manager, full binary secret scan, real process-kill
  recovery). Secret scan re-run on this VM: PASSED on the source
  half, binary half skipped (no built binary here).

### Fixed — Weekly audit finding RUSTSEC-2026-0594 (2026-11-23)

- Bumped `cookie` 0.18.2 → 0.18.3 in `Cargo.lock` (transitive via
  Tauri/wry): fixes RUSTSEC-2026-0594 ("Private cookie nonces can
  repeat in processes that fork", severity 6.5 medium, dated
  2026-11-23), found by the weekly scheduled-maintenance audit's
  second run. `cargo audit` clean again (0 vulnerabilities, same 7
  known no-fix warnings). No code changed; `cargo test` still 415
  passed / 0 failed.

### Added — Phase 10 F3 key-rotation runbook (2026-11-23)

- `docs/security/key-rotation-runbook.md` closes threat-model
  finding F3: routine rotation procedures for the GitHub App
  private key, broker setup token, per-device tokens, IPC shared
  secret, and fine-grained PATs, a verify-after-rotation
  checklist, and incident response for a suspected broker,
  desktop/agent, or key compromise, with the audit queries that
  prove what happened. Every env var, endpoint, and credential
  store location in it was verified against the source before
  writing.

### Added — Phase 8 completion (2026-11-23)

- **Notifications:** system notifications when a sync job finishes
  or fails, via `tauri-plugin-notification` v2 and a
  `useSyncNotifications` hook mounted in the app layout (30 s
  poll, silent seeding on the first poll, one notification per
  newly finished job naming the repository and job type, error
  message included on failures). A new fifth writable setting,
  `notifications.enabled`, with a toggle on the Settings page
  (on unless explicitly `false`); permission is requested once
  and a denial stops polling. The hook never breaks the app —
  every failure is swallowed and the next poll retries.
- **Frontend tests:** Vitest 5.1.1 + Testing Library 16 + jsdom
  31, run with `pnpm --filter @reponyx/desktop test` and in the
  CI frontend job — 41 tests in 6 files (theme, zod schemas,
  shared UI components, Settings page, notification hook,
  virtualized Repositories list).
- **True virtualization:** the Repositories list now renders only
  the rows on screen (`@tanstack/react-virtual`, dynamic row
  measurement, 640px scroll container); server-side pagination
  ("Show more") is unchanged underneath.
- **Tray menu:** new "Sync due now" item (materializes due
  schedules and reports the result as a notification), a
  separator before Quit, and state-aware Pause/Resume driven by
  a 30 s agent-health poll. Unused `Placeholder.tsx` deleted.
- **Live progress bars (added 2026-11-23 at the user's request):**
  the agent forwards the queue's progress broadcast to IPC
  subscribers as JSON `ServerMessage::Progress` frames, the
  desktop subscribes on one long-lived connection and re-emits
  `sync-progress` Tauri events (10 s retry loop), and the
  Synchronization page renders a determinate bar when git
  reports a percent (new `percent: Option<u8>` on
  `JobProgress`) and an indeterminate bar otherwise.

### Added — Phase 10 durable broker storage (F1, 2026-11-22)

- The Auth Broker now persists installations, pairing codes, setup
  `state` values, device tokens, and the audit log in SQLite
  (`broker.sqlite` under `REPONYX_BROKER_DATA_DIR`, default
  `broker-data`); a restart resumes where the broker left off, so
  devices no longer need to re-pair. Only hashes of codes/tokens
  are stored, never the raw values and never the private key. The
  minted-token cache and rate-limiter buckets remain in-memory;
  audit events older than 90 days are pruned at startup. Proven
  by `apps/auth-broker/tests/store_persistence.rs` (6 tests) and a
  live stop/restart smoke run of the real binary.
- The broker's state maps moved from `std::sync::RwLock` to
  `tokio::sync::RwLock`: a `std` guard held across an await makes
  the handler future non-`Send` (axum refused the routes), so the
  async lock is required now that mutations persist inside the
  critical section.

### Added — Phase 10 security audit, first pass (2026-11-22)

- Database upgrade tests: a V1 database (built from the real
  `0001_init.sql`, seeded with V1-shape rows) upgrades to the
  latest schema with its data intact, and a V3 database upgrades
  to V4 with the new dedupe index working on new rows
  (`crates/database/tests/migrations.rs`).
- Failure-recovery inventory (`docs/qa/failure-recovery.md`):
  every planned failure scenario mapped to the test that proves
  it, with the three honest gaps named (no real process-kill
  test, no mid-transfer network loss, no Windows failure tests).
- README rewritten: it still said "Phase 1" and listed the Auth
  Broker, background agent, and most crates as "not yet created".
  It now describes the app as built, with a first-run user guide,
  data locations, and the security notes users must read
  (BitLocker, hooks in working clones, broker restarts lose
  pairing).

### Security — Phase 10 threat-model review (2026-11-22)

- `docs/security/threat-model.md` was re-checked mitigation by
  mitigation against the code after Phase 9. Three findings are
  recorded there: F1 — the Auth Broker keeps installations,
  pairing codes, device tokens, and its audit log in memory only,
  so a broker restart loses all of them and devices must re-pair
  (durable broker storage is the open fix); F2 — the Windows named
  pipe has no ACL configured, so the per-install IPC token check is
  the only gate (impact bounded by the same-user boundary); F3 —
  no key-rotation / incident runbook exists yet. Two overstatements
  were corrected: IPC protection is the per-install shared secret,
  and the IPC protocol has no delete operation, so destructive
  actions are UI-only.
- Dependency audits run for the first time: `pnpm audit` found no
  known vulnerabilities; `cargo audit` (661 crates) found one
  vulnerability — `url` 2.5.8, RUSTSEC-2026-0592 (stack overflow
  in `Url::origin()` on nested `blob:` URLs) — fixed the same day
  by bumping `url` to 2.5.9. Re-run: 0 vulnerabilities, 7 warnings
  with no available fix (chrono + chrono-tz unmaintained,
  proc-macro-error unmaintained, zbus_names / zvariant /
  zvariant_derive unmaintained, glib 0.18.5 unsound — its fix needs
  glib ≥ 0.20, blocked by Tauri's pinned gtk 0.18 chain).

### Added — Phase 9 performance pass (2026-11-22)

- List queries are now paginated server-side, measured before
  and after at a stress scale of 1,000 repositories / 10,000 jobs
  / 5,000 history rows. `list_repositories_command` takes a page
  (100 rows, offset) and returns `{ items, total }`; the
  Repositories page loads pages of 100 and appends them on "Show
  more" instead of loading the whole table on every search or
  filter change. `list_sync_jobs_command` takes a limit (100 by
  default), so the Synchronization page's 5-second poll no longer
  loads every job row ever created. A new
  `get_repository_names_command` resolves repository names only
  for the rows actually on screen (Synchronization, Activity),
  replacing two full-table loads. Measured medians on this scale:
  repository page ~14 ms (was ~78 ms full), pending jobs ~5 ms
  (was ~56 ms), completed jobs ~10 ms (was ~244 ms). The
  pagination behaviour is covered by tests
  (`crates/database/tests/performance.rs`).

### Added — Phase 9 import/export (2026-11-22)

- The Settings page has an Import & export section. Export writes
  a versioned JSON config file (`reponyx-config`, version 1) with
  the four non-secret settings, each repository's sync selection
  (mode + enabled), and the schedules — never credentials, tokens,
  local paths, or archive state. Both commands open a native file
  dialog (Save for export, Open for import; `tauri-plugin-dialog`
  v2). Import validates the whole file before writing anything, so
  a malformed file changes nothing; entries for a repository or
  account this machine does not have yet are skipped and listed in
  the report. Identity across machines uses provider-stable ids
  (`provider_repository_id`, `provider_account_id`), never local
  UUIDs. The settings value validator is now the single source of
  truth in `reponyx-database` (`config.rs`); the desktop Settings
  validator delegates to it.

### Fixed — CI windows-build on the storage push (2026-11-22)

- CI run 44976745886 (`eab7e23`) failed `cargo check -p
reponyx-desktop` with two E0308 errors in
  `apps/desktop/src-tauri/src/storage.rs`: the `spawn_blocking`
  closures return `CommandResult<...>`, so
  `.await.map_err(...)?` leaves the inner Result —
  `delete_local_archive_command` wrapped it in `Ok(...)` and
  `move_archive_for_rename_command` destructured it as a plain
  tuple. Both now unwrap the inner Result. The other two
  `spawn_blocking` sites were checked and are correct (the
  overview closure returns a plain tuple; delete_orphan returns
  the inner Result as the function's tail). Lesson for desktop
  Rust review: a closure returning `CommandResult<T>` nests two
  Results — one `?` only peels the JoinError.

### Added — Phase 9 diagnostics export + DB backup (2026-11-21)

- The Settings page has a Diagnostics & backup section. Export
  diagnostics writes one redacted JSON file (app version, OS,
  git/LFS versions, database counts, storage sizes, the four
  non-secret settings, a 100-line log tail + the last 50 error
  lines) — every free-text field is redacted, and the finished
  file is scanned again for secret shapes; a failed scan writes
  nothing. Database backups are consistent `VACUUM INTO`
  snapshots (never a file copy of a live WAL database), listed
  with size and date, deletable, and restorable: a restore
  validates the file (SQLite header + `integrity_check` +
  `settings` table), stops the background agent over IPC, and
  stages the file — the swap happens at the next startup, before
  the database opens, with the replaced live database kept as a
  `pre-restore` fallback and stale WAL sidecars dropped. New
  testable pieces: the secret scan + log tail in
  `reponyx-logging`, counts + backup + validation + the settings
  allowlist in `reponyx-database`. Verified: `cargo test` 395
  passed / 0 failed (was 378, +17).

### Added — Phase 9 storage management + orphan detection (2026-11-21)

- The Storage page now shows what is wrong with the storage, not
  just how big it is: orphan archives (directories on disk with no
  repository row), rows whose archive is missing from disk (the
  next sync re-clones them), and rows whose archive directory still
  carries a pre-rename name, with a "Move archive" action that
  renames each side on disk — refusing to overwrite an existing
  destination and rolling back on failure — instead of re-cloning.
  Orphans and stored archives can be deleted from the page (each
  asks first and is re-verified server-side at delete time: the
  path must scan as an archive under the storage root right now
  and match no repository row); deleting a stored archive keeps
  the row and returns its status to Pending, so the next sync
  re-clones it. A low-space warning appears when less than 10% of
  the volume is free. New engine `scan_archive_dirs` +
  `delete_archive_dir` (canonical-path checks, never follows
  symlinks out of the root), a shared DB `set_local_path` helper
  (the runner's local SQL copy is gone), and 3 new Tauri commands.
  Verified: `cargo test` 378 passed / 0 failed (was 373, +5).

### Added — Phase 9 submodule sync + Both-mode completion (2026-11-21)

- Submodules are now synced as a separate step after a clone/fetch:
  the mirror fetches each submodule commit into its object database
  (`git fetch --no-tags <url> <sha>`, verified with `cat-file -e`;
  relative URLs resolved against the mirror's origin the way git
  does), and a working clone gets `git submodule update --init
--recursive` per submodule, so one inaccessible (e.g. private)
  submodule never blocks the others. The runner auto-enqueues a
  `SubmoduleSync` job when either local clone has submodules, and
  fails the job only when nothing at all synced. Both-mode repos
  now actually get both clones: after a `MirrorClone` the runner
  enqueues a `WorkingClone` job, and a `Fetch` enqueues the clone
  job for any missing side. The LFS step now prefers the mirror
  for Mirror/Both repos. Verified with real local fixtures: 4
  engine tests + 3 runner tests + 2 Both-mode tests. `cargo test`
  373 passed / 0 failed.

### Added — Phase 9 LFS sync (2026-11-21)

- LFS objects are now archived and verified as a separate step
  after a mirror sync: `git lfs fetch --all` (new
  `reponyx-git-engine` `lfs` module) followed by verification of
  every object file against its pointer oid (SHA-256). After a
  successful clone/fetch the runner auto-enqueues an `LfsFetch`
  job when the repository has LFS pointers; the job fails (with
  the missing/corrupt oids named) instead of reporting an archive
  that is not complete, and a corrupt object is re-downloaded.
  Verified with real git-lfs 3.8.1 fixtures: 3 engine tests + 2
  runner end-to-end tests. `cargo test` 352 passed / 0 failed.

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

### Added — Phase 8 Settings page (2026-11-21)

- Settings page (`/settings`): General (theme — System/Light/
  Dark, applied at startup via the class-based dark variant),
  Sync (default sync mode for newly discovered repositories,
  concurrent syncs 1–10), Storage (archive root, with a warning
  that existing archives stay put and the change applies after
  the agent restarts), and read-only Advanced info (app version,
  data directory, paired device identifier). Two new Tauri
  commands in `apps/desktop/src-tauri/src/settings.rs`
  (`get_settings_command`, `set_setting_command` — only four
  keys writable, each value validated before it is stored).
  Two settings that nothing consumed are now real: discovery
  reads `sync.default_mode` for new rows (existing rows keep
  theirs; new test `discovery_new_rows_use_default_sync_mode_
setting`), and the frontend applies `ui.theme`.

### Fixed — Paired device identifier never persisted (2026-11-21)

- Pairing stored the per-machine id only under the
  `device_identifier` key, but the agent's credential resolver
  reads `device.identifier` — a key nothing ever wrote. GitHub
  App accounts therefore failed every job in the agent with
  "requires a paired device identifier" even after pairing
  succeeded. `complete_pairing_command` now also persists
  `device.identifier`. Desktop-only change, verified by `cargo
fmt` + review (gdk blocker).

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
