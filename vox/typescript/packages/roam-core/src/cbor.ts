// Minimal CBOR codec for SchemaMessagePayload.
//
// Decoder: handles maps, arrays, text strings, byte strings, unsigned
// integers, booleans, and null.
//
// Encoder: produces facet-cbor-compatible CBOR for sending schema payloads.

// r[impl schema.principles.cbor]

// ============================================================================
// CBOR Encoding
// ============================================================================

function cborMajor(major: number, value: number): Uint8Array {
  if (value < 24) return Uint8Array.of((major << 5) | value);
  if (value <= 0xff) return Uint8Array.of((major << 5) | 24, value);
  if (value <= 0xffff)
    return Uint8Array.of((major << 5) | 25, value >> 8, value & 0xff);
  return Uint8Array.of(
    (major << 5) | 26,
    (value >>> 24) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 8) & 0xff,
    value & 0xff,
  );
}

function cborMajorBig(major: number, value: bigint): Uint8Array {
  if (value < 24n) return Uint8Array.of((major << 5) | Number(value));
  if (value <= 0xffn) return Uint8Array.of((major << 5) | 24, Number(value));
  if (value <= 0xffffn) {
    return Uint8Array.of((major << 5) | 25, Number(value >> 8n), Number(value & 0xffn));
  }
  if (value <= 0xffffffffn) {
    return Uint8Array.of(
      (major << 5) | 26,
      Number((value >> 24n) & 0xffn),
      Number((value >> 16n) & 0xffn),
      Number((value >> 8n) & 0xffn),
      Number(value & 0xffn),
    );
  }
  // 8-byte uint
  const out = new Uint8Array(9);
  out[0] = (major << 5) | 27;
  let v = value;
  for (let i = 8; i >= 1; i--) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

function cborConcat(chunks: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const c of chunks) total += c.length;
  const out = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) { out.set(c, off); off += c.length; }
  return out;
}

export function cborUint(value: number): Uint8Array {
  return cborMajor(0, value);
}

/** Encode a u64 bigint as CBOR unsigned integer (for method_id etc). */
export function cborUint64(value: bigint): Uint8Array {
  return cborMajorBig(0, value);
}

export function cborNull(): Uint8Array {
  return Uint8Array.of(0xf6);
}

export function cborBool(value: boolean): Uint8Array {
  return Uint8Array.of(value ? 0xf5 : 0xf4);
}

export function cborText(value: string): Uint8Array {
  const enc = new TextEncoder().encode(value);
  return cborConcat([cborMajor(3, enc.length), enc]);
}

/** Encode a 1-element tuple struct (TupleStruct in facet-cbor → CBOR array). */
export function cborTupleStruct1(inner: Uint8Array): Uint8Array {
  return cborConcat([cborMajor(4, 1), inner]);
}

/** Encode a CBOR array. */
export function cborArray(items: Uint8Array[]): Uint8Array {
  return cborConcat([cborMajor(4, items.length), ...items]);
}

/** Encode a CBOR map with string keys. */
export function cborMap(entries: Array<[string, Uint8Array]>): Uint8Array {
  const chunks: Uint8Array[] = [cborMajor(5, entries.length)];
  for (const [k, v] of entries) {
    chunks.push(cborText(k), v);
  }
  return cborConcat(chunks);
}

/** Encode an empty CBOR map (unit/empty struct in facet-cbor). */
export function cborEmptyMap(): Uint8Array {
  return Uint8Array.of(0xa0);
}

/** Encode a facet-cbor enum: 1-entry map {variant_name: payload}. */
export function cborEnum(variantName: string, payload: Uint8Array): Uint8Array {
  return cborMap([[variantName, payload]]);
}


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
