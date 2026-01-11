// TODO: move to roam-wire

import { encodeVarint } from "./binary/varint.ts";

export const ROAM_ERROR = {
  USER: 0,
  UNKNOWN_METHOD: 1,
  INVALID_PAYLOAD: 2,
  CANCELLED: 3,
} as const;

export function encodeUnknownMethod(): Uint8Array {
  return encodeVarint(ROAM_ERROR.UNKNOWN_METHOD);
}

export function encodeInvalidPayload(): Uint8Array {
  return encodeVarint(ROAM_ERROR.INVALID_PAYLOAD);
}
