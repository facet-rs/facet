// Roam wire protocol types and utilities
//
// This package contains Roam-specific wire protocol types including
// RPC error handling that follows the RAPACE specification,
// and wire message types for the protocol.

// ============================================================================
// RPC Error Types
// ============================================================================

export { RpcError, RpcErrorCode, decodeRpcResult, decodeUserError, tryDecodeRpcResult, type RpcResult } from "./rpc_error.ts";

// ============================================================================
// Wire Types
// ============================================================================

export type {
  // Hello
  HelloV1,
  HelloV2,
  Hello,
  // MetadataValue
  MetadataValueString,
  MetadataValueBytes,
  MetadataValueU64,
  MetadataValue,
  MetadataEntry,
  // Message
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
  Message,
} from "./types.ts";

export {
  // Discriminants
  MessageDiscriminant,
  MetadataValueDiscriminant,
  HelloDiscriminant,
  // Factory functions
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
} from "./types.ts";

// ============================================================================
// Wire Schemas
// ============================================================================

export {
  HelloSchema,
  MetadataValueSchema,
  MetadataEntrySchema,
  MessageSchema,
  wireSchemaRegistry,
  getHelloSchema,
  getMetadataValueSchema,
  getMetadataEntrySchema,
  getMessageSchema,
} from "./schemas.ts";

// ============================================================================
// Wire Codec
// ============================================================================

export {
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
} from "./codec.ts";
