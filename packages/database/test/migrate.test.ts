import test from "node:test";
import assert from "node:assert/strict";
import type { DbClient } from "../src/client";
import { migrate } from "../src/migrate";

class RecordingDbClient implements DbClient {
  calls: string[] = [];
  appliedVersions: string[] = [];

  async query<T>(text: string, params: unknown[] = []): Promise<T[]> {
    this.calls.push(text.trim().slice(0, 60));
    if (text.includes("FROM schema_migrations")) {
      return this.appliedVersions.map((version) => ({ version })) as T[];
    }
    if (text.startsWith("INSERT INTO schema_migrations")) {
      this.appliedVersions.push(String(params[0]));
    }
    return [] as T[];
  }
}

test("migrate applies pending migrations in order, then skips them", async () => {
  const db = new RecordingDbClient();
  const migrations = [
    { version: "0001_init", sql: "CREATE TABLE users (id uuid);" },
    {
      version: "0002_seed_packages",
      sql: "INSERT INTO packages VALUES ('Free');",
    },
  ];
  assert.deepEqual(await migrate(db, migrations), [
    "0001_init",
    "0002_seed_packages",
  ]);
  assert.deepEqual(await migrate(db, migrations), []);
  assert.deepEqual(db.appliedVersions, ["0001_init", "0002_seed_packages"]);
});

test("migrate skips already-applied versions", async () => {
  const db = new RecordingDbClient();
  db.appliedVersions = ["0001_init"];
  const applied = await migrate(db, [
    { version: "0001_init", sql: "CREATE TABLE users (id uuid);" },
    {
      version: "0002_seed_packages",
      sql: "INSERT INTO packages VALUES ('Free');",
    },
  ]);
  assert.deepEqual(applied, ["0002_seed_packages"]);
});
