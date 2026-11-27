import { neon } from "@neondatabase/serverless";
import { Pool, type PoolConfig } from "pg";

/**
 * Database client with two drivers behind one interface:
 * - Neon hosts (*.neon.tech) use the Neon serverless (HTTP) driver —
 *   the only driver that also runs on Cloudflare Workers (no TCP).
 * - Any other Postgres (Prisma Postgres, self-hosted, ...) uses `pg`
 *   (TCP), which runs on Vercel's Node runtime.
 * The driver is picked from the DATABASE_URL host — no config needed.
 * Queries are plain parameterized SQL ($1, $2, ...), identical for both.
 */
export interface DbClient {
  query<T = Record<string, unknown>>(
    text: string,
    params?: unknown[]
  ): Promise<T[]>;
  /** Release the underlying pool, if the driver holds one (pg). No-op for Neon. */
  close?(): Promise<void>;
}

function isNeonHost(databaseUrl: string): boolean {
  try {
    return new URL(databaseUrl).hostname.endsWith(".neon.tech");
  } catch {
    return false;
  }
}

function pgSslConfig(databaseUrl: string): PoolConfig["ssl"] {
  let sslmode: string | null = null;
  let hostname = "";
  try {
    const parsed = new URL(databaseUrl);
    sslmode = parsed.searchParams.get("sslmode");
    hostname = parsed.hostname;
  } catch {
    // Fall through to the secure default below.
  }
  if (sslmode === "disable") return false;
  if (sslmode === "prefer" || sslmode === "require") {
    return { rejectUnauthorized: false };
  }
  if (hostname === "localhost" || hostname === "127.0.0.1") return false;
  // verify-ca / verify-full / unset on a remote host: verify the cert
  // (Node also verifies the hostname, matching verify-full).
  return { rejectUnauthorized: true };
}

export function createDbClient(databaseUrl: string | undefined): DbClient {
  if (!databaseUrl) {
    throw new Error(
      "DATABASE_URL is not set. Create a Postgres database (Neon, Prisma Postgres, or any Postgres) and copy its connection string into .env.local (see apps/web/.env.example)."
    );
  }
  if (isNeonHost(databaseUrl)) {
    const sql = neon(databaseUrl);
    return {
      async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
        const rows = await sql.query(text, params);
        return rows as T[];
      },
    };
  }
  const pool = new Pool({
    connectionString: databaseUrl,
    ssl: pgSslConfig(databaseUrl),
  });
  return {
    async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
      const result = await pool.query(text, params);
      return result.rows as T[];
    },
    async close(): Promise<void> {
      await pool.end();
    },
  };
}
