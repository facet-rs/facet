// Roam wire protocol types for TypeScript.
//
// These types match the Rust definitions in roam-wire/src/lib.rs exactly.
// The discriminants match the #[repr(u8)] values used in Rust.

// ============================================================================
// Hello Message
// ============================================================================

/**
 * Hello message variant V3 (supports virtual connections and metadata flags).
 */
export interface HelloV3 {
  tag: "V3";
  maxPayloadSize: number;
  initialChannelCredit: number;
}

/**
 * Hello message for handshake.
 *
 * r[impl message.hello.structure]
 */
export type Hello = HelloV3;

// ============================================================================
// MetadataValue
// ============================================================================

/**
 * String metadata value.
 */
export interface MetadataValueString {
  tag: "String";
  value: string;
}

/**
 * Bytes metadata value.
 */
export interface MetadataValueBytes {
  tag: "Bytes";
  value: Uint8Array;
}

/**
 * U64 metadata value.
 */
export interface MetadataValueU64 {
  tag: "U64";
  value: bigint;
}

/**
 * Metadata value.
 *
 * r[impl call.metadata.type]
 */
export type MetadataValue = MetadataValueString | MetadataValueBytes | MetadataValueU64;

/**
 * Metadata flags.
 *
 * r[impl call.metadata.flags]
 */
export const MetadataFlags = {
  /** No special handling. */
  NONE: 0n,
  /** Value MUST NOT be logged, traced, or included in error messages. */
  SENSITIVE: 1n << 0n,
  /** Value MUST NOT be forwarded to downstream calls. */
  NO_PROPAGATE: 1n << 1n,
} as const;

/**
 * A metadata entry is a (key, value, flags) triple.
 *
 * r[impl call.metadata.type] - Metadata is a list of entries.
 * r[impl call.metadata.flags] - Each entry includes flags for handling behavior.
 */
export type MetadataEntry = [string, MetadataValue, bigint];

// ============================================================================
// Message
// ============================================================================

/**
 * Hello message (discriminant = 0).
 * Link control - no conn_id.
 */
export interface MessageHello {
  tag: "Hello";
  value: Hello;
}

/**
 * Connect message (discriminant = 1).
 * Virtual connection control - no conn_id.
 * r[impl message.connect.initiate] - Request a new virtual connection.
 */
export interface MessageConnect {
  tag: "Connect";
  requestId: bigint;
  metadata: MetadataEntry[];
}

/**
 * Accept message (discriminant = 2).
 * Virtual connection control - no conn_id.
 * r[impl message.accept.response] - Accept a virtual connection request.
 */
export interface MessageAccept {
  tag: "Accept";
  requestId: bigint;
  connId: bigint;
  metadata: MetadataEntry[];
}

/**
 * Reject message (discriminant = 3).
 * Virtual connection control - no conn_id.
 * r[impl message.reject.response] - Reject a virtual connection request.
 */
export interface MessageReject {
  tag: "Reject";
  requestId: bigint;
  reason: string;
  metadata: MetadataEntry[];
}

/**
 * Goodbye message (discriminant = 4).
 * Connection control - scoped to conn_id.
 * r[impl message.goodbye.send] - Close a virtual connection.
 * r[impl message.goodbye.connection-zero] - Goodbye on conn 0 closes entire link.
 */
export interface MessageGoodbye {
  tag: "Goodbye";
  connId: bigint;
  reason: string;
}

/**
 * Request message (discriminant = 5).
 * RPC - scoped to conn_id.
 *
 * r[impl core.metadata] - Request carries metadata key-value pairs.
 * r[impl call.metadata.unknown] - Unknown keys are ignored.
 * r[impl channeling.request.channels] - Channel IDs listed explicitly for proxy support.
 */
export interface MessageRequest {
  tag: "Request";
  connId: bigint;
  requestId: bigint;
  methodId: bigint;
  metadata: MetadataEntry[];
  /** Channel IDs used by this call, in argument declaration order. */
  channels: bigint[];
  payload: Uint8Array;
}

/**
 * Response message (discriminant = 6).
 * RPC - scoped to conn_id.
 *
 * r[impl core.metadata] - Response carries metadata key-value pairs.
 * r[impl call.metadata.unknown] - Unknown keys are ignored.
 */
export interface MessageResponse {
  tag: "Response";
  connId: bigint;
  requestId: bigint;
  metadata: MetadataEntry[];
  /** Channel IDs for streams in the response, in return type declaration order. */
  channels: bigint[];
  payload: Uint8Array;
}

/**
 * Cancel message (discriminant = 7).
 * RPC - scoped to conn_id.
 *
 * r[impl call.cancel.message] - Cancel message requests callee stop processing.
 * r[impl call.cancel.no-response-required] - Caller should timeout, not wait indefinitely.
 */
export interface MessageCancel {
  tag: "Cancel";
  connId: bigint;
  requestId: bigint;
}

/**
 * Data message (discriminant = 8).
 * Channels - scoped to conn_id.
 *
 * r[impl channeling.type] - Tx<T>/Rx<T> encoded as u64 channel ID on wire
 */
export interface MessageData {
  tag: "Data";
  connId: bigint;
  channelId: bigint;
  payload: Uint8Array;
}

/**
 * Close message (discriminant = 9).
 * Channels - scoped to conn_id.
 */
export interface MessageClose {
  tag: "Close";
  connId: bigint;
  channelId: bigint;
}

/**
 * Reset message (discriminant = 10).
 * Channels - scoped to conn_id.
 */
export interface MessageReset {
  tag: "Reset";
  connId: bigint;
  channelId: bigint;
}

/**
 * Credit message (discriminant = 11).
 * Channels - scoped to conn_id.
 */
export interface MessageCredit {
  tag: "Credit";
  connId: bigint;
  channelId: bigint;
  bytes: number;
}

/**
 * Protocol message.
 *
 * Variant order is wire-significant (postcard enum discriminants).
 */
export type Message =
  | MessageHello
  | MessageConnect
  | MessageAccept
  | MessageReject
  | MessageGoodbye
  | MessageRequest
  | MessageResponse
  | MessageCancel
  | MessageData
  | MessageClose
  | MessageReset
  | MessageCredit;

// ============================================================================
// Message Discriminants
// ============================================================================

/**
 * Wire discriminant values for Message variants.
 * These match the Rust #[repr(u8)] = N values.
 */
export const MessageDiscriminant = {
  Hello: 0,
  Connect: 1,
  Accept: 2,
  Reject: 3,
  Goodbye: 4,
  Request: 5,
  Response: 6,
  Cancel: 7,
  Data: 8,
  Close: 9,
  Reset: 10,
  Credit: 11,
} as const;

/**
 * Wire discriminant values for MetadataValue variants.
 */
export const MetadataValueDiscriminant = {
  String: 0,
  Bytes: 1,
  U64: 2,
} as const;

/**
 * Wire discriminant values for Hello variants.
 */
export const HelloDiscriminant = {
  V1: 0, // deprecated
  V2: 1, // deprecated
  V3: 2,
} as const;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create a Hello.V3 message.
 */
export function helloV3(maxPayloadSize: number, initialChannelCredit: number): HelloV3 {
  return { tag: "V3", maxPayloadSize, initialChannelCredit };
}

/**
 * Create a MetadataValue.String.
 */
export function metadataString(value: string): MetadataValue {
  return { tag: "String", value };
}

/**
 * Create a MetadataValue.Bytes.
 */
export function metadataBytes(value: Uint8Array): MetadataValue {
  return { tag: "Bytes", value };
}

/**
 * Create a MetadataValue.U64.
 */
export function metadataU64(value: bigint): MetadataValue {
  return { tag: "U64", value };
}

/**
 * Create a Message.Hello.
 */
export function messageHello(hello: Hello): Message {
  return { tag: "Hello", value: hello };
}

/**
 * Create a Message.Connect.
 */
export function messageConnect(requestId: bigint, metadata: MetadataEntry[] = []): Message {
  return { tag: "Connect", requestId, metadata };
}

/**
 * Create a Message.Accept.
 */
export function messageAccept(
  requestId: bigint,
  connId: bigint,
  metadata: MetadataEntry[] = [],
): Message {
  return { tag: "Accept", requestId, connId, metadata };
}

/**
 * Create a Message.Reject.
 */
export function messageReject(
  requestId: bigint,
  reason: string,
  metadata: MetadataEntry[] = [],
): Message {
  return { tag: "Reject", requestId, reason, metadata };
}

/**
 * Create a Message.Goodbye.
 */
export function messageGoodbye(reason: string, connId: bigint = 0n): Message {
  return { tag: "Goodbye", connId, reason };
}

/**
 * Create a Message.Request.
 */
export function messageRequest(
  requestId: bigint,
  methodId: bigint,
  payload: Uint8Array,
  metadata: MetadataEntry[] = [],
  channels: bigint[] = [],
  connId: bigint = 0n,
): Message {
  return { tag: "Request", connId, requestId, methodId, metadata, channels, payload };
}

/**
 * Create a Message.Response.
 */
export function messageResponse(
  requestId: bigint,
  payload: Uint8Array,
  metadata: MetadataEntry[] = [],
  channels: bigint[] = [],
  connId: bigint = 0n,
): Message {
  return { tag: "Response", connId, requestId, metadata, channels, payload };
}

/**
 * Create a Message.Cancel.
 */
export function messageCancel(requestId: bigint, connId: bigint = 0n): Message {
  return { tag: "Cancel", connId, requestId };
}

/**
 * Create a Message.Data.
 */
export function messageData(channelId: bigint, payload: Uint8Array, connId: bigint = 0n): Message {
  return { tag: "Data", connId, channelId, payload };
}

/**
 * Create a Message.Close.
 */
export function messageClose(channelId: bigint, connId: bigint = 0n): Message {
  return { tag: "Close", connId, channelId };
}

/**
 * Create a Message.Reset.
 */
export function messageReset(channelId: bigint, connId: bigint = 0n): Message {
  return { tag: "Reset", connId, channelId };
}

/**
 * Create a Message.Credit.
 */
export function messageCredit(channelId: bigint, bytes: number, connId: bigint = 0n): Message {
  return { tag: "Credit", connId, channelId, bytes };
}
