import { encodeVarint } from "../binary/varint.ts";
import { concat } from "../binary/bytes.ts";

export function encodeResultOk(okBytes: Uint8Array): Uint8Array {
  return concat(encodeVarint(0), okBytes);
}

export function encodeResultErr(errBytes: Uint8Array): Uint8Array {
  return concat(encodeVarint(1), errBytes);
}

