// @bearcove/roam-runtime - TypeScript runtime for roam RPC
// This package provides the core primitives and dispatcher for roam services.

// Binary encoding primitives
export { encodeVarint, decodeVarint, decodeVarintNumber } from "./binary/varint.ts";
export { cobsEncode, cobsDecode } from "./binary/cobs.ts";
export { concat } from "./binary/bytes.ts";

// Postcard encoding/decoding - comprehensive type support
export {
  // Decode result type
  type DecodeResult,
  // Primitives
  encodeBool, decodeBool,
  encodeU8, decodeU8,
  encodeI8, decodeI8,
  encodeU16, decodeU16,
  encodeI16, decodeI16,
  encodeU32, decodeU32,
  encodeI32, decodeI32,
  encodeU64, decodeU64,
  encodeI64, decodeI64,
  encodeF32, decodeF32,
  encodeF64, decodeF64,
  // String and bytes
  encodeString, decodeString,
  encodeBytes, decodeBytes,
  // Containers
  encodeOption, decodeOption,
  encodeVec, decodeVec,
  encodeTuple2, decodeTuple2,
  encodeTuple3, decodeTuple3,
  // Enum support
  encodeEnumVariant, decodeEnumVariant,
} from "./postcard/index.ts";

// Result encoding (for server-side responses)
import { encodeResultOk, encodeResultErr } from "./postcard/result.ts";
import {
  encodeUnknownMethod,
  encodeInvalidPayload,
  RAPACE_ERROR,
} from "./postcard/rapace_error.ts";
export { encodeResultOk, encodeResultErr, encodeUnknownMethod, encodeInvalidPayload, RAPACE_ERROR };

// RPC error types (for client-side error handling)
export {
  RpcError,
  RpcErrorCode,
  decodeRpcResult,
  decodeUserError,
} from "./postcard/rpc_error.ts";

// Streaming types
export {
  type StreamId,
  Role,
  StreamError,
  StreamIdAllocator,
  StreamRegistry,
  OutgoingSender,
  ChannelReceiver,
  Push,
  Pull,
  createRawPush,
  createRawPull,
  createTypedPush,
  createTypedPull,
  type OutgoingMessage,
  type OutgoingPoll,
} from "./streaming/index.ts";

// Transport abstraction
export { type MessageTransport } from "./transport.ts";

// Connection and protocol handling
export {
  Connection,
  ConnectionError,
  type Negotiated,
  type ServiceDispatcher,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  defaultHello,
} from "./connection.ts";

// Type definitions for method handlers
export type MethodHandler<H> = (handler: H, payload: Uint8Array) => Promise<Uint8Array>;

// Generic unary dispatcher
export class UnaryDispatcher<H> {
  private methodHandlers: Map<bigint, MethodHandler<H>>;

  constructor(methodHandlers: Map<bigint, MethodHandler<H>>) {
    this.methodHandlers = methodHandlers;
  }

  async dispatch(handler: H, methodId: bigint, payload: Uint8Array): Promise<Uint8Array> {
    const methodHandler = this.methodHandlers.get(methodId);
    if (!methodHandler) {
      // r[impl unary.error.unknown-method]
      return encodeResultErr(encodeUnknownMethod());
    }

    try {
      return await methodHandler(handler, payload);
    } catch (_error) {
      // r[impl unary.error.invalid-payload]
      return encodeResultErr(encodeInvalidPayload());
    }
  }
}
