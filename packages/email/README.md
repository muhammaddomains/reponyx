# @reponyx/email — pluggable email (web)

One send path for all platform email (verification, reset,
notifications), mirroring sendcms's `packages/email` provider shape:

- **Selection:** `EMAIL_PROVIDER=resend|smtp` in the env picks the
  provider; both credential sets can live in `.env` at once. Unset →
  Resend if its key is present, else SMTP if configured, else `none`
  (callers fail loudly — no silent drops).
- **Resend:** plain `fetch` POST to `https://api.resend.com/emails`,
  Bearer auth, 15s timeout. No SDK.
- **SMTP:** `nodemailer` transport from `SMTP_HOST`/`SMTP_PORT`/
  `SMTP_USER`/`SMTP_PASS` (port 465 → TLS, else STARTTLS on 587).
- **Admin view:** `describeEmailConfig` reports only whether each
  provider is configured — never the keys.

Templates live in `src/templates.ts` (verification 24h, reset 1h).

Tests: `pnpm --filter @reponyx/email test` (Node's built-in runner;
the Resend test stubs `fetch`, the SMTP test uses a stream transport).
