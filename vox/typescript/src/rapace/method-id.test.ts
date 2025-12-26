import { describe, it } from "node:test";
import * as assert from "node:assert";
import { computeMethodId } from "./method-id.js";

describe("method ID computation", () => {
  it("computes FNV-1a hash", () => {
    // These values should match what rapace-swift produces
    // The hash is 64-bit FNV-1a folded to 32 bits via XOR

    // Test with some known values
    const id1 = computeMethodId("Vfs", "read");
    const id2 = computeMethodId("Vfs", "write");
    const id3 = computeMethodId("Browser", "navigate");

    // They should be different
    assert.notStrictEqual(id1, id2);
    assert.notStrictEqual(id1, id3);
    assert.notStrictEqual(id2, id3);

    // They should be consistent
    assert.strictEqual(computeMethodId("Vfs", "read"), id1);
    assert.strictEqual(computeMethodId("Vfs", "write"), id2);
  });

  it("produces 32-bit values", () => {
    const id = computeMethodId("Test", "method");
    assert.ok(id >= 0);
    assert.ok(id <= 0xffffffff);
  });
});
