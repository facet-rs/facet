import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import {
  decodeBool,
  decodeBytes,
  decodeEnumVariant,
  decodeF32,
  decodeF64,
  decodeI8,
  decodeI16,
  decodeI32,
  decodeI64,
  decodeOption,
  decodeString,
  decodeTuple2,
  decodeU8,
  decodeU16,
  decodeU32,
  decodeU64,
  decodeVec,
  encodeBool,
  encodeBytes,
  encodeEnumVariant,
  encodeF32,
  encodeF64,
  encodeI8,
  encodeI16,
  encodeI32,
  encodeI64,
  encodeOption,
  encodeString,
  encodeTuple2,
  encodeU8,
  encodeU16,
  encodeU32,
  encodeU64,
  encodeVarint,
  encodeVec,
  type DecodeResult,
} from "@bearcove/vox-postcard";

const __dirname = dirname(fileURLToPath(import.meta.url));

function loadGoldenVector(path: string): Uint8Array {
  const projectRoot = join(__dirname, "..", "..", "..", "..");
  const vectorPath = join(projectRoot, "test-fixtures", "golden-vectors", path);
  return new Uint8Array(readFileSync(vectorPath));
}

function assertEncoding(encoded: Uint8Array, vectorPath: string) {
  const expected = loadGoldenVector(vectorPath);
  expect(Array.from(encoded)).toEqual(Array.from(expected));
}

function assertDecode<T>(
  vectorPath: string,
  decode: (buf: Uint8Array, offset: number) => DecodeResult<T>,
  expected: T,
) {
  const bytes = loadGoldenVector(vectorPath);
  const decoded = decode(bytes, 0);
  expect(decoded.value, `decode ${vectorPath}`).toEqual(expected);
  expect(decoded.next, `decode length ${vectorPath}`).toBe(bytes.length);
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

describe("Primitive decode from Rust golden vectors", () => {
  it("decodes bool", () => {
    assertDecode("primitives/bool_false.bin", decodeBool, false);
    assertDecode("primitives/bool_true.bin", decodeBool, true);
  });

  it("decodes u8", () => {
    assertDecode("primitives/u8_0.bin", decodeU8, 0);
    assertDecode("primitives/u8_127.bin", decodeU8, 127);
    assertDecode("primitives/u8_255.bin", decodeU8, 255);
  });

  it("decodes i8", () => {
    assertDecode("primitives/i8_0.bin", decodeI8, 0);
    assertDecode("primitives/i8_neg1.bin", decodeI8, -1);
    assertDecode("primitives/i8_127.bin", decodeI8, 127);
    assertDecode("primitives/i8_neg128.bin", decodeI8, -128);
  });

  it("decodes u16", () => {
    assertDecode("primitives/u16_0.bin", decodeU16, 0);
    assertDecode("primitives/u16_127.bin", decodeU16, 127);
    assertDecode("primitives/u16_128.bin", decodeU16, 128);
    assertDecode("primitives/u16_255.bin", decodeU16, 255);
    assertDecode("primitives/u16_256.bin", decodeU16, 256);
    assertDecode("primitives/u16_max.bin", decodeU16, 65535);
  });

  it("decodes i16", () => {
    assertDecode("primitives/i16_0.bin", decodeI16, 0);
    assertDecode("primitives/i16_1.bin", decodeI16, 1);
    assertDecode("primitives/i16_neg1.bin", decodeI16, -1);
    assertDecode("primitives/i16_max.bin", decodeI16, 32767);
    assertDecode("primitives/i16_min.bin", decodeI16, -32768);
  });

  it("decodes u32", () => {
    assertDecode("primitives/u32_0.bin", decodeU32, 0);
    assertDecode("primitives/u32_1.bin", decodeU32, 1);
    assertDecode("primitives/u32_127.bin", decodeU32, 127);
    assertDecode("primitives/u32_128.bin", decodeU32, 128);
    assertDecode("primitives/u32_max.bin", decodeU32, 4294967295);
  });

  it("decodes i32", () => {
    assertDecode("primitives/i32_0.bin", decodeI32, 0);
    assertDecode("primitives/i32_1.bin", decodeI32, 1);
    assertDecode("primitives/i32_neg1.bin", decodeI32, -1);
    assertDecode("primitives/i32_max.bin", decodeI32, 2147483647);
    assertDecode("primitives/i32_min.bin", decodeI32, -2147483648);
  });

  it("decodes u64", () => {
    assertDecode("primitives/u64_0.bin", decodeU64, 0n);
    assertDecode("primitives/u64_1.bin", decodeU64, 1n);
    assertDecode("primitives/u64_127.bin", decodeU64, 127n);
    assertDecode("primitives/u64_128.bin", decodeU64, 128n);
    assertDecode("primitives/u64_max.bin", decodeU64, 18446744073709551615n);
  });

  it("decodes i64", () => {
    assertDecode("primitives/i64_0.bin", decodeI64, 0n);
    assertDecode("primitives/i64_1.bin", decodeI64, 1n);
    assertDecode("primitives/i64_neg1.bin", decodeI64, -1n);
    assertDecode("primitives/i64_42.bin", decodeI64, 42n);
    assertDecode("primitives/i64_max.bin", decodeI64, 9223372036854775807n);
    assertDecode("primitives/i64_min.bin", decodeI64, -9223372036854775808n);
  });

  it("decodes f32", () => {
    assertDecode("primitives/f32_0.bin", decodeF32, 0.0);
    assertDecode("primitives/f32_1.bin", decodeF32, 1.0);
    assertDecode("primitives/f32_neg1.bin", decodeF32, -1.0);
    assertDecode("primitives/f32_1_5.bin", decodeF32, 1.5);
    assertDecode("primitives/f32_0_25.bin", decodeF32, 0.25);
  });

  it("decodes f64", () => {
    assertDecode("primitives/f64_0.bin", decodeF64, 0.0);
    assertDecode("primitives/f64_1.bin", decodeF64, 1.0);
    assertDecode("primitives/f64_neg1.bin", decodeF64, -1.0);
    assertDecode("primitives/f64_1_5.bin", decodeF64, 1.5);
    assertDecode("primitives/f64_0_25.bin", decodeF64, 0.25);
  });

  it("decodes string", () => {
    assertDecode("primitives/string_empty.bin", decodeString, "");
    assertDecode("primitives/string_hello.bin", decodeString, "hello world");
    assertDecode("primitives/string_unicode.bin", decodeString, "héllo 世界 🦀");
  });

  it("decodes bytes", () => {
    assertDecode("primitives/bytes_empty.bin", decodeBytes, new Uint8Array([]));
    assertDecode("primitives/bytes_deadbeef.bin", decodeBytes, new Uint8Array([0xde, 0xad, 0xbe, 0xef]));
  });

  it("decodes option", () => {
    assertDecode("primitives/option_none_u32.bin", (buf, offset) => decodeOption(buf, offset, decodeU32), null);
    assertDecode("primitives/option_some_u32_42.bin", (buf, offset) => decodeOption(buf, offset, decodeU32), 42);
    assertDecode(
      "primitives/option_none_string.bin",
      (buf, offset) => decodeOption(buf, offset, decodeString),
      null,
    );
    assertDecode(
      "primitives/option_some_string.bin",
      (buf, offset) => decodeOption(buf, offset, decodeString),
      "hello",
    );
  });

  it("decodes vec", () => {
    assertDecode("primitives/vec_empty_u32.bin", (buf, offset) => decodeVec(buf, offset, decodeU32), []);
    assertDecode("primitives/vec_u32_1_2_3.bin", (buf, offset) => decodeVec(buf, offset, decodeU32), [1, 2, 3]);
    assertDecode(
      "primitives/vec_i32_neg1_0_1.bin",
      (buf, offset) => decodeVec(buf, offset, decodeI32),
      [-1, 0, 1],
    );
    assertDecode(
      "primitives/vec_string.bin",
      (buf, offset) => decodeVec(buf, offset, decodeString),
      ["a", "b"],
    );
  });
});

describe("Cross-language helper golden vectors", () => {
  it("round-trips tuple (u32, string)", () => {
    assertEncoding(
      encodeTuple2(42, "hello", encodeU32, encodeString),
      "composite/tuple_u32_string.bin",
    );
    assertDecode(
      "composite/tuple_u32_string.bin",
      (buf, offset) => decodeTuple2(buf, offset, decodeU32, decodeString),
      [42, "hello"],
    );
  });

  it("round-trips tuple (bool, i64)", () => {
    assertEncoding(
      encodeTuple2(true, -99n, encodeBool, encodeI64),
      "composite/tuple_bool_i64.bin",
    );
    assertDecode(
      "composite/tuple_bool_i64.bin",
      (buf, offset) => decodeTuple2(buf, offset, decodeBool, decodeI64),
      [true, -99n],
    );
  });

  it("encodes unit enum variant indices", () => {
    assertEncoding(encodeEnumVariant(0), "composite/enum_red.bin");
    assertEncoding(encodeEnumVariant(1), "composite/enum_green.bin");
    assertEncoding(encodeEnumVariant(2), "composite/enum_blue.bin");
  });

  it("decodes unit enum variant indices", () => {
    assertDecode("composite/enum_red.bin", decodeEnumVariant, 0);
    assertDecode("composite/enum_green.bin", decodeEnumVariant, 1);
    assertDecode("composite/enum_blue.bin", decodeEnumVariant, 2);
  });
});
