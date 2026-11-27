# @reponyx/database — web database (Postgres)

- **Driver:** two drivers behind one `DbClient` interface, picked from
  the `DATABASE_URL` host — no config needed. Neon hosts
  (`*.neon.tech`) use `@neondatabase/serverless` (HTTP), the only
  driver that also runs on Cloudflare Workers (no TCP sockets
  needed). Any other Postgres (Prisma Postgres, self-hosted, ...)
  uses `pg` (TCP), which runs on Vercel's Node runtime. Queries are
  plain parameterized SQL — the same SQL `psql` runs. (2026-11-27:
  the Neon HTTP driver cannot talk to Prisma Postgres — it POSTs to
  `https://<host>/sql` and Prisma answers HTTP 404 — so `pg` was
  added for standard Postgres; the Neon path stays for Workers.)
- **Schema:** `migrations/0001_init.sql` (users, sessions, devices,
  linked GitHub accounts, installations, packages, subscriptions,
  registration attempts, logout events, audit log) and
  `migrations/0002_seed_packages.sql` (Free/Bronze/Silver/Gold/Diamond
  starting set). Never edit an applied migration — add a new one.
- **Migrate:** `pnpm --filter @reponyx/database migrate` (needs
  `DATABASE_URL` — a Neon or any Postgres connection string).
  Tracks applied versions in `schema_migrations`; re-running is a
  no-op. The migrate CLI closes the pool on exit (`client.close()`),
  so a `pg` run terminates instead of hanging.
- **What the DB never holds:** tokens, PATs, or the GitHub App key.
  Sessions and verification/reset tokens are stored as SHA-256
  hashes only; PATs never leave the user's own machine.

Tests: `pnpm --filter @reponyx/database test` (Node's built-in
runner; repository functions run against a fake client that
dispatches on the `/* name */` marker in each statement).
