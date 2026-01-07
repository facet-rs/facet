// Postcard serialization format for TypeScript
//
// This module provides encoding/decoding functions compatible with Rust's
// postcard format (https://postcard.rs/), which uses variable-length integers.

import { encodeVarint, decodeVarint, decodeVarintNumber } from "../binary/varint.ts";
import { concat } from "../binary/bytes.ts";

// ============================================================================
// Decode result type
// ============================================================================

export interface DecodeResult<T> {
  value: T;
  next: number; // offset after this value
}

// ============================================================================
// Primitive encoding
// ============================================================================

/** Encode a boolean (1 byte: 0x00 or 0x01). */
export function encodeBool(value: boolean): Uint8Array {
  return Uint8Array.of(value ? 1 : 0);
}

/** Decode a boolean. */
export function decodeBool(buf: Uint8Array, offset: number): DecodeResult<boolean> {
  if (offset >= buf.length) throw new Error("bool: eof");
  const byte = buf[offset];
  if (byte > 1) throw new Error(`bool: invalid value ${byte}`);
  return { value: byte === 1, next: offset + 1 };
}

/** Encode a u8 (1 byte). */
export function encodeU8(value: number): Uint8Array {
  return Uint8Array.of(value & 0xff);
}

/** Decode a u8. */
export function decodeU8(buf: Uint8Array, offset: number): DecodeResult<number> {
  if (offset >= buf.length) throw new Error("u8: eof");
  return { value: buf[offset], next: offset + 1 };
}

/** Encode an i8 (1 byte, two's complement). */
export function encodeI8(value: number): Uint8Array {
  return Uint8Array.of(value & 0xff);
}

/** Decode an i8. */
export function decodeI8(buf: Uint8Array, offset: number): DecodeResult<number> {
  if (offset >= buf.length) throw new Error("i8: eof");
  const byte = buf[offset];
  // Convert to signed
  const value = byte > 127 ? byte - 256 : byte;
  return { value, next: offset + 1 };
}

/** Encode a u16 (varint). */
export function encodeU16(value: number): Uint8Array {
  return encodeVarint(value);
}

/** Decode a u16. */
export function decodeU16(buf: Uint8Array, offset: number): DecodeResult<number> {
  const result = decodeVarintNumber(buf, offset);
  if (result.value > 0xffff) throw new Error("u16: overflow");
  return result;
}

/** Encode a u32 (varint). */
export function encodeU32(value: number): Uint8Array {
  return encodeVarint(value);
}

/** Decode a u32. */
export function decodeU32(buf: Uint8Array, offset: number): DecodeResult<number> {
  const result = decodeVarintNumber(buf, offset);
  if (result.value > 0xffffffff) throw new Error("u32: overflow");
  return result;
}

/** Encode a u64 (varint as bigint). */
export function encodeU64(value: bigint): Uint8Array {
  return encodeVarint(value);
}

/** Decode a u64. */
export function decodeU64(buf: Uint8Array, offset: number): DecodeResult<bigint> {
  return decodeVarint(buf, offset);
}

/** Zigzag encode a signed integer to unsigned. */
function zigzagEncode(n: bigint): bigint {
  // (n << 1) ^ (n >> 63) for 64-bit
  return (n << 1n) ^ (n >> 63n);
}

/** Zigzag decode an unsigned integer to signed. */
function zigzagDecode(n: bigint): bigint {
  return (n >> 1n) ^ -(n & 1n);
}

/** Encode an i16 (zigzag varint). */
export function encodeI16(value: number): Uint8Array {
  return encodeVarint(zigzagEncode(BigInt(value)));
}

/** Decode an i16. */
export function decodeI16(buf: Uint8Array, offset: number): DecodeResult<number> {
  const result = decodeVarint(buf, offset);
  const signed = zigzagDecode(result.value);
  return { value: Number(signed), next: result.next };
}

/** Encode an i32 (zigzag varint). */
export function encodeI32(value: number): Uint8Array {
  return encodeVarint(zigzagEncode(BigInt(value)));
}

/** Decode an i32. */
export function decodeI32(buf: Uint8Array, offset: number): DecodeResult<number> {
  const result = decodeVarint(buf, offset);
  const signed = zigzagDecode(result.value);
  return { value: Number(signed), next: result.next };
}

/** Encode an i64 (zigzag varint). */
export function encodeI64(value: bigint): Uint8Array {
  return encodeVarint(zigzagEncode(value));
}

/** Decode an i64. */
export function decodeI64(buf: Uint8Array, offset: number): DecodeResult<bigint> {
  const result = decodeVarint(buf, offset);
  return { value: zigzagDecode(result.value), next: result.next };
}

/** Encode an f32 (4 bytes little-endian IEEE 754). */
export function encodeF32(value: number): Uint8Array {
  const buf = new ArrayBuffer(4);
  new DataView(buf).setFloat32(0, value, true);
  return new Uint8Array(buf);
}

/** Decode an f32. */
export function decodeF32(buf: Uint8Array, offset: number): DecodeResult<number> {
  if (offset + 4 > buf.length) throw new Error("f32: eof");
  const view = new DataView(buf.buffer, buf.byteOffset + offset, 4);
  return { value: view.getFloat32(0, true), next: offset + 4 };
}

/** Encode an f64 (8 bytes little-endian IEEE 754). */
export function encodeF64(value: number): Uint8Array {
  const buf = new ArrayBuffer(8);
  new DataView(buf).setFloat64(0, value, true);
  return new Uint8Array(buf);
}

/** Decode an f64. */
export function decodeF64(buf: Uint8Array, offset: number): DecodeResult<number> {
  if (offset + 8 > buf.length) throw new Error("f64: eof");
  const view = new DataView(buf.buffer, buf.byteOffset + offset, 8);
  return { value: view.getFloat64(0, true), next: offset + 8 };
}

// ============================================================================
// String encoding
// ============================================================================

/** Encode a string (length-prefixed UTF-8). */
export function encodeString(value: string): Uint8Array {
  const bytes = new TextEncoder().encode(value);
  return concat(encodeVarint(bytes.length), bytes);
}

/** Decode a string. */
export function decodeString(buf: Uint8Array, offset: number): DecodeResult<string> {
  const len = decodeVarintNumber(buf, offset);
  const start = len.next;
  const end = start + len.value;
  if (end > buf.length) throw new Error("string: overrun");
  const s = new TextDecoder().decode(buf.subarray(start, end));
  return { value: s, next: end };
}

// ============================================================================
// Bytes encoding
// ============================================================================

/** Encode bytes (length-prefixed). */
export function encodeBytes(value: Uint8Array): Uint8Array {
  return concat(encodeVarint(value.length), value);
}

/** Decode bytes. */
export function decodeBytes(buf: Uint8Array, offset: number): DecodeResult<Uint8Array> {
  const len = decodeVarintNumber(buf, offset);
  const start = len.next;
  const end = start + len.value;
  if (end > buf.length) throw new Error("bytes: overrun");
  return { value: buf.subarray(start, end), next: end };
}

// ============================================================================
// Option encoding
// ============================================================================

/** Encode an Option<T>. */
export function encodeOption<T>(
  value: T | null,
  encodeInner: (v: T) => Uint8Array,
): Uint8Array {
  if (value === null) {
    return Uint8Array.of(0);
  } else {
    return concat(Uint8Array.of(1), encodeInner(value));
  }
}

/** Decode an Option<T>. */
export function decodeOption<T>(
  buf: Uint8Array,
  offset: number,
  decodeInner: (buf: Uint8Array, offset: number) => DecodeResult<T>,
): DecodeResult<T | null> {
  if (offset >= buf.length) throw new Error("option: eof");
  const variant = buf[offset];
  if (variant === 0) {
    return { value: null, next: offset + 1 };
  } else if (variant === 1) {
    const inner = decodeInner(buf, offset + 1);
    return { value: inner.value, next: inner.next };
  } else {
    throw new Error(`option: invalid variant ${variant}`);
  }
}

// ============================================================================
// Vec encoding
// ============================================================================

/** Encode a Vec<T>. */
export function encodeVec<T>(
  values: T[],
  encodeItem: (v: T) => Uint8Array,
): Uint8Array {
  const parts: Uint8Array[] = [encodeVarint(values.length)];
  for (const item of values) {
    parts.push(encodeItem(item));
  }
  return concat(...parts);
}

/** Decode a Vec<T>. */
export function decodeVec<T>(
  buf: Uint8Array,
  offset: number,
  decodeItem: (buf: Uint8Array, offset: number) => DecodeResult<T>,
): DecodeResult<T[]> {
  const len = decodeVarintNumber(buf, offset);
  let pos = len.next;
  const items: T[] = [];
  for (let i = 0; i < len.value; i++) {
    const item = decodeItem(buf, pos);
    items.push(item.value);
    pos = item.next;
  }
  return { value: items, next: pos };
}

// ============================================================================
// Tuple encoding (encode/decode each element in sequence)
// ============================================================================

/** Encode a 2-tuple. */
export function encodeTuple2<A, B>(
  a: A,
  b: B,
  encodeA: (v: A) => Uint8Array,
  encodeB: (v: B) => Uint8Array,
): Uint8Array {
  return concat(encodeA(a), encodeB(b));
}

/** Decode a 2-tuple. */
export function decodeTuple2<A, B>(
  buf: Uint8Array,
  offset: number,
  decodeA: (buf: Uint8Array, offset: number) => DecodeResult<A>,
  decodeB: (buf: Uint8Array, offset: number) => DecodeResult<B>,
): DecodeResult<[A, B]> {
  const a = decodeA(buf, offset);
  const b = decodeB(buf, a.next);
  return { value: [a.value, b.value], next: b.next };
}

/** Encode a 3-tuple. */
export function encodeTuple3<A, B, C>(
  a: A,
  b: B,
  c: C,
  encodeA: (v: A) => Uint8Array,
  encodeB: (v: B) => Uint8Array,
  encodeC: (v: C) => Uint8Array,
): Uint8Array {
  return concat(encodeA(a), encodeB(b), encodeC(c));
}

/** Decode a 3-tuple. */
export function decodeTuple3<A, B, C>(
  buf: Uint8Array,
  offset: number,
  decodeA: (buf: Uint8Array, offset: number) => DecodeResult<A>,
  decodeB: (buf: Uint8Array, offset: number) => DecodeResult<B>,
  decodeC: (buf: Uint8Array, offset: number) => DecodeResult<C>,
): DecodeResult<[A, B, C]> {
  const a = decodeA(buf, offset);
  const b = decodeB(buf, a.next);
  const c = decodeC(buf, b.next);
  return { value: [a.value, b.value, c.value], next: c.next };
}

// ============================================================================
// Struct encoding (encode/decode fields in order)
// ============================================================================

// Structs are encoded by encoding each field in declaration order.
// No special framing - just concatenate the encoded fields.

// ============================================================================
// Enum encoding (variant index + payload)
// ============================================================================

/** Encode an enum variant index. */
export function encodeEnumVariant(variantIndex: number): Uint8Array {
  return encodeVarint(variantIndex);
}

/** Decode an enum variant index. */
export function decodeEnumVariant(buf: Uint8Array, offset: number): DecodeResult<number> {
  return decodeVarintNumber(buf, offset);
}

// ============================================================================
// Re-export for convenience
// ============================================================================

export { encodeVarint, decodeVarint, decodeVarintNumber };
export { concat };
