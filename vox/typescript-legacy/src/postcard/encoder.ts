/**
 * Postcard encoder for binary serialization.
 *
 * Postcard is a compact binary format used by the rapace protocol.
 * Encoding rules:
 * - Booleans: 1 byte (0x00 or 0x01)
 * - Int8/UInt8: Raw 1 byte
 * - Signed integers (Int16-Int64): Zigzag encode + LEB128 varint
 * - Unsigned integers (UInt16-UInt64): Direct LEB128 varint
 * - Floats/Doubles: Little-endian IEEE 754 (4/8 bytes raw)
 * - Strings: Varint length (bytes) + UTF-8 payload
 * - Byte arrays: Varint length + raw bytes
 * - Options: 1 byte tag (0x00 = None, 0x01 = Some) + optional payload
 * - Sequences: Varint count + elements in sequence
 * - Structs: Field values in order, no delimiters
 * - Enums: Varint discriminant + variant payload
 */

import { encodeVarint, encodeSignedVarint } from "./varint.js";

const textEncoder = new TextEncoder();

/**
 * A buffer for building postcard-encoded data.
 */
export class PostcardEncoder {
  private buffer: number[] = [];

  /**
   * Get the encoded bytes.
   */
  get bytes(): Uint8Array {
    return new Uint8Array(this.buffer);
  }

  /**
   * Reset the encoder for reuse.
   */
  reset(): void {
    this.buffer.length = 0;
  }

  // --- Primitive Types ---

  /**
   * Encode a boolean (1 byte: 0x00 or 0x01).
   */
  bool(value: boolean): this {
    this.buffer.push(value ? 0x01 : 0x00);
    return this;
  }

  /**
   * Encode a UInt8 (1 byte, raw).
   */
  u8(value: number): this {
    this.buffer.push(value & 0xff);
    return this;
  }

  /**
   * Encode an Int8 (1 byte, two's complement).
   */
  i8(value: number): this {
    this.buffer.push(value < 0 ? value + 256 : value);
    return this;
  }

  /**
   * Encode a UInt16 (varint).
   */
  u16(value: number): this {
    this.appendVarint(BigInt(value));
    return this;
  }

  /**
   * Encode an Int16 (zigzag + varint).
   */
  i16(value: number): this {
    this.appendSignedVarint(BigInt(value));
    return this;
  }

  /**
   * Encode a UInt32 (varint).
   */
  u32(value: number): this {
    this.appendVarint(BigInt(value));
    return this;
  }

  /**
   * Encode an Int32 (zigzag + varint).
   */
  i32(value: number): this {
    this.appendSignedVarint(BigInt(value));
    return this;
  }

  /**
   * Encode a UInt64 (varint).
   */
  u64(value: bigint | number): this {
    this.appendVarint(typeof value === "bigint" ? value : BigInt(value));
    return this;
  }

  /**
   * Encode an Int64 (zigzag + varint).
   */
  i64(value: bigint | number): this {
    this.appendSignedVarint(typeof value === "bigint" ? value : BigInt(value));
    return this;
  }

  /**
   * Encode a Float32 (4 bytes, little-endian).
   */
  f32(value: number): this {
    const buf = new ArrayBuffer(4);
    new DataView(buf).setFloat32(0, value, true);
    const bytes = new Uint8Array(buf);
    for (const b of bytes) {
      this.buffer.push(b);
    }
    return this;
  }

  /**
   * Encode a Float64 (8 bytes, little-endian).
   */
  f64(value: number): this {
    const buf = new ArrayBuffer(8);
    new DataView(buf).setFloat64(0, value, true);
    const bytes = new Uint8Array(buf);
    for (const b of bytes) {
      this.buffer.push(b);
    }
    return this;
  }

  // --- String and Bytes ---

  /**
   * Encode a String (varint length + UTF-8 bytes).
   */
  string(value: string): this {
    const utf8 = textEncoder.encode(value);
    this.appendVarint(BigInt(utf8.length));
    for (const b of utf8) {
      this.buffer.push(b);
    }
    return this;
  }

  /**
   * Encode raw bytes (varint length + bytes).
   */
  byteArray(value: Uint8Array): this {
    this.appendVarint(BigInt(value.length));
    for (const b of value) {
      this.buffer.push(b);
    }
    return this;
  }

  /**
   * Encode raw bytes without length prefix.
   */
  rawBytes(value: Uint8Array): this {
    for (const b of value) {
      this.buffer.push(b);
    }
    return this;
  }

  // --- Optional ---

  /**
   * Encode an optional value.
   */
  option<T>(value: T | null | undefined, encode: (encoder: this, v: T) => void): this {
    if (value === null || value === undefined) {
      this.buffer.push(0x00); // None
    } else {
      this.buffer.push(0x01); // Some
      encode(this, value);
    }
    return this;
  }

  // --- Sequences ---

  /**
   * Encode an array (varint length + elements).
   */
  array<T>(values: T[], encode: (encoder: this, v: T) => void): this {
    this.appendVarint(BigInt(values.length));
    for (const value of values) {
      encode(this, value);
    }
    return this;
  }

  /**
   * Encode an array of strings.
   */
  stringArray(values: string[]): this {
    this.appendVarint(BigInt(values.length));
    for (const value of values) {
      this.string(value);
    }
    return this;
  }

  // --- Enums ---

  /**
   * Encode an enum discriminant (varint).
   */
  enumDiscriminant(discriminant: number): this {
    this.appendVarint(BigInt(discriminant));
    return this;
  }

  // --- Helpers ---

  private appendVarint(value: bigint): void {
    const encoded = encodeVarint(value);
    for (const b of encoded) {
      this.buffer.push(b);
    }
  }

  private appendSignedVarint(value: bigint): void {
    const encoded = encodeSignedVarint(value);
    for (const b of encoded) {
      this.buffer.push(b);
    }
  }
}

/**
 * Interface for types that can be encoded to postcard format.
 */
export interface PostcardEncodable {
  encode(encoder: PostcardEncoder): void;
}

/**
 * Encode a value to postcard format.
 */
export function encode(value: PostcardEncodable): Uint8Array {
  const encoder = new PostcardEncoder();
  value.encode(encoder);
  return encoder.bytes;
}
