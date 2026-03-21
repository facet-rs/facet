// Roam wire protocol types and utilities

export {
  RpcError,
  RpcErrorCode,
  decodeUserError,
} from "./rpc_error.ts";

export type {
  ConnectionId,
  Parity,
  ConnectionSettings,
  MetadataValue,
  MetadataFlags,
  MetadataEntry,
  ProtocolError,
  ConnectionOpen,
  ConnectionAccept,
  ConnectionReject,
  ConnectionClose,
  RequestId,
  MethodId,
  ChannelId,
  CborPayload,
  RequestCall,
  RequestResponse,
  RequestCancel,
  RequestBody,
  RequestMessage,
  ChannelItem,
  ChannelClose,
  ChannelReset,
  ChannelGrantCredit,
  ChannelBody,
  ChannelMessage,
  Ping,
  Pong,
  MessagePayload,
  Message,
  Metadata,
} from "./types.ts";

export {
  ParityDiscriminant,
  MetadataValueDiscriminant,
  RequestBodyDiscriminant,
  ChannelBodyDiscriminant,
  MessagePayloadDiscriminant,
} from "./wire.generated.ts";

export {
  MetadataFlagValues,
  parityOdd,
  parityEven,
  connectionSettings,
  metadataString,
  metadataBytes,
  metadataU64,
  metadataEntry,
  messageProtocolError,
  messagePing,
  messagePong,
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

export {
  encodeMessage,
  decodeMessage,
  decodeMessageWithPlan,
} from "./codec.ts";

export {
  type Schema,
  type SchemaRegistry,
  type TypeRef,
  wireMessageSchemasCbor,
  wireMessageSchemaRegistry,
  wireMessageRootRef,
} from "./schemas.ts";
