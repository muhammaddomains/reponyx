import type { DbClient } from "./client";

export interface Migration {
  version: string;
  sql: string;
}

/**
 * Applies pending migrations in order, tracking them in
 * `schema_migrations`. A migration runs exactly once; re-running is a
 * no-op. Never edit an applied migration — add a new one.
 */
export async function migrate(
  client: DbClient,
  migrations: Migration[]
): Promise<string[]> {
  await client.query(
    `CREATE TABLE IF NOT EXISTS schema_migrations (
       version text PRIMARY KEY,
       applied_at timestamptz NOT NULL DEFAULT now()
     )`
  );
  const applied = await client.query<{ version: string }>(
    `SELECT version FROM schema_migrations ORDER BY version`
  );
  const appliedSet = new Set(applied.map((row) => row.version));
  const newly: string[] = [];
  for (const migration of migrations) {
    if (appliedSet.has(migration.version)) continue;
    await client.query(migration.sql);
    await client.query(`INSERT INTO schema_migrations (version) VALUES ($1)`, [
      migration.version,
    ]);
    newly.push(migration.version);
  }
  return newly;
}
