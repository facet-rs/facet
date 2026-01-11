import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import {
  encodeBool,
  encodeU8,
  encodeI8,
  encodeU16,
  encodeI16,
  encodeU32,
  encodeI32,
  encodeU64,
  encodeI64,
  encodeF32,
  encodeF64,
  encodeString,
  encodeBytes,
  encodeOption,
  encodeVec,
  encodeVarint,
} from "@bearcove/roam-postcard";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** Load a golden vector from the test-fixtures directory */
function loadGoldenVector(path: string): Uint8Array {
  const projectRoot = join(__dirname, "..", "..", "..", "..");
  const vectorPath = join(projectRoot, "test-fixtures", "golden-vectors", path);
  return new Uint8Array(readFileSync(vectorPath));
}

/** Assert that encoded bytes match a golden vector */
function assertEncoding(encoded: Uint8Array, vectorPath: string) {
  const expected = loadGoldenVector(vectorPath);
  expect(Array.from(encoded)).toEqual(Array.from(expected));
}

describe("Varint encoding", () => {
  it("encodes varints correctly", () => {
    assertEncoding(encodeVarint(0), "varint/u64_0.bin");
    assertEncoding(encodeVarint(1), "varint/u64_1.bin");
    assertEncoding(encodeVarint(127), "varint/u64_127.bin");
    assertEncoding(encodeVarint(128), "varint/u64_128.bin");
    assertEncoding(encodeVarint(255), "varint/u64_255.bin");
    assertEncoding(encodeVarint(256), "varint/u64_256.bin");
    assertEncoding(encodeVarint(16383), "varint/u64_16383.bin");
    assertEncoding(encodeVarint(16384), "varint/u64_16384.bin");
    assertEncoding(encodeVarint(65535), "varint/u64_65535.bin");
    assertEncoding(encodeVarint(65536), "varint/u64_65536.bin");
    assertEncoding(encodeVarint(1048576), "varint/u64_1048576.bin");
  });
});

describe("Primitive encoding", () => {
  it("encodes bool", () => {
    assertEncoding(encodeBool(false), "primitives/bool_false.bin");
    assertEncoding(encodeBool(true), "primitives/bool_true.bin");
  });

  it("encodes u8", () => {
    assertEncoding(encodeU8(0), "primitives/u8_0.bin");
    assertEncoding(encodeU8(127), "primitives/u8_127.bin");
    assertEncoding(encodeU8(255), "primitives/u8_255.bin");
  });

  it("encodes i8", () => {
    assertEncoding(encodeI8(0), "primitives/i8_0.bin");
    assertEncoding(encodeI8(-1), "primitives/i8_neg1.bin");
    assertEncoding(encodeI8(127), "primitives/i8_127.bin");
    assertEncoding(encodeI8(-128), "primitives/i8_neg128.bin");
  });

  it("encodes u16", () => {
    assertEncoding(encodeU16(0), "primitives/u16_0.bin");
    assertEncoding(encodeU16(127), "primitives/u16_127.bin");
    assertEncoding(encodeU16(128), "primitives/u16_128.bin");
    assertEncoding(encodeU16(255), "primitives/u16_255.bin");
    assertEncoding(encodeU16(256), "primitives/u16_256.bin");
    assertEncoding(encodeU16(65535), "primitives/u16_max.bin");
  });

  it("encodes i16", () => {
    assertEncoding(encodeI16(0), "primitives/i16_0.bin");
    assertEncoding(encodeI16(1), "primitives/i16_1.bin");
    assertEncoding(encodeI16(-1), "primitives/i16_neg1.bin");
    assertEncoding(encodeI16(127), "primitives/i16_127.bin");
    assertEncoding(encodeI16(128), "primitives/i16_128.bin");
    assertEncoding(encodeI16(32767), "primitives/i16_max.bin");
    assertEncoding(encodeI16(-32768), "primitives/i16_min.bin");
  });

  it("encodes u32", () => {
    assertEncoding(encodeU32(0), "primitives/u32_0.bin");
    assertEncoding(encodeU32(1), "primitives/u32_1.bin");
    assertEncoding(encodeU32(127), "primitives/u32_127.bin");
    assertEncoding(encodeU32(128), "primitives/u32_128.bin");
    assertEncoding(encodeU32(255), "primitives/u32_255.bin");
    assertEncoding(encodeU32(256), "primitives/u32_256.bin");
    assertEncoding(encodeU32(4294967295), "primitives/u32_max.bin");
  });

  it("encodes i32", () => {
    assertEncoding(encodeI32(0), "primitives/i32_0.bin");
    assertEncoding(encodeI32(1), "primitives/i32_1.bin");
    assertEncoding(encodeI32(-1), "primitives/i32_neg1.bin");
    assertEncoding(encodeI32(127), "primitives/i32_127.bin");
    assertEncoding(encodeI32(128), "primitives/i32_128.bin");
    assertEncoding(encodeI32(-128), "primitives/i32_neg128.bin");
    assertEncoding(encodeI32(2147483647), "primitives/i32_max.bin");
    assertEncoding(encodeI32(-2147483648), "primitives/i32_min.bin");
  });

  it("encodes u64", () => {
    assertEncoding(encodeU64(0n), "primitives/u64_0.bin");
    assertEncoding(encodeU64(1n), "primitives/u64_1.bin");
    assertEncoding(encodeU64(127n), "primitives/u64_127.bin");
    assertEncoding(encodeU64(128n), "primitives/u64_128.bin");
    assertEncoding(encodeU64(18446744073709551615n), "primitives/u64_max.bin");
  });

  it("encodes i64", () => {
    assertEncoding(encodeI64(0n), "primitives/i64_0.bin");
    assertEncoding(encodeI64(1n), "primitives/i64_1.bin");
    assertEncoding(encodeI64(-1n), "primitives/i64_neg1.bin");
    assertEncoding(encodeI64(15n), "primitives/i64_15.bin");
    assertEncoding(encodeI64(42n), "primitives/i64_42.bin");
    assertEncoding(encodeI64(9223372036854775807n), "primitives/i64_max.bin");
    assertEncoding(encodeI64(-9223372036854775808n), "primitives/i64_min.bin");
  });

  it("encodes f32", () => {
    assertEncoding(encodeF32(0.0), "primitives/f32_0.bin");
    assertEncoding(encodeF32(1.0), "primitives/f32_1.bin");
    assertEncoding(encodeF32(-1.0), "primitives/f32_neg1.bin");
    assertEncoding(encodeF32(1.5), "primitives/f32_1_5.bin");
    assertEncoding(encodeF32(0.25), "primitives/f32_0_25.bin");
  });

  it("encodes f64", () => {
    assertEncoding(encodeF64(0.0), "primitives/f64_0.bin");
    assertEncoding(encodeF64(1.0), "primitives/f64_1.bin");
    assertEncoding(encodeF64(-1.0), "primitives/f64_neg1.bin");
    assertEncoding(encodeF64(1.5), "primitives/f64_1_5.bin");
    assertEncoding(encodeF64(0.25), "primitives/f64_0_25.bin");
  });

  it("encodes string", () => {
    assertEncoding(encodeString(""), "primitives/string_empty.bin");
    assertEncoding(encodeString("hello world"), "primitives/string_hello.bin");
    assertEncoding(encodeString("héllo 世界 🦀"), "primitives/string_unicode.bin");
  });

  it("encodes bytes", () => {
    assertEncoding(encodeBytes(new Uint8Array([])), "primitives/bytes_empty.bin");
    assertEncoding(
      encodeBytes(new Uint8Array([0xde, 0xad, 0xbe, 0xef])),
      "primitives/bytes_deadbeef.bin",
    );
  });

  it("encodes option", () => {
    assertEncoding(encodeOption(null, encodeU32), "primitives/option_none_u32.bin");
    assertEncoding(encodeOption(42, encodeU32), "primitives/option_some_u32_42.bin");
    assertEncoding(encodeOption(null, encodeString), "primitives/option_none_string.bin");
    assertEncoding(encodeOption("hello", encodeString), "primitives/option_some_string.bin");
  });

  it("encodes vec", () => {
    assertEncoding(encodeVec([], encodeU32), "primitives/vec_empty_u32.bin");
    assertEncoding(encodeVec([1, 2, 3], encodeU32), "primitives/vec_u32_1_2_3.bin");
    assertEncoding(encodeVec([-1, 0, 1], encodeI32), "primitives/vec_i32_neg1_0_1.bin");
    assertEncoding(encodeVec(["a", "b"], encodeString), "primitives/vec_string.bin");
  });
});
