/**
 * Postcard decoder for binary deserialization.
 */

import {
  ByteReader,
  decodeVarint,
  decodeVarintNumber,
  decodeSignedVarint,
  decodeSignedVarintNumber,
} from "./varint.js";

const textDecoder = new TextDecoder();

/**
 * A decoder for reading postcard-encoded data.
 */
export class PostcardDecoder {
  private reader: ByteReader;

  constructor(data: Uint8Array) {
    this.reader = new ByteReader(data);
  }

  /**
   * Get the number of remaining bytes.
   */
  get remaining(): number {
    return this.reader.remaining;
  }

  /**
   * Get the current position in the buffer.
   */
  get position(): number {
    return this.reader.position;
  }

  // --- Primitive Types ---

  /**
   * Decode a boolean (1 byte: 0x00 or 0x01).
   */
  bool(): boolean {
    const byte = this.reader.readByte();
    return byte !== 0x00;
  }

  /**
   * Decode a UInt8 (1 byte, raw).
   */
  u8(): number {
    return this.reader.readByte();
  }

  /**
   * Decode an Int8 (1 byte, two's complement).
   */
  i8(): number {
    const byte = this.reader.readByte();
    return byte > 127 ? byte - 256 : byte;
  }

  /**
   * Decode a UInt16 (varint).
   */
  u16(): number {
    return decodeVarintNumber(this.reader);
  }

  /**
   * Decode an Int16 (zigzag + varint).
   */
  i16(): number {
    return decodeSignedVarintNumber(this.reader);
  }

  /**
   * Decode a UInt32 (varint).
   */
  u32(): number {
    return decodeVarintNumber(this.reader);
  }

  /**
   * Decode an Int32 (zigzag + varint).
   */
  i32(): number {
    return decodeSignedVarintNumber(this.reader);
  }

  /**
   * Decode a UInt64 (varint).
   */
  u64(): bigint {
    return decodeVarint(this.reader);
  }

  /**
   * Decode a UInt64 as a number (throws if too large).
   */
  u64Number(): number {
    return decodeVarintNumber(this.reader);
  }

  /**
   * Decode an Int64 (zigzag + varint).
   */
  i64(): bigint {
    return decodeSignedVarint(this.reader);
  }

  /**
   * Decode an Int64 as a number (throws if out of range).
   */
  i64Number(): number {
    return decodeSignedVarintNumber(this.reader);
  }

  /**
   * Decode a Float32 (4 bytes, little-endian).
   */
  f32(): number {
    const bytes = this.reader.readBytes(4);
    return new DataView(bytes.buffer, bytes.byteOffset, 4).getFloat32(0, true);
  }

  /**
   * Decode a Float64 (8 bytes, little-endian).
   */
  f64(): number {
    const bytes = this.reader.readBytes(8);
    return new DataView(bytes.buffer, bytes.byteOffset, 8).getFloat64(0, true);
  }

  // --- String and Bytes ---

  /**
   * Decode a String (varint length + UTF-8 bytes).
   */
  string(): string {
    const length = decodeVarintNumber(this.reader);
    const bytes = this.reader.readBytes(length);
    return textDecoder.decode(bytes);
  }

  /**
   * Decode raw bytes (varint length + bytes).
   */
  bytes(): Uint8Array {
    const length = decodeVarintNumber(this.reader);
    return this.reader.readBytes(length);
  }

  /**
   * Read raw bytes without length prefix.
   */
  rawBytes(count: number): Uint8Array {
    return this.reader.readBytes(count);
  }

  // --- Optional ---

  /**
   * Decode an optional value.
   */
  option<T>(decode: (decoder: this) => T): T | null {
    const tag = this.reader.readByte();
    if (tag === 0x00) {
      return null;
    }
    return decode(this);
  }

  // --- Sequences ---

  /**
   * Decode an array (varint length + elements).
   */
  array<T>(decode: (decoder: this) => T): T[] {
    const length = decodeVarintNumber(this.reader);
    const result: T[] = [];
    for (let i = 0; i < length; i++) {
      result.push(decode(this));
    }
    return result;
  }

  /**
   * Decode an array of strings.
   */
  stringArray(): string[] {
    const length = decodeVarintNumber(this.reader);
    const result: string[] = [];
    for (let i = 0; i < length; i++) {
      result.push(this.string());
    }
    return result;
  }

  // --- Enums ---

  /**
   * Decode an enum discriminant (varint).
   */
  enumDiscriminant(): number {
    return decodeVarintNumber(this.reader);
  }

  // --- Helpers ---

  /**
   * Skip a number of bytes.
   */
  skip(count: number): void {
    this.reader.skip(count);
  }

  /**
   * Check if there are remaining bytes.
   */
  hasRemaining(): boolean {
    return this.reader.hasRemaining();
  }
}

/**
 * Interface for types that can be decoded from postcard format.
 */
export interface PostcardDecodable<T> {
  decode(decoder: PostcardDecoder): T;
}

/**
 * Decode a value from postcard format.
 */
export function decode<T>(data: Uint8Array, decodeFn: (decoder: PostcardDecoder) => T): T {
  const decoder = new PostcardDecoder(data);
  return decodeFn(decoder);
}
