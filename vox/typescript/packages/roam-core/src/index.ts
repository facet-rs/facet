// @bearcove/roam-runtime - TypeScript runtime for roam RPC
// This package provides the core primitives and dispatcher for roam services.

// Binary encoding primitives
export {
  encodeVarint,
  decodeVarint,
  decodeVarintNumber,
} from "../../roam-postcard/src/binary/varint.ts";
export { cobsEncode, cobsDecode } from "../../roam-postcard/src/binary/cobs.ts";
export { concat } from "../../roam-postcard/src/binary/bytes.ts";

// Postcard encoding/decoding - comprehensive type support
export {
  // Decode result type
  type DecodeResult,
  // Primitives
  encodeBool,
  decodeBool,
  encodeU8,
  decodeU8,
  encodeI8,
  decodeI8,
  encodeU16,
  decodeU16,
  encodeI16,
  decodeI16,
  encodeU32,
  decodeU32,
  encodeI32,
  decodeI32,
  encodeU64,
  decodeU64,
  encodeI64,
  decodeI64,
  encodeF32,
  decodeF32,
  encodeF64,
  decodeF64,
  // String and bytes
  encodeString,
  decodeString,
  encodeBytes,
  decodeBytes,
  // Containers
  encodeOption,
  decodeOption,
  encodeVec,
  decodeVec,
  encodeTuple2,
  decodeTuple2,
  encodeTuple3,
  decodeTuple3,
  // Enum support
  encodeEnumVariant,
  decodeEnumVariant,
} from "@bearcove/roam-postcard";

// Schema-driven encoding/decoding
export { encodeWithSchema, decodeWithSchema } from "@bearcove/roam-postcard";

// Result encoding (for server-side responses)
import { encodeResultOk, encodeResultErr } from "../../roam-postcard/src/result.ts";
import {
  encodeUnknownMethod,
  encodeInvalidPayload,
  ROAM_ERROR,
} from "../../roam-postcard/src/roam_error.ts";
export { encodeResultOk, encodeResultErr, encodeUnknownMethod, encodeInvalidPayload, ROAM_ERROR };

// RPC error types (for client-side error handling)
export { RpcError, RpcErrorCode, decodeRpcResult, decodeUserError } from "@bearcove/roam-wire";

// Wire types, schemas, and codec
export type {
  Hello,
  HelloV1,
  HelloV2,
  MetadataValue,
  MetadataValueString,
  MetadataValueBytes,
  MetadataValueU64,
  MetadataEntry,
  Message,
  MessageHello,
  MessageConnect,
  MessageAccept,
  MessageReject,
  MessageGoodbye,
  MessageRequest,
  MessageResponse,
  MessageCancel,
  MessageData,
  MessageClose,
  MessageReset,
  MessageCredit,
} from "@bearcove/roam-wire";

export {
  // Wire discriminants
  MessageDiscriminant,
  MetadataValueDiscriminant,
  HelloDiscriminant,
  // Wire factory functions
  helloV1,
  helloV2,
  metadataString,
  metadataBytes,
  metadataU64,
  messageHello,
  messageConnect,
  messageAccept,
  messageReject,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
  // Wire schemas
  HelloSchema,
  MetadataValueSchema,
  MetadataEntrySchema,
  MessageSchema,
  wireSchemaRegistry,
  // Wire codec
  encodeHello,
  decodeHello,
  encodeMetadataValue,
  decodeMetadataValue,
  encodeMetadataEntry,
  decodeMetadataEntry,
  encodeMessage,
  decodeMessage,
  encodeMessages,
  decodeMessages,
} from "@bearcove/roam-wire";

// Channel types
export {
  type ChannelId,
  Role,
  ChannelError,
  ChannelIdAllocator,
  ChannelRegistry,
  OutgoingSender,
  ChannelReceiver,
  Tx,
  Rx,
  channel,
  createServerTx,
  createServerRx,
  type OutgoingMessage,
  type OutgoingPoll,
  type TaskMessage,
  type TaskSender,
  type ChannelContext,
  // Schema types and binding
  type PrimitiveKind,
  type TxSchema,
  type RxSchema,
  type VecSchema,
  type OptionSchema,
  type MapSchema,
  type StructSchema,
  type TupleSchema,
  type EnumVariant,
  type EnumSchema,
  type RefSchema,
  type Schema,
  type SchemaRegistry,
  type MethodSchema,
  // Schema helper functions
  resolveSchema,
  findVariantByDiscriminant,
  findVariantByName,
  getVariantDiscriminant,
  getVariantFieldSchemas,
  getVariantFieldNames,
  isNewtypeVariant,
  isRefSchema,
  bindChannels,
  type BindingSerializers,
} from "./channeling/index.ts";

// Transport abstraction
export { type MessageTransport } from "./transport.ts";

// Connection and protocol handling
export {
  Connection,
  ConnectionError,
  type Negotiated,
  type ServiceDispatcher,
  type StreamingDispatcher,
  type HelloExchangeOptions,
  helloExchangeAcceptor,
  helloExchangeInitiator,
  defaultHello,
} from "./connection.ts";

// Type definitions for method handlers
export type MethodHandler<H> = (handler: H, payload: Uint8Array) => Promise<Uint8Array>;

// Generic RPC dispatcher
export class RpcDispatcher<H> {
  private methodHandlers: Map<bigint, MethodHandler<H>>;

  constructor(methodHandlers: Map<bigint, MethodHandler<H>>) {
    this.methodHandlers = methodHandlers;
  }

  async dispatch(handler: H, methodId: bigint, payload: Uint8Array): Promise<Uint8Array> {
    const methodHandler = this.methodHandlers.get(methodId);
    if (!methodHandler) {
      // r[impl call.error.unknown-method]
      return encodeResultErr(encodeUnknownMethod());
    }

    try {
      return await methodHandler(handler, payload);
    } catch (_error) {
      // r[impl call.error.invalid-payload]
      return encodeResultErr(encodeInvalidPayload());
    }
  }
}

/** @deprecated Use RpcDispatcher instead */
export const UnaryDispatcher = RpcDispatcher;
