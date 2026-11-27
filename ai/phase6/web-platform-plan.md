# RepoNyx web platform — full plan (2026-11-26, Phase 6)

**Status:** APPROVED 2026-11-26 ~07:34 MST — implementation in progress — **W1 code-complete 2026-11-26** (auth + DB + email + dashboard shell); **database moved to the user's Prisma Postgres 2026-11-27** (migrations applied live; first Vercel deploy still open). **Branch:** `feature/admin-control-plane` (2nd branch, not main). **Layout:** the RepoNyx monorepo — web app under `apps/`, shared code under `packages/`, following the sendcms pattern (`pnpm-workspace.yaml`, `turbo.json`).

## 0. Summary

RepoNyx becomes a service with three pieces: a **web** (control plane — accounts, login, packages, limits, admin), a **desktop app** (the experience — login first, then sync as today), and a **local broker** (runs on the user's machine with the agent and the app, in the single installer). One shared GitHub App (5896712) serves every user; the App key lives only in the web's environment and never leaves the server — the local broker asks the web for installation tokens. Login is mandatory for every app user. Packages are admin-managed, never hardcoded. Languages: English + Urdu, proper RTL, on the web and in the app. Billing (Stripe) is a separate later approval.

## 1. The idea (agreed shape)

- A **web** with auth: Google / GitHub / email, email verification, 2FA.
- After installing the app, the **main page is a login** that uses the web's API. Email → form right in the app; Google/GitHub → URL opens in the system browser and the account gets linked.
- Registration happens **on the web only**. The app has a Register button that opens the web's register page. Login (including Google/GitHub) works **only for already-registered users** — OAuth never auto-creates an account. Email verification is required before login works.
- On the web, **every user's info**: devices, linked accounts, usage, package + expiry.
- The **private key lives in the web's environment**; the web mints installation tokens with it. The key never leaves the server, and no broker ever receives it.
- On the web, **packages + limits**: when the account expires, how many accounts can be linked, etc.
- The user becomes the **operator of a service** (other people's emails + password hashes — privacy policy + terms, encrypted DB backups, same "no secrets in the repo" rule).

## 2. Architecture

- **Desktop** ↔ **Web API**: login, session, devices, limits, packages. The app is the experience; the web is the account.
- **Broker** ↔ **Web API**: the local broker asks the web for installation tokens; the web mints them with the key. The broker never holds the key.
- **Desktop** ↔ **Broker**: pairing + token minting — unchanged.
- **Broker** ↔ **GitHub**: installations + tokens — unchanged.
- The web **never sees** repo contents, PATs, or GitHub tokens.
- The broker, the agent and the app all run on the user's own machine; the installer stays **one file** (NSIS / portable ZIP, as the release already ships).
- This plan **supersedes** `ai/phase6/central-broker-plan.md` (no broker on any server).

## 3. The web (Next.js on Vercel + Postgres)

- **Stack:** Next.js (React/TS — the same stack as the desktop), **Vercel** (already connected), Postgres (**Neon or Prisma Postgres — the database driver is auto-detected from the URL host**; Neon HTTP driver keeps the Workers path, `pg` serves standard Postgres). **Portable hosting:** Vercel is the primary host, but the web is built so it can also run on **Cloudflare Workers** (no Vercel-only features; DB is Postgres; email goes through HTTP APIs).
- **Email (verification, reset, notifications):** pluggable provider — **Resend** (free tier: 100 emails/day) or the user's own **SMTP**. Selected by an env var (`EMAIL_PROVIDER=resend|smtp`); both sets of credentials can live in `.env` at once — the variable picks which one sends. The admin can also switch it in the web admin; the web never hardcodes one provider.
- **Not sendcms for email:** sendcms is a contact-form backend, not an email API — verified 2026-11-26 from its demo + docs: its only public endpoint accepts form submissions (`POST /api/public/v1/submit`, Origin-header auth, no API key) and notifies the form owner; it cannot send verification emails to arbitrary users. **Sendcms IS the right tool for the RepoNyx website's forms** — contact-us, subscribe, etc.
- **Auth:** email + password (argon2id hashing, verification link/code, sessions as random tokens hashed server-side) + Google + GitHub OAuth. The **web is the OAuth client** — its client secrets live in the web's env (Vercel env vars, never in the repo); the desktop holds no OAuth secrets.
- **Registration rules:** one registration per IP per 24 hours (see §7); email verification required before login works; OAuth login never auto-creates an account.
- **2FA:** TOTP (authenticator app) + backup codes. Opt-in for users; **mandatory for admin accounts**.
- **Pages:** login / signup, verify email, dashboard (my devices, linked accounts, usage, package + expiry), admin (all users, devices, packages).
- **Data (web database):** users, sessions, devices, linked GitHub accounts (**logins only, no tokens**), installations → user links, packages, subscriptions, audit log. Never tokens, never PATs, never the App key (the key lives in the web's env, not the database).
- **API (web):** `POST /auth/register|login|logout|verify|2fa`, `GET/POST /devices`, `GET /me/limits`, `POST /broker/mint` (installation tokens, ownership- + limit-checked), `GET/POST /admin/*` (users, devices, packages). Exact routes are set at implementation time; the contract is: every response carries a structured error (code + data) for the client to localize (see §7).

## 4. Desktop — login first, then the app as today

- The app opens to **Login** (no Overview until authenticated). **Login is mandatory for every app user.**
- **Email** → in-app form → session token → OS credential store (reuse `reponyx-credential-store`, where PATs already live). **Google/GitHub** → the system browser opens the web's OAuth URL → authorize → redirect to `reponyx://auth?code=...` → the desktop picks it up via the **deep-link plugin** (new: `@tauri-apps/plugin-deep-link` + protocol registration in `tauri.conf.json` — no deep-link setup exists today) → the desktop exchanges the code for a session token.
- **Register button** on the login screen opens the web's register page in the system browser. Login works only after registration + email verification.
- **2FA step in login:** if the account has 2FA on, the login flow asks for the authenticator code after the password — on the web and in the app (the app shows the same 2FA field; the web API returns a `2fa_required` step, never the session, until the code verifies).
- On login the desktop **registers its device** (the existing device identifier + machine name, Windows version, app version, last seen) and refreshes last-seen on each start. The web also records the device's last-seen IP from the request.
- Session expiry + logout + re-login. **Offline:** an already-logged-in session keeps working locally (repos are on disk), but broker features need the web + broker reachable.
- **App lock (device-local, separate from web login):** the app can lock itself with a **PIN** and/or **Windows Hello** (fingerprint / PIN / face via the device's own consent check). The lock engages on app start and after idle; the PIN is stored hashed in the OS credential store. This protects the app on a shared machine — it is not a second web login.
- **The app shows the account, the web changes it.** The app displays (read-only): the user, this device, the package, limits used / remaining (linked accounts, devices), joining date, expiry date. Any change — upgrade, buy, change package — happens on the web: a button in the app ("Manage on the web" / "Upgrade") opens the web dashboard in the browser.

## 5. Broker ↔ web — the key never leaves the server

- **Today:** the key comes from `REPONYX_BROKER_PRIVATE_KEY[_PATH]` at startup and the broker signs JWTs locally. **New:** the broker **never receives the key**. When the desktop asks the local broker to mint a token, the broker calls the web's mint endpoint (over TLS, with the user's session token, which the desktop already holds from login). The web checks that the installation belongs to that user and that the user is within their package limits, then mints the token with the key and returns it. The broker caches it exactly as today.
- **Why not "the broker fetches the key"?** If every user's local broker fetched the same master key, every user would hold the master key — the same exposure as baking it into the build, one step removed. The key's *power* comes from the server; the key itself stays on the server. The desktop ↔ broker API does not change (same mint endpoint, same shape) — only the broker's mint step changes, from "sign locally" to "ask the web".
- **One shared App for everyone:** every user installs the same RepoNyx GitHub App (App ID 5896712) on their own GitHub account. No per-user Apps — the web links each installation to the RepoNyx user who installed it, and mints tokens only for that user's own installations.
- The web mints **only for installations the requesting user owns** (the install callback links each installation to the RepoNyx user who installed it). Rotation = new key in the web env; no broker ever needs to re-fetch a key, because no broker holds one.
- **Owner's existing installations:** the owner's 3 installations (185410556 DashingMZA, 185459362 zuhandeveloper, 185468482 muhammaddomains) predate the web — the web lists them via the App key and the owner claims/links them to their RepoNyx account from the admin (no re-install needed).
- **Clone auth is not login auth.** The web's login (email / OAuth) and the repo-clone auth (GitHub installation tokens, PATs) are two separate things. Clone credentials never live in the web's database: the web mints a token and returns it; the local broker stores it in its own database (`broker-data`, as today) and the agent uses it from there. The web's database holds only the installation → user link, devices, packages and limits — never tokens, never PATs.

## 6. Devices, limits, packages — the web as source of truth

- **Devices:** register on login, last-seen heartbeat, revoke from the dashboard (the web tells the broker to revoke the device token too, so revocation bites in both places).
- **Limits enforced at pairing / link time** (not in the sync hot path): when a device requests pairing or links a GitHub account, the broker asks the web "is this user within limits?" — or the desktop asks the web first and passes a short-lived entitlement. A package defines: max devices, max linked GitHub accounts, expiry date, (later) max repos.
- **Packages are never hardcoded.** Every package — including Free — is created, edited and updated from the web admin, with no code change and no app release. The starting set is: Free = 2 linked accounts — any mix (2 Auth, 2 PAT, or 1 Auth + 1 PAT) — and 2 devices; Bronze = 10, Silver = 20, Gold = 50, Diamond = 100 linked accounts (device limits for paid packages are set by the admin when creating the package). The admin can change any of these numbers, add new packages, or remove one.
- **One RepoNyx user links many GitHub accounts** — the owner's current 3 (DashingMZA, muhammaddomains, zuhandeveloper) are the model: user 1 with 2–3 accounts and user 2 with 5 are the same shape, only the package limit differs. A linked account counts toward the limit whether it came in via Auth or via PAT.
- **Isolation is absolute:** a user can never see another user's accounts in the app. PATs live only in the local OS credential store on that user's own machine — they are never sent to the web, never synced, never stored in any database. Auth tokens are minted by the web only for installations linked to the requesting user (ownership-checked on every mint). So User 1's app only ever shows User 1's accounts — no cross-user PAT or Auth, in any way.
- **Expiry → back to Free.** When a paid package expires, the limits revert to Free (2 linked accounts). Accounts beyond the Free limit pause syncing — no new syncs, local repos untouched, nothing deleted — until the user upgrades or unlinks down to the limit. The account keeps working on the Free limits; it is never locked out entirely.
- **Web user info (per user):** machine name, Windows version, last-seen IP, PAT accounts used, Auth accounts used, remaining slots, joining date, expiry date — on the user's own dashboard, and on the admin's user list.
- **The admin sees every user.** The admin dashboard lists all users with the info above (total users, devices per user, active users) and can open any user's detail.

## 7. Abuse limits + error contract

- **Registration limit: 1 per IP per 24 hours.** A successful registration from an IP blocks another registration from the same IP for 24 hours. The error shows the exact retry time.
- **Account-switch limit: 1 hour, device-first.** After a logout, the same account can log back in immediately; a different account from the same device cannot log in for 1 hour (the clock starts at logout). Keyed on the device identifier first, IP as the fallback. Enforced server-side on the same login API the web and the app both use, so the rule is identical in both places.
- **Error contract:** every rejection returns a structured error (code + data, e.g. retry-at time) and both the web and the app render the localized message from their own `errors.json` — never a generic "login failed". The message always names the retry time.
- **Honest limit:** IP-based limits also catch shared IPs (office, university, CGNAT) — a legit user behind a shared IP can be blocked. The device-first keying keeps that rare, and the admin can override / unblock from the admin dashboard.

## 8. i18n — web + app, professional structure

- **One shared package, language first.** `packages/i18n/` in the RepoNyx monorepo — the same shape as the user's sendcms `packages/i18n` (verified on `feature/admin-control-plane`): `locales/{en,ur}/{web,desktop}/{domain}.json` — language first, then product, one file per feature/domain, never one big file per locale. Web domains: `common`, `auth`, `dashboard`, `admin`, `packages`, `errors`; desktop domains: `common`, `accounts`, `repositories`, `sync`, `schedules`, `settings`, `errors`.
- **Framework-free translator** (like sendcms's: `Intl` + JSON only, `{name}` interpolation, `Intl.PluralRules`, locale-aware date/number/currency helpers). The web and the desktop each use a thin adapter over the same package (a React hook in each app); a missing key renders `[missing: key]` — visible, never a silent empty string. A validation script (`pnpm --filter @reponyx/i18n validate`) checks en/ur key parity and fails CI on a mismatch.
- **Languages now: en + ur.** `ur` is Urdu script (اردو) — not Roman Urdu. Adding a language later = a new folder `locales/<code>/` with the same file set + one line in the locale config. No code changes elsewhere.
- **en is the source of truth.** Every key exists in `en` first; `ur` mirrors the same keys. A CI check fails the build on missing keys in any locale.
- **RTL done properly.** `dir="rtl"` on `<html>` for `ur`, `dir="ltr"` for `en`. Layout uses logical CSS (Tailwind `ms-/me-/ps-/pe-`, `text-start/end`, `rtl:` variants) — no hardcoded `left/right` in layout code. Menus, drawers and sidebars flip side; direction arrows flip (`rtl:scale-x-[-1]`). Numbers stay Latin digits; dates use locale-aware formatting (`Intl.DateTimeFormat`).
- **Server errors are localized by the client.** The API returns an error code + data (e.g. `retry_at`), never a user-facing sentence; the web and the app render the message from their own `errors.json`. That is why the same limit error reads correctly in both languages, in both places.
- **Language switcher in both places.** The **desktop app** gets full i18n with a language switcher in Settings — every screen, menu and dialog switches language, including RTL. The web has its switcher in the account menu. Persisted per user; first run auto-detects from the OS / browser locale, default `en`.

## 9. Billing — providers decided, timing separate

- **Providers:** Stripe, PayPal, Polar, Freemius, Gumroad. The web maps each package to the provider's subscription/product (except Gumroad — see below); each provider's webhook drives the subscription status.
- **Admin controls which providers are active.** Each provider has an on/off toggle in the web admin. A provider can be enabled **only when its env values exist** — without them it stays disabled and never appears at checkout. Keys are **never shown in the admin UI** — only whether they are set. (Same rule as sendcms's `.env.example`, Phase 8.)
- **Env contract (from sendcms's `.env.example`, adapted):** Stripe `STRIPE_SECRET_KEY` / `STRIPE_PUBLISHABLE_KEY` / `STRIPE_WEBHOOK_SECRET`; PayPal `PAYPAL_MODE=sandbox|live` + `PAYPAL_CLIENT_ID` / `PAYPAL_CLIENT_SECRET` / `PAYPAL_WEBHOOK_ID`; Polar `POLAR_ACCESS_TOKEN` / `POLAR_WEBHOOK_SECRET` / `POLAR_ENVIRONMENT=sandbox` (production only with a production token); Freemius `FREEMIUS_PRODUCT_ID` / `FREEMIUS_API_KEY` / `FREEMIUS_SECRET_KEY` / `FREEMIUS_PUBLIC_KEY` / `FREEMIUS_PLAN_MAP` (JSON `{ "<planVersionId>": "<freemiusPlanId>" }` — a version with no mapping cannot be sold through Freemius) / `FREEMIUS_SANDBOX` (`true` = sandbox, `false` = production; unset = sandbox in non-production, production when `NODE_ENV=production`; production enablement is an explicit admin action); Gumroad `GUMROAD_APP_ID` / `GUMROAD_APP_SECRET`.
- **Gumroad is registered only — NO checkout.** Gumroad has no API to create a payment or a subscription, so it can be toggled but never sells through checkout. It therefore never appears at checkout regardless of country.
- **Country scope per selling provider.** A provider serves **all countries** or **specific countries** (a list, admin-editable). Starting mapping: Stripe → worldwide; PayPal → USA, UK, etc. At checkout the buyer is only offered the providers active for their country (country from IP geolocation, overridable by the buyer's selection).
- **Nothing is hardcoded.** Providers, active toggles, and country lists are all admin-managed data — no provider or country is written in code.
- **Timing is a separate approval.** Needs provider accounts + business details + tax setup. **No billing code until the user approves the business side.** This plan covers packages + limits, not taking payments (yet).

## 10. Security — the hard parts, stated plainly

- The web becomes the most sensitive piece: user emails + password hashes + the App key. It needs TLS everywhere, argon2id, hashed sessions, rate limits on login / signup, email verification, an audit log, encrypted DB backups.
- **2FA on the web:** TOTP (authenticator app) + backup codes. Opt-in for users; **mandatory for admin accounts** (the admin can see every user and change every package). The desktop login handles the 2FA step through the same web API.
- The broker calls the web's mint endpoint with the user's session token — there is no separate broker credential to leak. The session is the credential; revoking the session revokes the broker's access too.
- A public signup means abuse (bots, fake accounts) — rate limits + email verification first, CAPTCHA later if needed.
- **Honest limits:** the web is the single holder of the App key — a compromised web means a compromised key (same as the central broker would have been; the key is the App's master key either way). Rotation + encrypted backups + the rotation runbook (audit F3) are the mitigation. The App key never enters the desktop build, the repo, logs, or any database.

## 11. What doesn't change

- Sync engine, agent, repos on disk, PAT path, device-token model (90 days).
- The broker, agent and app all run on the user's machine; the installer stays one file.
- The web never sees repo contents, PATs, or GitHub tokens. The GitHub App key never enters the desktop build.

## 12. Rollout (phases)

- **Branch:** `feature/admin-control-plane` (2nd branch, not main), created 2026-11-26 ~07:34 MST (from main at `2f4bbca`) when implementation started. Monorepo layout: web app under `apps/`, shared code under `packages/` (`packages/i18n`, `packages/auth`, `packages/email`, …), following the sendcms pattern (`pnpm-workspace.yaml`, `turbo.json`). Main stays as it is until the branch is reviewed and merged.
- **W1 — web foundation:** Next.js + Postgres + email / password + verification + sessions + dashboard shell + i18n scaffolding (locale trees, RTL shell, en + ur). **Code-complete 2026-11-26** — `packages/auth` (Argon2id via hash-wasm, WASM so it also runs on Cloudflare Workers), `packages/email` (Resend/SMTP via `EMAIL_PROVIDER`), `packages/database` (Neon HTTP driver + `pg` dual driver since 2026-11-27, `0001_init` + `0002_seed_packages`, migrate CLI), auth API (`/api/auth/register|verify|login|logout|reset|reset/confirm`, `/api/me/limits`, `/api/devices`), dashboard shell, i18n en + ur (6 web domains, 85 keys, 0 failures). Verified: package tests 25/25, web auth-flow tests 5/5 against real Postgres (PGlite), typechecks clean, `next build` clean (18 routes). **Database 2026-11-27: the user's own Prisma Postgres (not Neon)** — `DATABASE_URL` on the Vercel project points at it, migrations applied live and verified (12 tables, 5 packages). **Email + OAuth credentials configured by the user 2026-11-27** (Resend + SMTP vars, Google/GitHub OAuth client IDs/secrets, GitHub App id + private key — all in the Vercel env). **First deploy 2026-11-27 ~01:08 MST: branch deployment READY** (`reponyx-qf643fyqp-stylies.vercel.app`; first production attempt failed on Root Directory `./` — fixed to `apps/web` + Output `.next`). **Production deploy DONE 2026-11-27 ~01:11 MST** (`dpl_6YcHm1iZZVuUmFfNqxjRa9NKyRBK`, commit `47c52b2`, target production, `reponyx.bond` live — home 200, login 200, login API returns the app's own `invalid_credentials` for a nonexistent email). **Live register→verify→login test PASSED 2026-11-27 ~01:17 MST** (user did it themselves on reponyx.bond, owner account admin@csofts.com — dashboard: Free, 0/2 accounts, 0/2 devices, Joined 11/27/2026). Email path proven end to end.
- **W2 — OAuth:** Google + GitHub login on the web. Callback URLs (user, 2026-11-27 — use exactly these routes): Google `https://reponyx.bond/api/google/oauth/callback`, GitHub `https://reponyx.bond/api/auth/callback/github`. Client IDs/secrets live in the Vercel env (`GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`, `GITHUB_OAUTH_CLIENT_ID`/`GITHUB_OAUTH_CLIENT_SECRET`); the login OAuth apps are separate from the GitHub App (5896712) used for repo access. **DONE 2026-11-27 (local commit, not pushed yet):** start routes `/api/auth/oauth/{google,github}` → provider authorize URLs; callbacks at the exact registered URLs; state HMAC-signed with `SESSION_SECRET` (10-min TTL, provider-bound); no auto-create (unregistered → `oauth_no_account`); provider-verified email satisfies verification for registered accounts; same account-switch limit + 2FA step as password login; login page shows Google/GitHub buttons only when configured. Tests: 9 new OAuth tests (stubbed provider fetch, real PGlite + routes) + 5 auth-flow = 14/14; `next build` clean (22 routes). **Pushed + deployed 2026-11-27 ~04:13 MST: snapshot `0cf5fff` (sync 69) + web-UI upload `cd010f2` (sync 70, the 8 binary public assets — the snapshot flow cannot carry binaries); Vercel auto-deployed both, latest `dpl_6JifbvLvG1krhkb2U5CZHMXtn68c` READY (production target, commit cd010f2). Live smoke test on reponyx.bond green: home/login 200 with both OAuth buttons, all assets 200, start routes 307 to the providers with the exact registered callback URLs. Still open: a live Google/GitHub click-through (needs the user in a browser).** **W2 polish 2026-11-27 (local, not pushed yet):** OAuth on the register page too (buttons on top of the form, sendcms order); the register intent rides inside the signed state — login still refuses unregistered users, but the register intent creates the account with the provider-verified email + profile name, under the same 1/IP/24h limit (existing email → `email_taken`); full name required at registration (migration `0003_add_user_name`, `users.name`, dashboard + limits API return it); OAuth buttons moved above the email form on login too, divider 'or continue with email'; Terms of Service page (`/terms`, English legal text) + agreement footer on login/register; clean URLs — auth pages now at `/login`, `/register`, `/reset`, `/reset/confirm`, `/verify`, old `/auth/*` routes redirect (query preserved) and email links use the new URLs. Tests 19/19 (5 auth-flow + 14 OAuth), i18n 99 keys 0 failures, `next build` clean (27 routes). **W2 polish, part 2 2026-11-27 (local, not pushed yet):** one account, many sign-in methods — the same verified email is one account whether you sign in with email/password, Google, or GitHub (migration `0004_linked_providers`, `users.has_password`); a login through a provider links it automatically, provider identity resolves to its owner first; dashboard profile has a Sign-in methods card with Link (new `link` intent, provider email must match the account) + Unlink (refused when it would remove the only way in, `last_login_method`); name is mandatory for OAuth registration — a provider that shares no name sends the user to `/register/complete` with a 10-min HMAC-signed pending token, and `POST /api/auth/register/complete` creates the account only once the name is given (profile complete, then login); the profile name is editable (`PATCH /api/me`). Tests 28/28 (5 auth-flow + 19 OAuth + 4 linked), i18n 120 keys 0 failures, `next build` clean (32 routes). Migration 0004 must be applied to the live Prisma DB before the deploy goes live.
- **W3 — desktop login:** login gate, in-app email form, browser OAuth + deep link, device registration, session in the OS store, login screen in en + ur, Register button → web.
- **W4 — broker ↔ web:** web mint endpoint (ownership + limit checks), broker proxies minting to the web instead of signing locally, device-revocation mirroring.
- **W5 — packages + limits + admin dashboard** (users, devices, usage numbers).
- **W6 — billing:** Stripe (separate approval).
- **W7 — E2E + docs + security review** (including a fresh look at the web's auth).
- **i18n from day one:** every new screen ships in en + ur, never English-only first; missing-key CI check from W1.
- Verification per phase: the established gates (`fmt`, clippy with `-D warnings`, `cargo test`, frontend tests, Semgrep, CI windows-build for the desktop) + web: typecheck, tests, Vercel preview deploys.

### Done means (whole platform)

- Register on the web → verify email → log in on the web and in the app; a Google/GitHub login for an unregistered user is refused with a clear message (no auto-created account on login); a Google/GitHub registration on the register page creates the account with the provider-verified email and name.
- Free user links 2 accounts (any mix of Auth/PAT); the 3rd link is refused with a clear message; expiry drops the limits back to Free and extra accounts pause (nothing deleted).
- The admin sees every user (devices, packages, usage, joining date, expiry) and can create/edit any package, including Free.
- en + ur throughout, RTL correct on the web and in the desktop app; the desktop language switcher works.
- CI green on the branch; no secrets in the repo; the App key exists only in the web's env.

## 13. Costs (verify at purchase)

- Vercel Hobby $0 (Pro $20/mo if needed), Postgres — the user's own Prisma Postgres database (the Neon `reponyx-db` from the earlier plan is unused), Resend free 100 emails/day, domain $0–12/yr, payment-provider fees vary (Stripe ~2.9% + 30¢; PayPal, Polar, Freemius, Gumroad each take their own cut — verify at setup; only when billing starts). No broker VPS — the broker runs on the user's machine. **To start: ~$0–5/mo.**

## 14. Decisions log (what changed, and why)

- 2026-11-26 ~06:17 — Central broker chosen, then superseded ~06:25 by the web platform (bigger scope: auth + packages + admin).
- 2026-11-26 ~06:45 — No broker on any server; broker + agent + app all local, one installer. The broker does NOT fetch the key (that would give every user the master key); the web mints tokens, the key never leaves the server.
- 2026-11-26 ~06:50 — One shared GitHub App (5896712) for all users; no per-user Apps.
- 2026-11-26 ~06:51 — One RepoNyx user links many GitHub accounts; the package limit counts linked accounts whether Auth or PAT.
- 2026-11-26 ~06:55 — Packages: Free 2 (any mix), Bronze 10, Silver 20, Gold 50, Diamond 100; admin-created. Isolation is absolute.
- 2026-11-26 ~06:56 — App shows account read-only, updates via a web button; expiry reverts to Free; web shows machine name / Windows / IP / usage; admin sees every user.
- 2026-11-26 ~07:04 — Registration 1 per IP per 24h; account-switch 1h, device-first, IP fallback; structured errors with retry time.
- 2026-11-26 ~07:06 — No package hardcoded, including Free.
- 2026-11-26 ~07:13 — i18n: `packages/i18n`, `locales/{en,ur}/{web,desktop}`, framework-free translator (sendcms pattern, verified).
- 2026-11-26 ~07:19 — Everything on a 2nd branch, monorepo layout.
- 2026-11-26 ~07:21 — sendcms is a contact-form backend, NOT a verification-email provider (verified via demo + docs). Email = Resend/SMTP; website forms = sendcms.
- 2026-11-26 ~07:23 — Branch named `feature/admin-control-plane`.
- 2026-11-26 ~07:26 — Email provider selected by env var (`EMAIL_PROVIDER=resend|smtp`); both credential sets can live in `.env`.
- 2026-11-26 ~07:26 — Web hosts on Vercel, but must also run on Cloudflare Workers (portable build; Neon driver kept for the Workers path, `pg` for standard Postgres). All six earlier open questions confirmed already answered — only billing remains open, and it is not re-asked.
- 2026-11-26 ~07:33 — Payment providers: Stripe, PayPal, Polar, Freemius, Gumroad; admin toggles which are active; each provider has a country scope (all or specific — e.g. Stripe worldwide, PayPal USA/UK, Gumroad Pakistan/India); nothing hardcoded.

## 15. Open questions

1. **Billing timing:** providers decided (Stripe, PayPal, Polar, Freemius, Gumroad — admin-controlled, country-scoped); now, later, or never? Separate approval — no billing code until decided.

## Related work (not part of this plan)

- Accounts page restructure (`ai/phase6/accounts-page-plan.md`) — separate plan, awaiting the user's approval.
- Central-broker plan (`ai/phase6/central-broker-plan.md`) — superseded by this plan.
