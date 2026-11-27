import { argon2id, argon2Verify } from "hash-wasm";
import { randomBytes } from "crypto";

/**
 * Password hashing — Argon2id, the algorithm the plan requires.
 * Implemented with hash-wasm (pure WebAssembly, no native binding) so
 * the same code runs on Vercel (Node) and on Cloudflare Workers.
 * Parameters are the OWASP first-party defaults: 64 MiB memory,
 * 3 iterations, parallelism 1.
 */
const ARGON2_MEMORY_KIB = 65_536;
const ARGON2_ITERATIONS = 3;
const ARGON2_PARALLELISM = 1;
const ARGON2_HASH_LENGTH = 32;
const ARGON2_SALT_LENGTH = 16;

export async function hashPassword(password: string): Promise<string> {
  const salt = randomBytes(ARGON2_SALT_LENGTH);
  return argon2id({
    password,
    salt,
    parallelism: ARGON2_PARALLELISM,
    iterations: ARGON2_ITERATIONS,
    memorySize: ARGON2_MEMORY_KIB,
    hashLength: ARGON2_HASH_LENGTH,
    outputType: "encoded",
  });
}

export async function verifyPassword(password: string, encodedHash: string): Promise<boolean> {
  try {
    return await argon2Verify({ password, hash: encodedHash });
  } catch {
    // Malformed stored hash — never crash login on it.
    return false;
  }
}
