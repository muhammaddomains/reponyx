# W5 — packages + limits + admin dashboard

Status: IMPLEMENTED 2026-11-28 (local commit pending, not pushed yet) (user decision ~07:25 MST: stop piecemeal
debugging of the pairing/link/device issues; build all remaining phases
W5 → W6 → W7 first, then ONE consolidated review of everything).

Branch: `feature/admin-control-plane`. Same verification gates as W1–W4
(fmt; clippy `-D warnings` excl. desktop; cargo test excl. desktop;
frontend tests; typecheck; lint; vite build; Semgrep with the exact CI
command; prettier on `apps/web/**` + `packages/{i18n,auth,email,database}/**`).
Push only on approval (snapshot flow, manifest builder, sync 84+).

## Scope (from the web-platform plan §12 + §"Done means")

1. **Admin dashboard** — the admin sees every user: devices, packages,
   usage numbers, joining date, expiry. Admin-only routes + UI.
   Admin is mandatory-2FA (see §2): an admin without 2FA cannot open
   the admin dashboard.
2. **Package management (admin)** — create/edit any package, including
   Free (none hardcoded). Fields are the limits: `max_linked_accounts`,
   `max_devices` (+ name). Price/billing fields belong to W6, not W5.
   Editing Free's limits takes effect immediately (the mint/link rank
   checks read the package row, W4).
3. **TOTP enrollment (2FA)** — opt-in for users, mandatory for admins.
   Web: setup (secret + QR code + backup codes), confirm/enable,
   disable, regenerate backup codes. `POST /api/auth/2fa/verify`
   already exists (W3); enrollment needs new endpoints. Desktop login
   already handles the 2FA step (W3) — no desktop change expected
   unless the enrollment flow needs one.
4. **Limits, enforced + visible** — the mint/link paths already check
   limits (W4: rank oldest-first, `limit_exceeded` with `{max}`).
   W5 adds the admin-side view of usage vs limits and the user-side
   display (dashboard already shows Linked x/y, Devices x/y).
5. **i18n en + ur** for every new screen (never English-only first);
   missing-key CI check from W1.

## Out of scope (later phases)

- W6 billing (Stripe first, then PayPal/Polar/Freemius/Gumroad;
  admin toggles, country-scoped; sandbox/live) — separate approval
  in the original plan; the user said 2026-11-28 ~07:25 "build all
  remaining phases", which is taken as approval to build W6 after W5
  unless they say otherwise.
- W7 E2E + docs + security review (incl. a fresh look at web auth).
- The consolidated review of the pairing/link/device behaviour
  (user decision 2026-11-28 ~07:25): add-account → immediate web/DB
  + real-time count in the app. That is a review item AFTER W5–W7,
  not a W5 fix.

## Open questions to settle from code (survey first)

- How is "admin" determined today? (`users.is_admin` column? the
  owner email `admin@csofts.com`? Check the live DB + `0001_init.sql`
  + the session/user code.)
- Packages table fields (what exists: name, max_linked_accounts,
  max_devices — check `0002_seed_packages` + queries).
- TOTP state: does `@reponyx/auth` already store secrets/backup
  codes, or is that new? (W3 added verify only.)
- Admin dashboard home: new route (`/admin`) + nav, or a dashboard
  section? (Decide from the existing dashboard shell.)

## Verification (per phase)

- Package tests + web tests (PGlite) for the new queries/routes;
  typecheck; lint; prettier; Semgrep; `next build`.
- Live: admin logs in, sees all users; edits Free limits; a user
  enrolls TOTP and logs in with it; an admin without 2FA is refused
  the admin pages.
