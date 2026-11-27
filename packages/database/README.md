# @reponyx/database — web database (Neon Postgres)

- **Driver:** `@neondatabase/serverless` (HTTP) — the web's only
  database access. The HTTP driver is what keeps the web portable:
  it works on Vercel (Node) and on Cloudflare Workers (no TCP
  sockets needed). Queries are plain parameterized SQL — the same
  SQL `psql` runs.
- **Schema:** `migrations/0001_init.sql` (users, sessions, devices,
  linked GitHub accounts, installations, packages, subscriptions,
  registration attempts, logout events, audit log) and
  `migrations/0002_seed_packages.sql` (Free/Bronze/Silver/Gold/Diamond
  starting set). Never edit an applied migration — add a new one.
- **Migrate:** `pnpm --filter @reponyx/database migrate` (needs
  `DATABASE_URL` — a Neon connection string). Tracks applied
  versions in `schema_migrations`; re-running is a no-op.
- **What the DB never holds:** tokens, PATs, or the GitHub App key.
  Sessions and verification/reset tokens are stored as SHA-256
  hashes only; PATs never leave the user's own machine.

Tests: `pnpm --filter @reponyx/database test` (Node's built-in
runner; repository functions run against a fake client that
dispatches on the `/* name */` marker in each statement).
