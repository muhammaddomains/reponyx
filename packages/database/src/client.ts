import { neon } from "@neondatabase/serverless";

/**
 * Minimal database client over the Neon serverless (HTTP) driver.
 * The HTTP driver is what makes the web portable: it works on Vercel
 * (Node) and on Cloudflare Workers (no TCP sockets needed). Queries
 * are plain parameterized SQL — the same SQL `psql` runs.
 */
export interface DbClient {
  query<T = Record<string, unknown>>(text: string, params?: unknown[]): Promise<T[]>;
}

export function createDbClient(databaseUrl: string | undefined): DbClient {
  if (!databaseUrl) {
    throw new Error(
      "DATABASE_URL is not set. Create a Neon project and copy its connection string into .env.local (see apps/web/.env.example).",
    );
  }
  const sql = neon(databaseUrl);
  return {
    async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
      const rows = await sql.query(text, params);
      return rows as T[];
    },
  };
}
