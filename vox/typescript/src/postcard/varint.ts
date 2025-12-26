/**
 * Varint encoding/decoding using LEB128 format.
 *
 * LEB128 (Little Endian Base 128) stores 7 bits of data per byte,
 * using the high bit as a continuation flag.
 */

export class VarintError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "VarintError";
  }
}

/**
 * Encodes an unsigned 64-bit integer using LEB128 varint encoding.
 *
 * @param value - The value to encode (must be a non-negative safe integer or bigint)
 * @returns The encoded bytes
 */
export function encodeVarint(value: bigint | number): Uint8Array {
  const result: number[] = [];
  let remaining = typeof value === "bigint" ? value : BigInt(value);

  if (remaining < 0n) {
    throw new VarintError("Cannot encode negative value as unsigned varint");
  }

  do {
    // Take the low 7 bits
    let byte = Number(remaining & 0x7fn);
    remaining >>= 7n;

    // If there are more bits, set the continuation flag
    if (remaining !== 0n) {
      byte |= 0x80;
    }

    result.push(byte);
  } while (remaining !== 0n);

  return new Uint8Array(result);
}

/**
 * A reader for decoding values from a Uint8Array.
 */
export class ByteReader {
  private data: Uint8Array;
  private offset: number;

  constructor(data: Uint8Array) {
    this.data = data;
    this.offset = 0;
  }

  get remaining(): number {
    return this.data.length - this.offset;
  }

  get position(): number {
    return this.offset;
  }

  hasRemaining(count: number = 1): boolean {
    return this.remaining >= count;
  }

  readByte(): number {
    if (this.offset >= this.data.length) {
      throw new VarintError("Unexpected end of data");
    }
    return this.data[this.offset++];
  }

  readBytes(count: number): Uint8Array {
    if (this.offset + count > this.data.length) {
      throw new VarintError("Unexpected end of data");
    }
    const result = this.data.slice(this.offset, this.offset + count);
    this.offset += count;
    return result;
  }

  skip(count: number): void {
    if (this.offset + count > this.data.length) {
      throw new VarintError("Unexpected end of data");
    }
    this.offset += count;
  }
}

/**
 * Decodes an unsigned 64-bit integer from LEB128 varint encoding.
 *
 * @param reader - The byte reader to decode from
 * @returns The decoded value as a bigint
 */
export function decodeVarint(reader: ByteReader): bigint {
  let result = 0n;
  let shift = 0n;

  while (true) {
    const byte = reader.readByte();

    // Check for overflow before shifting
    if (shift >= 64n) {
      throw new VarintError("Varint overflow");
    }

    // Add the 7 data bits to the result
    result |= BigInt(byte & 0x7f) << shift;

    // If continuation bit is not set, we're done
    if ((byte & 0x80) === 0) {
      return result;
    }

    shift += 7n;
  }
}

/**
 * Decodes an unsigned varint and returns it as a number.
 * Throws if the value doesn't fit in a safe integer.
 *
 * @param reader - The byte reader to decode from
 * @returns The decoded value as a number
 */
export function decodeVarintNumber(reader: ByteReader): number {
  const value = decodeVarint(reader);
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new VarintError("Varint value too large for number");
  }
  return Number(value);
}

/**
 * Encodes a signed 64-bit integer using zigzag encoding.
 *
 * Zigzag encoding maps signed integers to unsigned integers so that
 * small magnitude values (positive and negative) have small encodings.
 * This is done by interleaving negative and positive values:
 * 0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, 2 -> 4, etc.
 *
 * @param value - The signed value to encode
 * @returns The unsigned zigzag-encoded value
 */
export function zigzagEncode(value: bigint | number): bigint {
  const v = typeof value === "bigint" ? value : BigInt(value);
  return (v << 1n) ^ (v >> 63n);
}

/**
 * Decodes a zigzag-encoded unsigned integer back to a signed integer.
 *
 * @param value - The unsigned zigzag-encoded value
 * @returns The decoded signed value
 */
export function zigzagDecode(value: bigint): bigint {
  return (value >> 1n) ^ -(value & 1n);
}

/**
 * Encodes a signed 64-bit integer using zigzag + LEB128 varint encoding.
 *
 * @param value - The signed value to encode
 * @returns The encoded bytes
 */
export function encodeSignedVarint(value: bigint | number): Uint8Array {
  return encodeVarint(zigzagEncode(value));
}

/**
 * Decodes a signed 64-bit integer from zigzag + LEB128 varint encoding.
 *
 * @param reader - The byte reader to decode from
 * @returns The decoded signed value
 */
export function decodeSignedVarint(reader: ByteReader): bigint {
  return zigzagDecode(decodeVarint(reader));
}

/**
 * Decodes a signed varint and returns it as a number.
 * Throws if the value doesn't fit in a safe integer.
 *
 * @param reader - The byte reader to decode from
 * @returns The decoded value as a number
 */
export function decodeSignedVarintNumber(reader: ByteReader): number {
  const value = decodeSignedVarint(reader);
  if (
    value > BigInt(Number.MAX_SAFE_INTEGER) ||
    value < BigInt(Number.MIN_SAFE_INTEGER)
  ) {
    throw new VarintError("Signed varint value too large for number");
  }
  return Number(value);
}
