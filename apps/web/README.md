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
