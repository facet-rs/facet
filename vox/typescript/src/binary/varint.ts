export function encodeVarint(value: number | bigint): Uint8Array {
  let remaining = typeof value === "bigint" ? value : BigInt(value);
  if (remaining < 0n) throw new Error("negative varint");
  const out: number[] = [];
  do {
    let byte = Number(remaining & 0x7fn);
    remaining >>= 7n;
    if (remaining !== 0n) byte |= 0x80;
    out.push(byte);
  } while (remaining !== 0n);
  return Uint8Array.from(out);
}

export function decodeVarint(
  buf: Uint8Array,
  offset: number,
): { value: bigint; next: number } {
  let result = 0n;
  let shift = 0n;
  let i = offset;
  while (true) {
    if (i >= buf.length) throw new Error("varint: eof");
    const byte = buf[i++];
    if (shift >= 64n) throw new Error("varint: overflow");
    result |= BigInt(byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) return { value: result, next: i };
    shift += 7n;
  }
}

export function decodeVarintNumber(
  buf: Uint8Array,
  offset: number,
): { value: number; next: number } {
  const { value, next } = decodeVarint(buf, offset);
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("varint too large");
  return { value: Number(value), next };
}

