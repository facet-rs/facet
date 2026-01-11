// Roam wire protocol types and utilities
//
// This package contains Roam-specific wire protocol types including
// RPC error handling that follows the RAPACE specification,
// and wire message types for the protocol.

// ============================================================================
// RPC Error Types
// ============================================================================

export { RpcError, RpcErrorCode, decodeRpcResult, decodeUserError } from "./rpc_error.ts";

// ============================================================================
// Wire Types
// ============================================================================

export type {
  // Hello
  HelloV1,
  Hello,
  // MetadataValue
  MetadataValueString,
  MetadataValueBytes,
  MetadataValueU64,
  MetadataValue,
  MetadataEntry,
  // Message
  MessageHello,
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
  metadataString,
  metadataBytes,
  metadataU64,
  messageHello,
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
