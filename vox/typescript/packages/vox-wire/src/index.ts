// Vox wire protocol: the Message envelope types, codec (on the phon engine), and
// the phon registry/schema-ids for the envelope.

export { RpcError, RpcErrorCode, decodeUserError } from "./rpc_error.ts";

export type {
  LaneId,
  RequestId,
  MethodId,
  ChannelId,
  Parity,
  ConnectionSettings,
  ProtocolError,
  LaneOpen,
  LaneAccept,
  LaneReject,
  LaneClose,
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
  BindingDirection,
  SchemaMessage,
  Ping,
  Pong,
  MessagePayload,
  Message,
  Metadata,
} from "./types.ts";

export {
  emptyMetadata,
  coerceMetadata,
  metadataKeyIsRedacted,
  metadataKeyIsNoPropagate,
  parityOdd,
  parityEven,
  connectionSettings,
  messageProtocolError,
  messagePing,
  messagePong,
  messageLaneOpen,
  messageLaneAccept,
  messageLaneReject,
  messageLaneClose,
  messageRequest,
  messageResponse,
  messageSchema,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
} from "./types.ts";

export {
  encodeMessage,
  decodeMessage,
  buildMessageDecoder,
  decodeMessageWith,
  parseSchemaClosure,
  type AuxiliaryRoot,
  type MessageDecoder,
} from "./codec.ts";

// The phon registry + schema ids for the Message envelope (generated).
export {
  registry as messageRegistry,
  schemaId as messageSchemaId,
  messageSchemaClosure,
} from "./wire.phon.generated.ts";
