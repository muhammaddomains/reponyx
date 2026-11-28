import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  BACKUP_CODE_COUNT,
  generateBackupCodes,
  normalizeBackupCode,
} from "../src/backup-codes.ts";

describe("backup codes", () => {
  it("generates 10 distinct codes in XXXXX-XXXXX form", () => {
    const codes = generateBackupCodes();
    assert.equal(codes.length, BACKUP_CODE_COUNT);
    assert.equal(new Set(codes).size, codes.length);
    for (const code of codes) {
      assert.match(code, /^[A-HJ-NP-Z2-9]{5}-[A-HJ-NP-Z2-9]{5}$/);
    }
  });

  it("normalizes dashes, spaces and case for comparison", () => {
    assert.equal(normalizeBackupCode("abcde-fghij"), "ABCDEFGHIJ");
    assert.equal(normalizeBackupCode(" ab cd-ef "), "ABCDEF");
    assert.equal(normalizeBackupCode("ABCDE-FGHIJ"), "ABCDEFGHIJ");
  });
});
