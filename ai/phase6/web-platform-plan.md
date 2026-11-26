# RepoNyx web platform — plan (2026-11-26, Phase 6)

## The idea (user's words)

- A **web** with auth: Google / GitHub / email, email
  verification.
- After installing the app, the **main page is a login** that
  uses the web's API. Email → form right in the app;
  Google/GitHub → URL opens in Chrome/Firefox and the account
  gets linked.
- On the web, **every user's info**: how many devices, how
  many users are using it.
- The **private key lives in the web's environment**; the
  broker fetches the key from there — and anything else it
  needs.
- On the web, **packages + limits**: when the account expires,
  how many accounts can be linked, etc.

## Recommendation: web = control plane, app = experience, broker = key holder

Three pieces, each with one job. This plan **absorbs** the
central-broker plan (its broker hosting stays) and moves the
pairing/admin pieces to the web:

- **Desktop** ↔ **Web API**: login, session, devices, limits,
  packages. The app is the experience; the web is the account.
- **Broker** ↔ **Web API**: fetches the App key at startup,
  reports pairing/device state. The broker still mints the
  GitHub tokens; the web never mints anything.
- **Desktop** ↔ **Broker**: pairing + token minting —
  unchanged. **Broker** ↔ **GitHub**: installations + tokens —
  unchanged.
- The web **never sees** repo contents, PATs, or GitHub tokens.

## The pieces

### 1. The web (Next.js on Vercel + Postgres)

- **Stack:** Next.js (React/TS — the same stack as the
  desktop), **Vercel** (already connected), Postgres (Neon /
  Vercel Postgres), **Resend** for email (free tier: 100
  emails/day) — or the user's own SMTP.
- **Auth:** email + password (argon2id hashing, verification
  link/code, sessions as random tokens hashed server-side) +
  Google + GitHub OAuth. The **web is the OAuth client** — its
  client secrets live in the web's env (Vercel env vars, never
  in the repo); the desktop holds no OAuth secrets.
- **Data:** users, sessions, devices, linked GitHub accounts
  (**logins only, no tokens**), packages, subscriptions, audit
  log.
- **Pages:** login / signup, verify email, dashboard (my
  devices, linked accounts, usage, package + expiry), admin
  (all users, devices, packages).
- **Responsibility shift, stated plainly:** the web holds
  other people's emails + password hashes. That needs a
  privacy policy + terms, encrypted DB backups, and the same
  "no secrets in the repo" rule. The user stops being just a
  user of the app and becomes the operator of a service.

### 2. Desktop login — the main page becomes login

- The app opens to **Login** (no Overview until
  authenticated). **Email** → in-app form → session token →
  OS credential store (reuse `reponyx-credential-store`, where
  PATs already live). **Google/GitHub** → the system browser
  opens the web's OAuth URL → authorize → redirect to
  `reponyx://auth?code=...` → the desktop picks it up via the
  **deep-link plugin** (new: `@tauri-apps/plugin-deep-link` +
  protocol registration in `tauri.conf.json` — no deep-link
  setup exists today) → the desktop exchanges the code for a
  session token.
- On login the desktop **registers its device** (the existing
  device identifier + name, platform, app version, last seen)
  and refreshes last-seen on each start.
- Session expiry + logout + re-login. **Offline:** an
  already-logged-in session keeps working locally (repos are
  on disk), but broker features need the web + broker
  reachable.
- **Open question:** login mandatory for everyone, or only for
  Auth / broker features (PAT-only offline use without an
  account)? The plan assumes login-first for all — the user's
  words were "app install k bad main page login ho" — but it is
  flagged as a decision.

### 3. Broker fetches the key from the web

- **Today:** the key comes from
  `REPONYX_BROKER_PRIVATE_KEY[_PATH]` at startup. **New:** the
  broker can fetch the key from the web API at startup (and
  re-fetch on rotation) — authenticated with a **per-broker
  credential** (long-lived API token in the broker's env, or
  mTLS), over TLS, held **in memory only**, never logged, never
  written to disk, never sent to a desktop.
- **Honest limit:** this moves the trust, it doesn't remove
  it. The broker's fetch credential becomes as sensitive as the
  key, and a compromised web means a compromised key. What it
  buys: one place to rotate / revoke, no key file on any
  machine, and the broker can run on a plain VPS with no
  secrets baked into its image.
- The web serves the key **only to an authenticated broker**;
  the endpoint is rate-limited + audited. Rotation = new key in
  the web env → brokers re-fetch (or restart).

### 4. Devices, limits, packages — the web as source of truth

- **Devices:** register on login, last-seen heartbeat, revoke
  from the dashboard (the web tells the broker to revoke the
  device token too, so revocation bites in both places).
- **Limits enforced at pairing / link time** (not in the sync
  hot path): when a device requests pairing or links a GitHub
  account, the broker asks the web "is this user within
  limits?" — or the desktop asks the web first and passes a
  short-lived entitlement. A package defines: max devices, max
  linked GitHub accounts, expiry date, (later) max repos.
- **Packages:** e.g. Free (1 device, 2 accounts) / Pro (more)
  — the exact numbers are a **business decision, not a code
  decision**. Expiry: the account shows "expires on X";
  expired → Auth features pause (local repos + PAT keep
  working — to be decided in implementation).
- **The numbers the user asked for:** total users, devices per
  user, active users — on the admin dashboard.

### 5. Billing — separate, later, needs its own approval

- Stripe Checkout + webhooks → subscription status drives the
  package. Needs a Stripe account + business details + tax
  setup. **No billing code until the user approves the business
  side.** This plan covers packages + limits, not taking
  payments.

## What doesn't change

- Sync engine, agent, repos on disk, PAT path, broker token
  minting, device-token model (90 days), local mode for the
  owner.
- The web never sees repo contents, PATs, or GitHub tokens.
  The GitHub App key never enters the desktop build.

## Security — the hard parts, stated plainly

- The web becomes the most sensitive piece: user emails +
  password hashes + the App key. It needs TLS everywhere,
  argon2id, hashed sessions, rate limits on login / signup,
  email verification, an audit log, encrypted DB backups.
- The broker's fetch credential: treat it like a password;
  rotate it; if leaked, revoke it at the web.
- A public signup means abuse (bots, fake accounts) — rate
  limits + email verification first, CAPTCHA later if needed.

## Rollout (phases)

- **W1 — web foundation:** Next.js + Postgres + email /
  password + verification + sessions + dashboard shell.
- **W2 — OAuth:** Google + GitHub login on the web.
- **W3 — desktop login:** login gate, in-app email form,
  browser OAuth + deep link, device registration, session in
  the OS store.
- **W4 — broker ↔ web:** key fetch, device-revocation
  mirroring, limit checks at pairing / link.
- **W5 — packages + limits + admin dashboard** (users,
  devices, usage numbers).
- **W6 — billing:** Stripe (separate approval).
- **W7 — E2E + docs + security review** (including a fresh
  look at the web's auth).
- Verification per phase: the established gates (`fmt`,
  clippy with `-D warnings`, `cargo test`, frontend tests,
  Semgrep, CI windows-build for the desktop) + web: typecheck,
  tests, Vercel preview deploys.

## Costs (verify at purchase)

- Vercel Hobby $0 (Pro $20/mo if needed), Postgres Neon free
  tier (then ~$5–10/mo), Resend free 100 emails/day, VPS for
  the broker ~€4.50/mo, domain $0–12/yr, Stripe 2.9% + 30¢ per
  payment (only when billing starts). **To start: ~$5–10/mo.**

## Open questions (before implementation)

1. Login mandatory for all, or only for Auth / broker features
   (PAT-only offline use without an account)?
2. Web hosting: Vercel (recommended — already connected) or the
   same VPS as the broker?
3. Email provider: Resend, or the user's own SMTP?
4. Package numbers (Free vs Pro limits) + what happens when a
   package lapses?
5. Billing: Stripe now, later, or never (free service)?
6. The central-broker plan's hosting (C2) is absorbed here —
   the broker still needs its VPS. Approve both together, or
   web first?
