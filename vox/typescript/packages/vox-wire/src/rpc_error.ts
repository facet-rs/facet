// RPC error types matching the Vox spec
// r[impl rpc.error.scope]
// r[impl rpc.fallible]
// r[impl rpc.fallible.vox-error]
// r[impl rpc.fallible.vox-error.outcome]

/** RPC error discriminants */
export const RpcErrorCode = {
  /** r[impl rpc.fallible.vox-error] User-defined application error */
  USER: 0,
  /** r[impl rpc.unknown-method] - Method ID not recognized */
  UNKNOWN_METHOD: 1,
  /** r[impl rpc.error.scope] Request payload deserialization failed */
  INVALID_PAYLOAD: 2,
  /** r[impl rpc.fallible.vox-error] Call was cancelled */
  CANCELLED: 3,
  /** Call made no request-scoped progress before its idle timeout */
  TIMED_OUT: 4,
  /** r[impl rpc.fallible.vox-error.outcome] Runtime could not determine the call outcome */
  INDETERMINATE: 5,
} as const;

export type RpcErrorCode = (typeof RpcErrorCode)[keyof typeof RpcErrorCode];

/**
 * RPC call error with structured error information.
 *
 * r[impl rpc.error.scope]
 * r[impl rpc.fallible]
 * r[impl rpc.fallible.caller-signature]
 * r[impl rpc.fallible.vox-error]
 */
export class RpcError extends Error {
  /** The error code discriminant */
  readonly code: RpcErrorCode;
  /** Raw user error payload bytes when the caller has not decoded them yet. */
  readonly payload: Uint8Array | null;
  /** Decoded user error value when the caller has already interpreted the payload. */
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
      case RpcErrorCode.TIMED_OUT:
        return "Timed out";
      case RpcErrorCode.INDETERMINATE:
        return "Indeterminate";
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
  decoder?: (buf: Uint8Array, offset: number) => { value: E },
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
