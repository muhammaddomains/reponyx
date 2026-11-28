# W4 — broker ↔ web: plan

Status: IMPLEMENTED 2026-11-28 (plan approved 05:02:37 MST; web + broker + desktop sides built and verified — cargo 479/479, web 48/48, frontend 138/138, Semgrep 0, MinGW check passes; push needs approval, sync 81)
Branch: `feature/admin-control-plane`
Plan source: `ai/phase6/web-platform-plan.md` §5–§6, §12 (W4 line).

## Goal

Two things the user asked for, both blocked on the broker↔web link:

1. **Web Linked accounts 0/2 → 2/2.** The web's `/api/me/limits` already counts
   `linked_github_accounts` — but nothing ever writes rows there. W4 writes
   them: the desktop reports its linked installations (and PAT logins) to the
   web, the web verifies each installation against the shared GitHub App and
   records the installation → user link.
2. **The App key leaves the user's machine.** Per the approved platform plan
   (§5), the local broker never holds the private key. The web mints
   installation tokens with the key (already in the Vercel env); the local
   broker asks the web to mint, using the user's web session as the
   credential. The desktop ↔ broker API does not change.

## What already exists (surveyed 2026-11-28, no re-derivation needed)

- Web DB (migration `0001_init`, live on Prisma): `linked_github_accounts`
  (`user_id, provider, provider_account_id, login, auth_kind 'app'|'pat'`,
  unique per user+provider+account) and `installations`
  (`user_id, github_installation_id bigint UNIQUE, account_login`).
  Queries exist: `countLinkedAccountsForUser`,
  `listInstallationsForUser`, `listLinkedAccountsForUser`. No insert/upsert
  queries yet. **No new migration needed for W4.**
- `/api/me/limits` already returns the linked count — once rows exist, the
  dashboard and the desktop Settings count move with no further change.
- Broker mint path today: `POST /installations/{id}/token` (device-token
  auth + paired-device gate + per-device rate limit) →
  `state.mint_token` (in-memory cache, 5-min margin) →
  `tokens::mint_installation_token` (signs the App JWT locally with
  `REPONYX_BROKER_PRIVATE_KEY[_PATH]`). The agent calls this endpoint; it
  does not change.
- Desktop session token lives in the OS store (`reponyx:web` / `session`,
  `web_auth.rs` via `reponyx-credential-store`). The broker has no
  credential-store dependency today.
- Broker config: `app_id`, `app_slug`, `private_key_pem` (SecretString),
  `github_api_base`, `setup_token`, … Desktop spawns the broker with
  `REPONYX_BROKER_APP_ID` / `REPONYX_BROKER_PRIVATE_KEY_PATH` /
  `REPONYX_BROKER_SETUP_TOKEN` (`broker.rs`).
- Web has no GitHub App client and no JWT library (`jose` /
  `jsonwebtoken` are in neither `apps/web/package.json` nor
  `packages/auth/package.json`).
- Devices: only `GET/POST /api/devices` exists. The `revokeDevice` query
  exists but no route uses it — there is no device-revoke endpoint yet.

## Design

### 1. Web mints tokens — `POST /api/broker/mint`

- New `apps/web/src/lib/github-app.ts`: App JWT signing (RS256, 9-min)
  with `jose` (latest stable; works on Node **and** Cloudflare Workers,
  unlike `jsonwebtoken`), `listAppInstallations()` (`GET
  /app/installations` → `[{id, account:{id, login}}]`),
  `mintInstallationToken(id)` (`POST /app/installations/{id}/access_tokens`).
  Reads `REPONYX_GITHUB_APP_ID` + `REPONYX_GITHUB_APP_PRIVATE_KEY` from
  env (already set on Vercel by the user). Missing env → 503
  `not_configured`, fail-closed.
- New route `POST /api/broker/mint`, session auth (see §2). Body
  `{installationId, deviceIdentifier}`. Checks, in order:
  1. session valid;
  2. installation linked to this user (`installations` row) — else 404
     `installation_not_linked`;
  3. device not revoked (the `devices` row for this user + device
     identifier has `revoked_at IS NULL`) — else 403 `device_revoked`;
  4. package limit — the installation's linked row must be within the
     first `max_linked_accounts` by `created_at` (oldest-first keeps
     working; this is the expiry → back-to-Free pause: extra accounts
     stop minting, nothing is deleted, the user is never locked out
     entirely) — else 403 `limit_exceeded`.
  Then mints with the App key and returns `{token, expiresAt}`.
  Audit row `broker.mint`. The token is never logged or stored.
- The web never sees repo contents, PATs, or GitHub tokens — it mints a
  token and returns it; the local broker caches it as today.

### 2. Session auth for the broker — Bearer support

- `requireSession` (`apps/web/src/lib/auth.ts`) today reads the
  `reponyx-session` cookie only. W4 accepts `Authorization: Bearer
  <token>` as well (same token, same verification, same expiry). The
  broker sends the session token as Bearer; the desktop's existing web
  calls keep using whatever they use today.

### 3. Broker reads the session from the OS store (no new endpoint)

- The broker gains a `reponyx-credential-store` dependency and reads the
  desktop's session token with `default_store().get(SERVICE_WEB,
  ACCOUNT_WEB_SESSION)` whenever it needs to call the web (mint, install
  verification). No token is passed in argv, env, or URLs; no token is
  duplicated into broker storage.
- Why this way: the broker runs as the same Windows user, so the OS
  store is readable; logout deletes the entry, so revoking the session
  revokes the broker's web access with no extra code; a broker restart
  loses nothing.
- Test seam: `SharedState` holds the `Arc<dyn CredentialStore>` used for
  the session read (production: `default_store()`; tests:
  `memory_store()` pre-loaded). No env-var session token in production.
- Linux dev (no secret service): a store failure reads as "no web
  session" with a clear error, never a crash.

### 4. Broker proxies minting to the web — local key deleted

- `state.mint_token` keeps its cache and signature; only the miss path
  changes: instead of `tokens::mint_installation_token` (local JWT), it
  POSTs to `{web_base}/api/broker/mint` with the session token and the
  device identifier, and caches the returned token exactly as today.
  `force_refresh` bypasses the cache and re-mints from the web.
- `tokens.rs`: `app_jwt`, `mint_installation_token`,
  `list_app_installations`, `verify_app_installation`,
  `fetch_app_installations` are deleted. `jsonwebtoken` leaves the broker
  dependencies (prune from the workspace only if nothing else uses it).
- `BrokerConfig`: `private_key_pem` removed; `app_id` removed (only ever
  used as the JWT `iss`); new `web_base_url` (env `REPONYX_WEB_URL`,
  default `https://reponyx.bond`). `app_slug` stays — it builds the public
  install URL (`https://github.com/apps/{slug}`).
- Web unreachable / session expired at mint time → the broker returns a
  clear error (`NetworkUnavailable` / a new `web_session_expired` code
  the desktop maps to "log in again on the web"). Cached tokens keep
  serving until expiry, so a brief outage does not instantly break sync.
- **Offline behaviour change (by design, plan §5):** GitHub App sync
  needs the web reachable once the cached token expires. PAT sync is
  untouched and still works offline.

### 5. Install callback verifies through the web, not the local key

- Today the callback verifies the installation with the local key
  (`verify_app_installation`, fail-closed, audit B3) and fills the login
  from `GET /app/installations`. After W4 it asks the web instead: the
  broker calls the web (session from the OS store) to verify the
  installation id and fetch its account login, then records it.
- If the web is unreachable or there is no session, the callback fails
  closed exactly as today (nothing is recorded). The link step (§6) is a
  second, hard gate — a bogus local record can never mint.
- `GET /installations` keeps its sync-80 shape (lists recorded rows,
  records nothing); the GitHub login fill-in call is gone — the recorded
  login (from the verified callback) is what shows.

### 6. Linking — desktop reports, web verifies and records

- New web route `POST /api/me/github-accounts` (session auth):
  - `{authKind: "app", installationId}` — the web lists the App's
    installations with its key, finds the id, and checks it is not
    already linked to another user. Then, under the package limit
    (count < max, else 403 `limit_exceeded` with retry data), it upserts
    the `installations` row (by `github_installation_id`) and the
    `linked_github_accounts` row (`auth_kind='app'`,
    `provider_account_id` = the GitHub **account id** — stable across
    reinstalls, unlike the installation id — `login` = account login).
    Returns the linked account.
  - `{authKind: "pat", login}` — no token is ever sent. The web upserts
    the `linked_github_accounts` row (`auth_kind='pat'`,
    `provider_account_id` = login) under the same limit check.
  - Idempotent: linking an already-linked account returns it (no
    duplicate, no double count).
- New web route `DELETE /api/me/github-accounts/[id]` (session auth):
  deletes the linked row (and the `installations` row for `app` kind).
- New desktop calls (new `web_*` commands in `web_auth.rs` + api-client
  functions, same session mechanism as the existing web commands):
  - after `complete_pairing` succeeds → link the installation
    (`authKind: "app"`).
  - on the Accounts page load → reconcile: link every paired broker
    installation not yet linked. This links the user's 6 existing
    installations with no re-install and no re-pairing.
  - when a PAT account is added → link (`authKind: "pat"`, login only);
    when an account is removed/deleted → unlink.
- `/api/me/limits` needs no change. The dashboard "Linked accounts" and
  the desktop Settings count move as soon as the first link lands.

### 7. Device-revocation mirroring

- New web route `DELETE /api/devices/[id]` (session auth; owner only)
  using the existing `revokeDevice` query, plus a Revoke button on the
  dashboard Devices list. Revoking sets `revoked_at`.
- The mint check (§1, step 3) refuses revoked devices, so revocation
  bites on the broker side too — no separate broker call needed, and no
  way for the web to reach localhost.
- The desktop's existing session-status check already surfaces a dead
  session; no change there.

### 8. Broker setup becomes zero-config (user-visible)

- The broker no longer needs the App id or the private-key path, so the
  desktop's broker setup form (App id + key-path inputs in
  `BrokerSetup.tsx`) is removed. The broker needs only the setup token
  (auto-resolved as today) and the web URL (default
  `https://reponyx.bond`, overridable via `REPONYX_WEB_URL` for local
  dev). The desktop passes the resolved web base URL to the broker at
  spawn (same env override the desktop's own web calls use).
- The `broker.app_id` / `broker.private_key_path` settings keys stay
  stored (ignored) — no settings migration. The user's `.pem` file stays
  on disk, unused; nothing is deleted. The in-app "broker needs setup"
  state effectively disappears (the broker starts without configuration).
- The 5 `REPONYX_BROKER_*` repo secrets remain useless (no workflow reads
  them) — still to be deleted by the user, unchanged.

## What W4 does NOT do

- No admin dashboard (W5) — the owner's existing installations are
  linked by the desktop reconciliation (§6), not "claimed from the
  admin". The plan's admin-claim path is superseded by this; the admin
  still sees every user in W5.
- No billing (W6, separate approval). No TOTP enrollment UI (W5).
- No new migration. No new Vercel env vars (App id + key already set).
- No change to the sync engine, agent, IPC, PAT path, device-token
  model (90 days), or the desktop ↔ broker API shape.

## Files (expected)

- Web: `apps/web/src/lib/github-app.ts` (new), `apps/web/src/app/api/broker/mint/route.ts` (new), `apps/web/src/app/api/me/github-accounts/route.ts` (new), `apps/web/src/app/api/me/github-accounts/[id]/route.ts` (new), `apps/web/src/app/api/devices/[id]/route.ts` (new), `apps/web/src/lib/auth.ts` (Bearer), `packages/database/src/queries.ts` (link/unlink/device-by-identifier queries), `packages/database/src/types.ts` (if a row shape changes), web tests, i18n `errors.json` (+ dashboard keys) en+ur × web+desktop.
- Broker: `apps/auth-broker/src/tokens.rs` (delete local signing; add web-mint client), `apps/auth-broker/src/state.rs` (mint via web, session store field), `apps/auth-broker/src/config.rs` (drop key/app-id, add web_base_url), `apps/auth-broker/src/routes.rs` (callback verify via web, mint route unchanged), `apps/auth-broker/Cargo.toml` (+ `reponyx-credential-store`, − `jsonwebtoken` if unused), broker tests (wiremock web mock).
- Desktop: `apps/desktop/src-tauri/src/broker.rs` (spawn env: drop app-id/key-path, add web URL), `apps/desktop/src-tauri/src/web_auth.rs` (link/unlink/reconcile commands), `apps/desktop/src/components/BrokerSetup.tsx` (setup form removed), `apps/desktop/src/routes/Accounts.tsx` + `AddAccount.tsx` (call link/unlink at the right moments), `packages/api-client` (new functions), frontend tests.
- Docs: `CHANGELOG.md`, `ai/memory/PROJECT_STATUS.md`.

## Verification

- Standard gates: `cargo fmt`, clippy `-D warnings` (excl. desktop),
  `cargo test --workspace --exclude reponyx-desktop`, `pnpm -r
  typecheck`, frontend tests, `pnpm -r lint`, vite build, local Semgrep
  (exact CI command), `bash scripts/check-no-private-key.sh` (broker
  code changes — must pass: no key material anywhere), MinGW desktop
  check (broker.rs / web_auth.rs touch the desktop crate).
- Web tests: mint/link/unlink/revoke against PGlite with the GitHub App
  API stubbed (same fetch-stub pattern as the OAuth tests); limit and
  revocation cases; Bearer auth accepted, cookie still accepted.
- Broker tests: wiremock stands in for the web mint/verify endpoints;
  session read via `memory_store()`; cache behaviour unchanged.
- Live check after push + deploy: link the 6 existing installations from
  the desktop (Accounts page), watch the web dashboard go 0/6 → 6/6
  (Free limit is 6 since 2026-11-28), then a Sync now to prove
  web-minted tokens flow end to end.

## Open questions for the user (decisions in this plan)

1. **Free limit is 6 now (DECIDED 2026-11-28, APPLIED same day).** The user set the Free
   package's `max_linked_accounts` to 6 (direct DB update, verified BEFORE 2/2 → AFTER 6/2 — the admin UI
   to edit packages is W5). All 6 of the user's current installations can
   link; oldest-first still applies above the limit. **Free is
   admin-editable** like every other package — no package is hardcoded,
   including Free; the admin can change Free's limits in W5.
   (`max_devices` stays 2.)
2. **Broker setup form removal** (§8) — the App id + key-path inputs
   disappear. Existing values are ignored, not deleted.
3. **Offline App sync** (§4) — after the cached token expires, App sync
   needs reponyx.bond reachable. PAT sync still works offline.

## Rollout

1. Build + verify locally, commit locally, report with real numbers.
2. Push only on the user's approval (snapshot sync 81).
3. Deploy + CI check via the usual one-time cron.
4. Live test on the user's machine: Accounts page links the installations,
   dashboard count moves, Sync now works on a web-minted token.
