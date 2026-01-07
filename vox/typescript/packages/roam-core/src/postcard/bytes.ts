import { decodeVarintNumber } from "../binary/varint.ts";

export function decodeBytes(
  buf: Uint8Array,
  offset: number,
): { value: Uint8Array; next: number } {
  const len = decodeVarintNumber(buf, offset);
  const start = len.next;
  const end = start + len.value;
  if (end > buf.length) throw new Error("bytes: overrun");
  return { value: buf.subarray(start, end), next: end };
}

