// RPC error types matching the Roam spec
// r[impl core.error.roam-error] - RoamError wraps call results
// r[impl call.error.protocol] - Protocol errors use discriminants 1-3

import { decodeVarintNumber, type DecodeResult } from "@bearcove/roam-postcard";

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
  /** Raw error payload bytes (for user errors) */
  readonly payload: Uint8Array | null;

  constructor(code: RpcErrorCode, payload: Uint8Array | null = null) {
    const message = RpcError.codeToMessage(code);
    super(message);
    this.name = "RpcError";
    this.code = code;
    this.payload = payload;
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
 * Decode the outer Result<T, RoamError> wrapper from an RPC response.
 *
 * Returns the offset after the result discriminant if Ok,
 * or throws RpcError if Err.
 *
 * @param buf The response buffer
 * @param offset Starting offset
 * @returns The offset to start decoding the success payload
 * @throws RpcError if the response is an error
 */
export function decodeRpcResult(buf: Uint8Array, offset: number): number {
  if (buf.length === 0) {
    throw new Error(`decodeRpcResult: empty buffer (length=0), cannot decode Result discriminant`);
  }

  // Decode outer Result discriminant: 0 = Ok, 1 = Err
  const outerResult = decodeVarintNumber(buf, offset);

  if (outerResult.value === 0) {
    // Ok - return offset to success payload
    return outerResult.next;
  }

  if (outerResult.value !== 1) {
    // Invalid Result discriminant - provide context
    throw new Error(
      `decodeRpcResult: invalid outer Result discriminant: ${outerResult.value} ` +
        `(expected 0=Ok or 1=Err) at offset ${offset}\n` +
        `  Buffer (${buf.length} bytes): ${hexDump(buf, offset, 32)}`,
    );
  }

  // Err - decode the RoamError discriminant
  const errorDiscrim = decodeVarintNumber(buf, outerResult.next);
  const errorCode = errorDiscrim.value;

  // Validate RoamError discriminant
  if (errorCode < 0 || errorCode > 3) {
    throw new Error(
      `decodeRpcResult: invalid RoamError discriminant: ${errorCode} ` +
        `(expected 0=USER, 1=UNKNOWN_METHOD, 2=INVALID_PAYLOAD, 3=CANCELLED) ` +
        `at offset ${outerResult.next}\n` +
        `  Buffer (${buf.length} bytes): ${hexDump(buf, outerResult.next, 32)}`,
    );
  }

  if (errorCode === RpcErrorCode.USER) {
    // User error - payload follows
    const payload = buf.slice(errorDiscrim.next);
    throw new RpcError(errorCode, payload);
  }

  // Protocol error - no additional payload
  throw new RpcError(errorCode as RpcErrorCode);
}

/** Helper to create a hex dump of buffer bytes for error messages */
function hexDump(buf: Uint8Array, offset: number, length: number): string {
  const start = Math.max(0, offset);
  const end = Math.min(buf.length, start + length);
  const bytes: string[] = [];
  for (let i = start; i < end; i++) {
    if (i === offset) {
      bytes.push(`[${buf[i].toString(16).padStart(2, "0")}]`);
    } else {
      bytes.push(buf[i].toString(16).padStart(2, "0"));
    }
  }
  return bytes.join(" ");
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
  decoder: (buf: Uint8Array, offset: number) => DecodeResult<E>,
): E {
  if (!error.isUserError() || error.payload === null) {
    throw new Error("Cannot decode user error: not a user error");
  }
  return decoder(error.payload, 0).value;
}

/**
 * Result type for RPC response decoding.
 *
 * This follows Rust-style Result semantics instead of throwing exceptions.
 */
export type RpcResult =
  | { ok: true; offset: number }
  | { ok: false; error: RpcError };

/**
 * Try to decode the outer Result<T, RoamError> wrapper from an RPC response.
 *
 * Returns a Result type instead of throwing, following Rust-style error handling.
 *
 * @param buf The response buffer
 * @param offset Starting offset (default 0)
 * @returns RpcResult indicating success (with offset to payload) or failure (with RpcError)
 */
export function tryDecodeRpcResult(buf: Uint8Array, offset: number = 0): RpcResult {
  if (buf.length === 0) {
    return {
      ok: false,
      error: new RpcError(RpcErrorCode.INVALID_PAYLOAD),
    };
  }

  // Decode outer Result discriminant: 0 = Ok, 1 = Err
  const outerResult = decodeVarintNumber(buf, offset);

  if (outerResult.value === 0) {
    // Ok - return offset to success payload
    return { ok: true, offset: outerResult.next };
  }

  if (outerResult.value !== 1) {
    // Invalid Result discriminant
    return {
      ok: false,
      error: new RpcError(RpcErrorCode.INVALID_PAYLOAD),
    };
  }

  // Err - decode the RoamError discriminant
  const errorDiscrim = decodeVarintNumber(buf, outerResult.next);
  const errorCode = errorDiscrim.value;

  // Validate RoamError discriminant
  if (errorCode < 0 || errorCode > 3) {
    return {
      ok: false,
      error: new RpcError(RpcErrorCode.INVALID_PAYLOAD),
    };
  }

  if (errorCode === RpcErrorCode.USER) {
    // User error - payload follows
    const payload = buf.slice(errorDiscrim.next);
    return { ok: false, error: new RpcError(errorCode, payload) };
  }

  // Protocol error - no additional payload
  return { ok: false, error: new RpcError(errorCode as RpcErrorCode) };
}
