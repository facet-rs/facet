// Roam wire protocol schemas for TypeScript.
//
// These schemas match the Rust definitions in roam-wire/src/lib.rs exactly.
// The discriminants match the #[repr(u8)] values used in Rust.

import type { Schema, SchemaRegistry, EnumSchema, TupleSchema } from "@bearcove/roam-postcard";

// ============================================================================
// Hello Schema
// ============================================================================

/**
 * Schema for Hello enum.
 *
 * Rust definition (v4.0.0):
 * ```rust
 * #[repr(u8)]
 * pub enum Hello {
 *     V1 { max_payload_size: u32, initial_channel_credit: u32 } = 0,
 *     V2 { max_payload_size: u32, initial_channel_credit: u32 } = 1,
 *     V4 { max_payload_size: u32, initial_channel_credit: u32 } = 3,
 *     V5 { max_payload_size: u32, initial_channel_credit: u32, max_concurrent_requests: u32 } = 4,
 * }
 * ```
 */
export const HelloSchema: EnumSchema = {
  kind: "enum",
  variants: [
    {
      name: "V1",
      discriminant: 0,
      fields: {
        maxPayloadSize: { kind: "u32" },
        initialChannelCredit: { kind: "u32" },
      },
    },
    {
      name: "V2",
      discriminant: 1,
      fields: {
        maxPayloadSize: { kind: "u32" },
        initialChannelCredit: { kind: "u32" },
      },
    },
    {
      name: "V4",
      discriminant: 3,
      fields: {
        maxPayloadSize: { kind: "u32" },
        initialChannelCredit: { kind: "u32" },
      },
    },
    {
      name: "V5",
      discriminant: 4,
      fields: {
        maxPayloadSize: { kind: "u32" },
        initialChannelCredit: { kind: "u32" },
        maxConcurrentRequests: { kind: "u32" },
      },
    },
    {
      name: "V6",
      discriminant: 5,
      fields: {
        maxPayloadSize: { kind: "u32" },
        initialChannelCredit: { kind: "u32" },
        maxConcurrentRequests: { kind: "u32" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
      },
    },
  ],
};

// ============================================================================
// MetadataValue Schema
// ============================================================================

/**
 * Schema for MetadataValue enum.
 *
 * Rust definition:
 * ```rust
 * #[repr(u8)]
 * pub enum MetadataValue {
 *     String(String) = 0,
 *     Bytes(Vec<u8>) = 1,
 *     U64(u64) = 2,
 * }
 * ```
 */
export const MetadataValueSchema: EnumSchema = {
  kind: "enum",
  variants: [
    { name: "String", discriminant: 0, fields: { kind: "string" } },
    { name: "Bytes", discriminant: 1, fields: { kind: "bytes" } },
    { name: "U64", discriminant: 2, fields: { kind: "u64" } },
  ],
};

// ============================================================================
// MetadataEntry Schema
// ============================================================================

/**
 * Schema for a metadata entry tuple (String, MetadataValue, u64).
 *
 * r[impl call.metadata.type] - Metadata is a list of entries.
 * r[impl call.metadata.flags] - Each entry includes flags for handling behavior.
 */
export const MetadataEntrySchema: TupleSchema = {
  kind: "tuple",
  elements: [{ kind: "string" }, { kind: "ref", name: "MetadataValue" }, { kind: "u64" }],
};

// ============================================================================
// Message Schema
// ============================================================================

/**
 * Schema for Message enum.
 *
 * Rust definition (v4.0.0):
 * ```rust
 * #[repr(u8)]
 * pub enum Message {
 *     Hello(Hello) = 0,
 *     Connect { request_id: u64, metadata: Metadata } = 1,
 *     Accept { request_id: u64, conn_id: u64, metadata: Metadata } = 2,
 *     Reject { request_id: u64, reason: String, metadata: Metadata } = 3,
 *     Goodbye { conn_id: u64, reason: String } = 4,
 *     Request { conn_id: u64, request_id: u64, method_id: u64, metadata: Metadata, channels: Vec<u64>, payload: Vec<u8> } = 5,
 *     Response { conn_id: u64, request_id: u64, metadata: Metadata, channels: Vec<u64>, payload: Vec<u8> } = 6,
 *     Cancel { conn_id: u64, request_id: u64 } = 7,
 *     Data { conn_id: u64, channel_id: u64, payload: Vec<u8> } = 8,
 *     Close { conn_id: u64, channel_id: u64 } = 9,
 *     Reset { conn_id: u64, channel_id: u64 } = 10,
 *     Credit { conn_id: u64, channel_id: u64, bytes: u32 } = 11,
 * }
 * ```
 *
 * Where `Metadata = Vec<(String, MetadataValue, u64)>`.
 */
export const MessageSchema: EnumSchema = {
  kind: "enum",
  variants: [
    // Hello(Hello) = 0
    {
      name: "Hello",
      discriminant: 0,
      fields: { kind: "ref", name: "Hello" },
    },
    // Connect { request_id: u64, metadata: Vec<(String, MetadataValue)> } = 1
    {
      name: "Connect",
      discriminant: 1,
      fields: {
        requestId: { kind: "u64" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
      },
    },
    // Accept { request_id: u64, conn_id: u64, metadata: Vec<(String, MetadataValue)> } = 2
    {
      name: "Accept",
      discriminant: 2,
      fields: {
        requestId: { kind: "u64" },
        connId: { kind: "u64" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
      },
    },
    // Reject { request_id: u64, reason: String, metadata: Vec<(String, MetadataValue)> } = 3
    {
      name: "Reject",
      discriminant: 3,
      fields: {
        requestId: { kind: "u64" },
        reason: { kind: "string" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
      },
    },
    // Goodbye { conn_id: u64, reason: String } = 4
    {
      name: "Goodbye",
      discriminant: 4,
      fields: {
        connId: { kind: "u64" },
        reason: { kind: "string" },
      },
    },
    // Request { conn_id: u64, request_id: u64, method_id: u64, metadata: Vec<(String, MetadataValue)>, channels: Vec<u64>, payload: Vec<u8> } = 5
    {
      name: "Request",
      discriminant: 5,
      fields: {
        connId: { kind: "u64" },
        requestId: { kind: "u64" },
        methodId: { kind: "u64" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
        channels: { kind: "vec", element: { kind: "u64" } },
        payload: { kind: "bytes" },
      },
    },
    // Response { conn_id: u64, request_id: u64, metadata: Vec<(String, MetadataValue)>, channels: Vec<u64>, payload: Vec<u8> } = 6
    {
      name: "Response",
      discriminant: 6,
      fields: {
        connId: { kind: "u64" },
        requestId: { kind: "u64" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
        channels: { kind: "vec", element: { kind: "u64" } },
        payload: { kind: "bytes" },
      },
    },
    // Cancel { conn_id: u64, request_id: u64 } = 7
    {
      name: "Cancel",
      discriminant: 7,
      fields: {
        connId: { kind: "u64" },
        requestId: { kind: "u64" },
      },
    },
    // Data { conn_id: u64, channel_id: u64, payload: Vec<u8> } = 8
    {
      name: "Data",
      discriminant: 8,
      fields: {
        connId: { kind: "u64" },
        channelId: { kind: "u64" },
        payload: { kind: "bytes" },
      },
    },
    // Close { conn_id: u64, channel_id: u64 } = 9
    {
      name: "Close",
      discriminant: 9,
      fields: {
        connId: { kind: "u64" },
        channelId: { kind: "u64" },
      },
    },
    // Reset { conn_id: u64, channel_id: u64 } = 10
    {
      name: "Reset",
      discriminant: 10,
      fields: {
        connId: { kind: "u64" },
        channelId: { kind: "u64" },
      },
    },
    // Credit { conn_id: u64, channel_id: u64, bytes: u32 } = 11
    {
      name: "Credit",
      discriminant: 11,
      fields: {
        connId: { kind: "u64" },
        channelId: { kind: "u64" },
        bytes: { kind: "u32" },
      },
    },
  ],
};

// ============================================================================
// Wire Schema Registry
// ============================================================================

/**
 * Registry of all wire protocol schemas.
 *
 * Use this registry when encoding/decoding wire types with `encodeWithSchema`
 * and `decodeWithSchema` to resolve type references.
 */
export const wireSchemaRegistry: SchemaRegistry = new Map<string, Schema>([
  ["Hello", HelloSchema],
  ["MetadataValue", MetadataValueSchema],
  ["MetadataEntry", MetadataEntrySchema],
  ["Message", MessageSchema],
]);

// ============================================================================
// Convenience Exports
// ============================================================================

/**
 * Get the schema for Hello type.
 */
export function getHelloSchema(): EnumSchema {
  return HelloSchema;
}

/**
 * Get the schema for MetadataValue type.
 */
export function getMetadataValueSchema(): EnumSchema {
  return MetadataValueSchema;
}

/**
 * Get the schema for MetadataEntry type.
 */
export function getMetadataEntrySchema(): TupleSchema {
  return MetadataEntrySchema;
}

/**
 * Get the schema for Message type.
 */
export function getMessageSchema(): EnumSchema {
  return MessageSchema;
}
