// @bearcove/rapace-runtime - TypeScript runtime for Rapace RPC
// This package provides the core primitives and dispatcher for Rapace services.

// Binary encoding primitives
export { encodeVarint, decodeVarint, decodeVarintNumber } from "./binary/varint.ts";
export { cobsEncode, cobsDecode } from "./binary/cobs.ts";
export { concat, encodeString, encodeBytes } from "./binary/bytes.ts";

// Postcard encoding/decoding
export { decodeString } from "./postcard/string.ts";
export { decodeBytes } from "./postcard/bytes.ts";
import { encodeResultOk, encodeResultErr } from "./postcard/result.ts";
import { encodeUnknownMethod, encodeInvalidPayload, RAPACE_ERROR } from "./postcard/rapace_error.ts";
export { encodeResultOk, encodeResultErr, encodeUnknownMethod, encodeInvalidPayload, RAPACE_ERROR };

// Type definitions for method handlers
export type MethodHandler<H> = (
  handler: H,
  payload: Uint8Array
) => Promise<Uint8Array>;

// Generic unary dispatcher
export class UnaryDispatcher<H> {
  private methodHandlers: Map<bigint, MethodHandler<H>>;

  constructor(methodHandlers: Map<bigint, MethodHandler<H>>) {
    this.methodHandlers = methodHandlers;
  }

  async dispatch(
    handler: H,
    methodId: bigint,
    payload: Uint8Array
  ): Promise<Uint8Array> {
    const methodHandler = this.methodHandlers.get(methodId);
    if (!methodHandler) {
      // r[impl unary.error.unknown-method]
      return encodeResultErr(encodeUnknownMethod());
    }

    try {
      return await methodHandler(handler, payload);
    } catch (error) {
      // r[impl unary.error.invalid-payload]
      return encodeResultErr(encodeInvalidPayload());
    }
  }
}
