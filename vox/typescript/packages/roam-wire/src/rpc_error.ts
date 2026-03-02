// RPC error types matching the Roam spec
// r[impl core.error.roam-error] - RoamError wraps call results
// r[impl call.error.protocol] - Protocol errors use discriminants 1-3

import type { DecodeResult } from "@bearcove/roam-postcard";

/** RAPACE error discriminants */
export const RpcErrorCode = {
  /** User-defined application error */
  USER: 0,
  /** r[impl call.error.unknown-method] - Method ID not recognized */
  UNKNOWN_METHOD: 1,
  /** r[impl call.error.invalid-payload] - Request payload deserialization failed */
  INVALID_PAYLOAD: 2,
  /** Call was cancelled */
  CANCELLED: 3,
} as const;

export type RpcErrorCode = (typeof RpcErrorCode)[keyof typeof RpcErrorCode];

/**
 * RPC call error with structured error information.
 *
 * r[impl core.error.call-vs-connection] - Call errors affect only this call, not the connection.
 */
export class RpcError extends Error {
  /** The error code discriminant */
  readonly code: RpcErrorCode;
  /** Raw error payload bytes (for user errors, legacy). */
  readonly payload: Uint8Array | null;
  /** Decoded user error value (set by ConnectionCaller when using descriptors). */
  readonly userError?: unknown;

  constructor(code: RpcErrorCode, payload: Uint8Array | null = null, userError?: unknown) {
    const message = RpcError.codeToMessage(code);
    super(message);
    this.name = "RpcError";
    this.code = code;
    this.payload = payload;
    this.userError = userError;
  }

  /** Check if this is a user-defined error */
  isUserError(): boolean {
    return this.code === RpcErrorCode.USER;
  }

  /** Check if this is a protocol error */
  isProtocolError(): boolean {
    return this.code !== RpcErrorCode.USER;
  }

  private static codeToMessage(code: RpcErrorCode): string {
    switch (code) {
      case RpcErrorCode.USER:
        return "Application error";
      case RpcErrorCode.UNKNOWN_METHOD:
        return "Unknown method";
      case RpcErrorCode.INVALID_PAYLOAD:
        return "Invalid payload";
      case RpcErrorCode.CANCELLED:
        return "Cancelled";
      default:
        return `Unknown error code: ${code}`;
    }
  }
}

/**
 * Decode a user error payload with a custom decoder.
 *
 * @param error The RpcError (must be a user error)
 * @param decoder Function to decode the user error type
 * @returns The decoded user error
 * @throws Error if not a user error or decoding fails
 */
export function decodeUserError<E>(
  error: RpcError,
  decoder?: (buf: Uint8Array, offset: number) => DecodeResult<E>,
): E {
  if (!error.isUserError()) {
    throw new Error("Cannot decode user error: not a user error");
  }
  if (error.userError !== undefined) {
    return error.userError as E;
  }
  if (decoder && error.payload !== null) {
    return decoder(error.payload, 0).value;
  }
  throw new Error("Cannot decode user error: no payload or decoded value");
}
