// Validates packages/i18n locales: every JSON parses, and en + ur have
// identical key sets per domain file. No dependencies — plain Node.
// Exits non-zero on any mismatch (runs in CI via `pnpm --filter @reponyx/i18n validate`).
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "locales");
const SOURCE = "en";
const LOCALES = ["en", "ur"];

async function listJsonFiles(dir, prefix = "") {
  const entries = await readdir(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) files.push(...(await listJsonFiles(path.join(dir, entry.name), rel)));
    else if (entry.name.endsWith(".json")) files.push(rel);
  }
  return files.sort();
}

function keyDiff(a, b) {
  const inA = new Set(Object.keys(a));
  const inB = new Set(Object.keys(b));
  return {
    missingInB: [...inA].filter((key) => !inB.has(key)),
    extraInB: [...inB].filter((key) => !inA.has(key)),
  };
}

const files = await listJsonFiles(path.join(root, SOURCE));
let failures = 0;
let keys = 0;

for (const file of files) {
  const sourceText = await readFile(path.join(root, SOURCE, file), "utf8");
  const source = JSON.parse(sourceText);
  keys += Object.keys(source).length;
  for (const locale of LOCALES) {
    if (locale === SOURCE) continue;
    let target;
    try {
      target = JSON.parse(await readFile(path.join(root, locale, file), "utf8"));
    } catch {
      console.error(`FAIL ${file}: missing or unparsable in ${locale}`);
      failures += 1;
      continue;
    }
    const { missingInB, extraInB } = keyDiff(source, target);
    if (missingInB.length > 0 || extraInB.length > 0) {
      console.error(`FAIL ${file} (${locale}): missing ${JSON.stringify(missingInB)}, extra ${JSON.stringify(extraInB)}`);
      failures += 1;
    }
  }
}

console.log(`i18n: ${files.length} domain files, ${keys} keys in ${SOURCE}, ${failures} failure(s)`);
process.exit(failures > 0 ? 1 : 0);
