import { decodeVarintNumber } from "../binary/varint.ts";

export function decodeString(
  buf: Uint8Array,
  offset: number,
): { value: string; next: number } {
  const len = decodeVarintNumber(buf, offset);
  const start = len.next;
  const end = start + len.value;
  if (end > buf.length) throw new Error("string: overrun");
  const s = new TextDecoder().decode(buf.subarray(start, end));
  return { value: s, next: end };
}

