// Minimal CBOR decoder for parsing SchemaMessagePayload.
//
// Only implements the subset needed to decode facet-cbor-serialized
// schema messages: maps, arrays, text strings, byte strings, unsigned
// integers, booleans, and null.

// r[impl schema.principles.cbor]

export type CborValue =
  | number
  | string
  | boolean
  | null
  | Uint8Array
  | CborValue[]
  | CborMap;

export type CborMap = { [key: string]: CborValue };

interface DecodeResult {
  value: CborValue;
  next: number;
}

export function decodeCbor(buf: Uint8Array, offset = 0): DecodeResult {
  if (offset >= buf.length) {
    throw new Error(`CBOR: unexpected end of input at offset ${offset}`);
  }

  const initial = buf[offset];
  const major = initial >> 5;
  const additional = initial & 0x1f;

  switch (major) {
    case 0: // unsigned integer
      return decodeUint(buf, offset);
    case 1: { // negative integer
      const { value, next } = decodeUint(buf, offset);
      return { value: -(value + 1), next };
    }
    case 2: // byte string
      return decodeByteString(buf, offset);
    case 3: // text string
      return decodeTextString(buf, offset);
    case 4: // array
      return decodeArray(buf, offset);
    case 5: // map
      return decodeMap(buf, offset);
    case 7: // simple/float
      switch (additional) {
        case 20:
          return { value: false, next: offset + 1 };
        case 21:
          return { value: true, next: offset + 1 };
        case 22:
          return { value: null, next: offset + 1 };
        default:
          throw new Error(`CBOR: unsupported simple value ${additional} at offset ${offset}`);
      }
    default:
      throw new Error(`CBOR: unsupported major type ${major} at offset ${offset}`);
  }
}

function decodeArgument(buf: Uint8Array, offset: number): { value: number; next: number } {
  const additional = buf[offset] & 0x1f;
  if (additional < 24) {
    return { value: additional, next: offset + 1 };
  }
  if (additional === 24) {
    return { value: buf[offset + 1], next: offset + 2 };
  }
  if (additional === 25) {
    const value = (buf[offset + 1] << 8) | buf[offset + 2];
    return { value, next: offset + 3 };
  }
  if (additional === 26) {
    const value =
      (buf[offset + 1] << 24) |
      (buf[offset + 2] << 16) |
      (buf[offset + 3] << 8) |
      buf[offset + 4];
    return { value: value >>> 0, next: offset + 5 };
  }
  if (additional === 27) {
    // 8-byte integer — for method_id (u64)
    const view = new DataView(buf.buffer, buf.byteOffset + offset + 1, 8);
    const hi = view.getUint32(0);
    const lo = view.getUint32(4);
    // Return as number if it fits, otherwise we'd need BigInt
    // For schema purposes, method_id fits in Number.MAX_SAFE_INTEGER territory
    const value = hi * 0x100000000 + lo;
    return { value, next: offset + 9 };
  }
  throw new Error(`CBOR: unsupported additional info ${additional} at offset ${offset}`);
}

function decodeUint(buf: Uint8Array, offset: number): { value: number; next: number } {
  return decodeArgument(buf, offset);
}

function decodeByteString(buf: Uint8Array, offset: number): { value: Uint8Array; next: number } {
  const { value: length, next: start } = decodeArgument(buf, offset);
  const end = start + length;
  return { value: buf.subarray(start, end), next: end };
}

function decodeTextString(buf: Uint8Array, offset: number): { value: string; next: number } {
  const { value: length, next: start } = decodeArgument(buf, offset);
  const end = start + length;
  const text = new TextDecoder().decode(buf.subarray(start, end));
  return { value: text, next: end };
}

function decodeArray(buf: Uint8Array, offset: number): { value: CborValue[]; next: number } {
  const { value: count, next: start } = decodeArgument(buf, offset);
  const items: CborValue[] = [];
  let pos = start;
  for (let i = 0; i < count; i++) {
    const result = decodeCbor(buf, pos);
    items.push(result.value);
    pos = result.next;
  }
  return { value: items, next: pos };
}

function decodeMap(buf: Uint8Array, offset: number): { value: CborMap; next: number } {
  const { value: count, next: start } = decodeArgument(buf, offset);
  const map: CborMap = {};
  let pos = start;
  for (let i = 0; i < count; i++) {
    const key = decodeCbor(buf, pos);
    if (typeof key.value !== "string") {
      throw new Error(`CBOR: map key must be string, got ${typeof key.value} at offset ${pos}`);
    }
    const val = decodeCbor(buf, key.next);
    map[key.value as string] = val.value;
    pos = val.next;
  }
  return { value: map, next: pos };
}
