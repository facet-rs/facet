import { encodeVarint } from "./varint.ts";

export function concat(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

export function encodeString(str: string): Uint8Array {
  const bytes = new TextEncoder().encode(str);
  return concat(encodeVarint(bytes.length), bytes);
}

export function encodeBytes(bytes: Uint8Array): Uint8Array {
  return concat(encodeVarint(bytes.length), bytes);
}

