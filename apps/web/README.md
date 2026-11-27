# @reponyx/web — RepoNyx control-plane web app

Next.js (React/TS) web app: registration, login, dashboard, admin,
packages + limits, and (W6, separate approval) billing. The web is the
control plane — it never sees repo contents, PATs, or GitHub tokens.
The GitHub App private key lives only in the web's env (see
`.env.example`); the local broker asks the web to mint installation
tokens, and the web mints them with the key.

## Run

```bash
pnpm install
pnpm web        # next dev (http://localhost:3000)
pnpm web:build  # next build
```

Copy `.env.example` to `.env.local` and fill in the values. Never commit
real values.

## i18n

Translations live in `packages/i18n` (`@reponyx/i18n`). The root layout
reads the `reponyx-locale` cookie and sets `lang` + `dir` from it
(`en` LTR, `ur` RTL). Pages use `getWebTranslator(domain)` from
`src/lib/i18n.ts`. Server errors are localized by the client — the API
returns code + data only.

## Status

W1 code-complete 2026-11-26 (branch `feature/admin-control-plane`):
`packages/auth` (Argon2id via hash-wasm + tokens), `packages/email`
(Resend/SMTP selector), `packages/database` (Neon driver, schema,
migrations, migrate CLI), auth API (register / verify / login / logout /
reset + sessions + devices + limits), dashboard shell (package, usage,
devices, linked accounts), i18n en + ur (6 web domains, 85 keys).
Verified: package tests 25/25, web auth-flow tests 5/5 against real
Postgres (PGlite), typechecks clean, `next build` clean.

Still open: the live Neon project (the `DATABASE_URL` for `.env.local`
and Vercel) and the first Vercel deploy — both need the Neon project
to exist. OAuth (W2), desktop login (W3), broker↔web minting (W4),
packages + admin (W5), billing (W6, separate approval) land in order —
see `ai/phase6/web-platform-plan.md`.

## Formatting

Prettier 3.10.1 + `prettier-plugin-tailwindcss` (root `.prettierrc`;
`pnpm format` / `pnpm format:check`, scoped to the web platform).
Every file is properly formatted — JSON pretty-printed (2-space, keys
alphabetical, final newline), never a single-line dump.

## Design

Tailwind v4 + the sendcms dashboard design tokens
(`src/app/design-tokens.css`, OKLCH teal/emerald, light + dark) +
Geist + `next-themes`. Auth pages use the two-column `AuthShell`;
shared classes live in `src/components/ui.ts`.

## Public assets

`public/` holds the favicon set, `logo.png`, `apple-touch-icon.png`,
`icon-192.png` / `icon-512.png`, `og-image.png` (1200×630) and
`site.webmanifest` — all generated from the desktop app icon
(`apps/desktop/src-tauri/icons/icon.png`, 1024px) with the exact token
colors. The root layout wires them into metadata (icons, manifest,
Open Graph, theme color).

## Status

W2 done 2026-11-27 (local, not pushed yet): Google + GitHub OAuth
(start routes + callbacks at the exact registered URLs, HMAC-signed
state via `SESSION_SECRET`, no auto-create), sendcms design + proper
pages, public folder, Prettier formatting. Verified: package tests
25/25, web tests 14/14 (5 auth-flow + 9 OAuth), i18n validate 95 keys
0 failures, typechecks clean, `next build` clean (22 routes).
