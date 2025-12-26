import { describe, it } from "node:test";
import * as assert from "node:assert";
import {
  encodeVarint,
  decodeVarint,
  zigzagEncode,
  zigzagDecode,
  encodeSignedVarint,
  decodeSignedVarint,
  ByteReader,
} from "./varint.js";

describe("varint encoding", () => {
  it("encodes small values", () => {
    assert.deepStrictEqual(encodeVarint(0n), new Uint8Array([0x00]));
    assert.deepStrictEqual(encodeVarint(1n), new Uint8Array([0x01]));
    assert.deepStrictEqual(encodeVarint(127n), new Uint8Array([0x7f]));
  });

  it("encodes multi-byte values", () => {
    assert.deepStrictEqual(encodeVarint(128n), new Uint8Array([0x80, 0x01]));
    assert.deepStrictEqual(encodeVarint(300n), new Uint8Array([0xac, 0x02]));
    assert.deepStrictEqual(
      encodeVarint(16384n),
      new Uint8Array([0x80, 0x80, 0x01])
    );
  });

  it("roundtrips values", () => {
    const values = [0n, 1n, 127n, 128n, 255n, 300n, 16384n, 1000000n];
    for (const v of values) {
      const encoded = encodeVarint(v);
      const reader = new ByteReader(encoded);
      const decoded = decodeVarint(reader);
      assert.strictEqual(decoded, v, `Failed for value ${v}`);
    }
  });
});

describe("zigzag encoding", () => {
  it("encodes correctly", () => {
    assert.strictEqual(zigzagEncode(0n), 0n);
    assert.strictEqual(zigzagEncode(-1n), 1n);
    assert.strictEqual(zigzagEncode(1n), 2n);
    assert.strictEqual(zigzagEncode(-2n), 3n);
    assert.strictEqual(zigzagEncode(2n), 4n);
  });

  it("decodes correctly", () => {
    assert.strictEqual(zigzagDecode(0n), 0n);
    assert.strictEqual(zigzagDecode(1n), -1n);
    assert.strictEqual(zigzagDecode(2n), 1n);
    assert.strictEqual(zigzagDecode(3n), -2n);
    assert.strictEqual(zigzagDecode(4n), 2n);
  });

  it("roundtrips values", () => {
    const values = [0n, 1n, -1n, 100n, -100n, 1000000n, -1000000n];
    for (const v of values) {
      assert.strictEqual(zigzagDecode(zigzagEncode(v)), v);
    }
  });
});

describe("signed varint", () => {
  it("roundtrips values", () => {
    const values = [0n, 1n, -1n, 100n, -100n, 1000000n, -1000000n];
    for (const v of values) {
      const encoded = encodeSignedVarint(v);
      const reader = new ByteReader(encoded);
      const decoded = decodeSignedVarint(reader);
      assert.strictEqual(decoded, v, `Failed for value ${v}`);
    }
  });
});
