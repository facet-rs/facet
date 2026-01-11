// Roam wire protocol types for TypeScript.
//
// These types match the Rust definitions in roam-wire/src/lib.rs exactly.
// The discriminants match the #[repr(u8)] values used in Rust.

// ============================================================================
// Hello Message
// ============================================================================

/**
 * Hello message variant V1.
 */
export interface HelloV1 {
  tag: "V1";
  maxPayloadSize: number;
  initialChannelCredit: number;
}

/**
 * Hello message for handshake.
 *
 * r[impl message.hello.structure]
 */
export type Hello = HelloV1;

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
 * r[impl unary.metadata.type]
 */
export type MetadataValue = MetadataValueString | MetadataValueBytes | MetadataValueU64;

/**
 * A metadata entry is a key-value pair.
 */
export type MetadataEntry = [string, MetadataValue];

// ============================================================================
// Message
// ============================================================================

/**
 * Hello message (discriminant = 0).
 */
export interface MessageHello {
  tag: "Hello";
  value: Hello;
}

/**
 * Goodbye message (discriminant = 1).
 */
export interface MessageGoodbye {
  tag: "Goodbye";
  reason: string;
}

/**
 * Request message (discriminant = 2).
 *
 * r[impl core.metadata] - Request carries metadata key-value pairs.
 * r[impl unary.metadata.unknown] - Unknown keys are ignored.
 */
export interface MessageRequest {
  tag: "Request";
  requestId: bigint;
  methodId: bigint;
  metadata: MetadataEntry[];
  payload: Uint8Array;
}

/**
 * Response message (discriminant = 3).
 *
 * r[impl core.metadata] - Response carries metadata key-value pairs.
 * r[impl unary.metadata.unknown] - Unknown keys are ignored.
 */
export interface MessageResponse {
  tag: "Response";
  requestId: bigint;
  metadata: MetadataEntry[];
  payload: Uint8Array;
}

/**
 * Cancel message (discriminant = 4).
 *
 * r[impl unary.cancel.message] - Cancel message requests callee stop processing.
 * r[impl unary.cancel.no-response-required] - Caller should timeout, not wait indefinitely.
 */
export interface MessageCancel {
  tag: "Cancel";
  requestId: bigint;
}

/**
 * Data message (discriminant = 5).
 *
 * r[impl channeling.type] - Tx<T>/Rx<T> encoded as u64 channel ID on wire
 */
export interface MessageData {
  tag: "Data";
  channelId: bigint;
  payload: Uint8Array;
}

/**
 * Close message (discriminant = 6).
 */
export interface MessageClose {
  tag: "Close";
  channelId: bigint;
}

/**
 * Reset message (discriminant = 7).
 */
export interface MessageReset {
  tag: "Reset";
  channelId: bigint;
}

/**
 * Credit message (discriminant = 8).
 */
export interface MessageCredit {
  tag: "Credit";
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
  Goodbye: 1,
  Request: 2,
  Response: 3,
  Cancel: 4,
  Data: 5,
  Close: 6,
  Reset: 7,
  Credit: 8,
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
  V1: 0,
} as const;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create a Hello.V1 message.
 */
export function helloV1(maxPayloadSize: number, initialChannelCredit: number): Hello {
  return { tag: "V1", maxPayloadSize, initialChannelCredit };
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
 * Create a Message.Goodbye.
 */
export function messageGoodbye(reason: string): Message {
  return { tag: "Goodbye", reason };
}

/**
 * Create a Message.Request.
 */
export function messageRequest(
  requestId: bigint,
  methodId: bigint,
  payload: Uint8Array,
  metadata: MetadataEntry[] = [],
): Message {
  return { tag: "Request", requestId, methodId, metadata, payload };
}

/**
 * Create a Message.Response.
 */
export function messageResponse(
  requestId: bigint,
  payload: Uint8Array,
  metadata: MetadataEntry[] = [],
): Message {
  return { tag: "Response", requestId, metadata, payload };
}

/**
 * Create a Message.Cancel.
 */
export function messageCancel(requestId: bigint): Message {
  return { tag: "Cancel", requestId };
}

/**
 * Create a Message.Data.
 */
export function messageData(channelId: bigint, payload: Uint8Array): Message {
  return { tag: "Data", channelId, payload };
}

/**
 * Create a Message.Close.
 */
export function messageClose(channelId: bigint): Message {
  return { tag: "Close", channelId };
}

/**
 * Create a Message.Reset.
 */
export function messageReset(channelId: bigint): Message {
  return { tag: "Reset", channelId };
}

/**
 * Create a Message.Credit.
 */
export function messageCredit(channelId: bigint, bytes: number): Message {
  return { tag: "Credit", channelId, bytes };
}
