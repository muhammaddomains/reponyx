// Applies pending migrations to the Neon database named by DATABASE_URL.
// Usage: pnpm --filter @reponyx/database migrate
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createDbClient, migrate } from "../src/index";

const migrationsDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "migrations",
);

const files = (await readdir(migrationsDir))
  .filter((name) => name.endsWith(".sql"))
  .sort();
const migrations = [];
for (const file of files) {
  migrations.push({
    version: file.replace(/\.sql$/, ""),
    sql: await readFile(path.join(migrationsDir, file), "utf8"),
  });
}

const client = createDbClient(process.env.DATABASE_URL);
try {
  const applied = await migrate(client, migrations);
  if (applied.length === 0) {
    console.log("database: already up to date");
  } else {
    console.log(`database: applied ${applied.join(", ")}`);
  }
} finally {
  await client.close?.();
}
