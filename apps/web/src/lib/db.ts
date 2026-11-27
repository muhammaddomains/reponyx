import { createDbClient, type DbClient } from "@reponyx/database";

let client: DbClient | null = null;
let override: DbClient | null = null;

/** Test-only: replace the database client (used by the route-flow tests). */
export function setDbClientForTests(next: DbClient | null): void {
  override = next;
}

/** Memoized Neon client. Throws at request time (not import time) when DATABASE_URL is missing, so `next build` still works without a database. */
export function getDb(): DbClient {
  if (override) return override;
  if (!client) client = createDbClient(process.env.DATABASE_URL);
  return client;
}
