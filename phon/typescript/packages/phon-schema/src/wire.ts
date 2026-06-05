// Shared wire primitives for every phon TypeScript codec: the self-describing
// tag table, the validating little-endian `Reader`, the growable `ByteSink`, and
// the error types. Both the self-describing `Value` codec (value.ts), the schema
// parser (schema.ts), and the compact/typed engine (@bearcove/phon-engine)
// build on these so the byte-level rules live in exactly one place.
//
// Spec: docs/content/spec.md — "Wire format", "Decoding untrusted input".

// ============================================================================
// Self-describing tag bytes.
// Must match `mod tag` in rust/phon-schema/src/selfdescribing.rs exactly.
// ============================================================================

// r[impl self-describing.tag-led]
// r[impl self-describing.no-extra-kinds]
export const Tag = {
  UNIT: 0x00,
  BOOL: 0x01,
  U8: 0x02,
  U16: 0x03,
  U32: 0x04,
  U64: 0x05,
  U128: 0x06,
  I8: 0x07,
  I16: 0x08,
  I32: 0x09,
  I64: 0x0a,
  I128: 0x0b,
  F32: 0x0c,
  F64: 0x0d,
  CHAR: 0x0e,
  STRING: 0x0f,
  BYTES: 0x10,
  LIST: 0x11,
  SET: 0x12,
  MAP: 0x13,
  ARRAY: 0x14,
  TUPLE: 0x15,
  STRUCT: 0x16,
  ENUM: 0x17,
  OPTION_NONE: 0x18,
  OPTION_SOME: 0x19,
  TENSOR: 0x1a,
  DATETIME: 0x1b,
  UUID: 0x1c,
  QNAME: 0x1d,
} as const;

/// Maximum nesting depth accepted on decode (`r[validate.depth]`), matching the
/// Rust `MAX_DEPTH`. Deeper nesting is a decode error, not a stack overflow.
export const MAX_DEPTH = 128;

/// A wire-driven count of zero-sized elements cannot be bounded by the bytes
/// remaining (each element costs nothing), so it is capped at a fixed ceiling
/// instead — mirroring Rust `Reader::read_len`'s `ZST_COUNT_CAP`
/// (`r[validate.lengths]`).
export const ZST_COUNT_CAP = 1 << 24;

// ============================================================================
// Errors
// ============================================================================

/// Thrown for any malformed input. A crafted message must never crash the
/// decoder; it becomes one of these (mirrors Rust's `DecodeError`).
export class DecodeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DecodeError";
  }
}

/// Thrown when a value cannot be encoded (e.g. an integer outside u128/i128
/// range, which no wire tag can hold), or when two schemas are incompatible.
export class EncodeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "EncodeError";
  }
}

// ============================================================================
// Numeric bounds (shared by readers and sinks)
// ============================================================================

export const U64_MAX = (1n << 64n) - 1n;
export const U128_MAX = (1n << 128n) - 1n;
export const I64_MIN = -(1n << 63n);
export const I64_MAX = (1n << 63n) - 1n;
export const I128_MIN = -(1n << 127n);
export const I128_MAX = (1n << 127n) - 1n;

export const UTF8_DECODER = new TextDecoder("utf-8", { fatal: true });
export const UTF8_ENCODER = new TextEncoder();

export function hex(n: number): string {
  return `0x${n.toString(16).padStart(2, "0")}`;
}

/// Decode a lowercase/uppercase hex string to bytes (used by the conformance
/// corpus, which carries wire bytes as hex to stay JSON-safe).
export function hexToBytes(s: string): Uint8Array {
  if (s.length % 2 !== 0) throw new Error("odd-length hex string");
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(s.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function bytesToHex(b: Uint8Array): string {
  let s = "";
  for (const byte of b) s += byte.toString(16).padStart(2, "0");
  return s;
}

// ============================================================================
// Reader — a validating little-endian cursor (mirror of rust bytes::Reader)
// ============================================================================

export class Reader {
  private readonly view: DataView;
  private readonly bytes: Uint8Array;
  private pos = 0;

  constructor(bytes: Uint8Array) {
    this.bytes = bytes;
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  /// Absolute position from the start of the buffer — the reference point for
  /// alignment padding in the compact format (`skipPad`).
  position(): number {
    return this.pos;
  }

  remaining(): number {
    return this.bytes.length - this.pos;
  }

  private need(n: number): void {
    if (this.remaining() < n) {
      throw new DecodeError(`unexpected end of input: need ${n}, have ${this.remaining()}`);
    }
  }

  /// Consume zero-padding bytes until the absolute position is a multiple of
  /// `n` (compact-mode alignment, mirror of Rust `skip_pad`). `n` is a power of
  /// two; `n <= 1` is a no-op.
  skipPad(n: number): void {
    if (n <= 1) return;
    while (this.pos % n !== 0) this.readU8();
  }

  readU8(): number {
    this.need(1);
    return this.view.getUint8(this.pos++);
  }

  readU16(): bigint {
    this.need(2);
    const v = this.view.getUint16(this.pos, true);
    this.pos += 2;
    return BigInt(v);
  }

  readU32raw(): number {
    this.need(4);
    const v = this.view.getUint32(this.pos, true);
    this.pos += 4;
    return v;
  }

  readU32(): bigint {
    return BigInt(this.readU32raw());
  }

  readU64(): bigint {
    this.need(8);
    const v = this.view.getBigUint64(this.pos, true);
    this.pos += 8;
    return v;
  }

  readU128(): bigint {
    const lo = this.readU64();
    const hi = this.readU64();
    return lo | (hi << 64n);
  }

  readI8(): bigint {
    this.need(1);
    return BigInt(this.view.getInt8(this.pos++));
  }

  readI16(): bigint {
    this.need(2);
    const v = this.view.getInt16(this.pos, true);
    this.pos += 2;
    return BigInt(v);
  }

  readI32(): bigint {
    this.need(4);
    const v = this.view.getInt32(this.pos, true);
    this.pos += 4;
    return BigInt(v);
  }

  readI64(): bigint {
    this.need(8);
    const v = this.view.getBigInt64(this.pos, true);
    this.pos += 8;
    return v;
  }

  readI128(): bigint {
    const lo = this.readU64();
    const hi = this.readU64();
    const u = lo | (hi << 64n);
    return u > I128_MAX ? u - (1n << 128n) : u;
  }

  readF32(): number {
    this.need(4);
    const v = this.view.getFloat32(this.pos, true);
    this.pos += 4;
    return v;
  }

  readF64(): number {
    this.need(8);
    const v = this.view.getFloat64(this.pos, true);
    this.pos += 8;
    return v;
  }

  readBool(): boolean {
    const b = this.readU8();
    if (b === 0) return false;
    if (b === 1) return true;
    throw new DecodeError(`invalid bool byte ${hex(b)}`);
  }

  /// A u32 Unicode scalar: 0..=0x10FFFF excluding surrogates.
  // r[impl validate.text]
  readCharCode(): number {
    const n = this.readU32raw();
    if (n > 0x10ffff || (n >= 0xd800 && n <= 0xdfff)) {
      throw new DecodeError(`invalid Unicode scalar ${hex(n)}`);
    }
    return n;
  }

  /// A u32 count/length, checked so it cannot drive a read or allocation larger
  /// than the buffer allows (`r[validate.lengths]`). A zero `minElemSize` (a
  /// zero-sized element) is bounded by the fixed `ZST_COUNT_CAP` instead, since
  /// the bytes remaining cannot bound it.
  // r[impl validate.lengths]
  readLen(minElemSize: number): number {
    const count = this.readU32raw();
    const max = minElemSize === 0 ? ZST_COUNT_CAP : Math.floor(this.remaining() / minElemSize);
    if (count > max) {
      throw new DecodeError(`length ${count} exceeds ${this.remaining()} bytes remaining`);
    }
    return count;
  }

  readSlice(n: number): Uint8Array {
    this.need(n);
    const slice = this.bytes.subarray(this.pos, this.pos + n);
    this.pos += n;
    return slice;
  }

  // r[impl validate.text]
  readStr(): string {
    const len = this.readLen(1);
    const slice = this.readSlice(len);
    try {
      return UTF8_DECODER.decode(slice);
    } catch {
      throw new DecodeError("invalid UTF-8 in string");
    }
  }

  readBytes(): Uint8Array {
    const len = this.readLen(1);
    // Copy so the returned value owns its memory independent of the input buffer.
    return new Uint8Array(this.readSlice(len));
  }
}

// ============================================================================
// ByteSink — a growable little-endian writer (mirror of rust bytes::Sink)
// ============================================================================

export class ByteSink {
  private buf = new Uint8Array(64);
  private len = 0;

  /// Current byte length — the reference point for alignment padding (`padTo`).
  get length(): number {
    return this.len;
  }

  private reserve(n: number): void {
    if (this.len + n <= this.buf.length) return;
    let cap = this.buf.length * 2;
    while (cap < this.len + n) cap *= 2;
    const next = new Uint8Array(cap);
    next.set(this.buf.subarray(0, this.len));
    this.buf = next;
  }

  /// Append zero-padding bytes until the length is a multiple of `n`
  /// (compact-mode alignment, mirror of Rust `pad_to`). `n <= 1` is a no-op.
  padTo(n: number): void {
    if (n <= 1) return;
    while (this.len % n !== 0) this.u8(0);
  }

  u8(n: number): void {
    this.reserve(1);
    this.buf[this.len++] = n & 0xff;
  }

  raw(bytes: Uint8Array): void {
    this.reserve(bytes.length);
    this.buf.set(bytes, this.len);
    this.len += bytes.length;
  }

  u16(n: bigint): void {
    this.reserve(2);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.len, 2).setUint16(0, Number(BigInt.asUintN(16, n)), true);
    this.len += 2;
  }

  u32(n: number): void {
    this.reserve(4);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.len, 4).setUint32(0, n >>> 0, true);
    this.len += 4;
  }

  u64(n: bigint): void {
    this.reserve(8);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.len, 8).setBigUint64(0, BigInt.asUintN(64, n), true);
    this.len += 8;
  }

  u128(n: bigint): void {
    const u = BigInt.asUintN(128, n);
    this.u64(u & U64_MAX);
    this.u64(u >> 64n);
  }

  i16(n: bigint): void {
    this.u16(BigInt.asUintN(16, n));
  }

  i32(n: bigint): void {
    this.reserve(4);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.len, 4).setInt32(0, Number(BigInt.asIntN(32, n)), true);
    this.len += 4;
  }

  i64(n: bigint): void {
    this.u64(BigInt.asUintN(64, n));
  }

  i128(n: bigint): void {
    this.u128(BigInt.asUintN(128, n));
  }

  f32(n: number): void {
    this.reserve(4);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.len, 4).setFloat32(0, n, true);
    this.len += 4;
  }

  f64(n: number): void {
    this.reserve(8);
    new DataView(this.buf.buffer, this.buf.byteOffset + this.len, 8).setFloat64(0, n, true);
    this.len += 8;
  }

  str(s: string): void {
    const bytes = UTF8_ENCODER.encode(s);
    this.u32(bytes.length);
    this.raw(bytes);
  }

  bytes(b: Uint8Array): void {
    this.u32(b.length);
    this.raw(b);
  }

  finish(): Uint8Array {
    return this.buf.subarray(0, this.len);
  }
}
