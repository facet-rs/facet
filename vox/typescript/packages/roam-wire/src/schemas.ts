// Roam wire protocol schemas for TypeScript.
//
// These schemas match the Rust definitions in roam-wire/src/lib.rs exactly.
// The discriminants match the #[repr(u8)] values used in Rust.

import type {
  Schema,
  SchemaRegistry,
  EnumSchema,
  TupleSchema,
} from "@bearcove/roam-postcard";

// ============================================================================
// Hello Schema
// ============================================================================

/**
 * Schema for Hello enum.
 *
 * Rust definition:
 * ```rust
 * #[repr(u8)]
 * pub enum Hello {
 *     V1 { max_payload_size: u32, initial_channel_credit: u32 } = 0,
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
 * Schema for a metadata entry tuple (String, MetadataValue).
 */
export const MetadataEntrySchema: TupleSchema = {
  kind: "tuple",
  elements: [{ kind: "string" }, { kind: "ref", name: "MetadataValue" }],
};

// ============================================================================
// Message Schema
// ============================================================================

/**
 * Schema for Message enum.
 *
 * Rust definition:
 * ```rust
 * #[repr(u8)]
 * pub enum Message {
 *     Hello(Hello) = 0,
 *     Goodbye { reason: String } = 1,
 *     Request { request_id: u64, method_id: u64, metadata: Vec<(String, MetadataValue)>, channels: Vec<u64>, payload: Vec<u8> } = 2,
 *     Response { request_id: u64, metadata: Vec<(String, MetadataValue)>, payload: Vec<u8> } = 3,
 *     Cancel { request_id: u64 } = 4,
 *     Data { channel_id: u64, payload: Vec<u8> } = 5,
 *     Close { channel_id: u64 } = 6,
 *     Reset { channel_id: u64 } = 7,
 *     Credit { channel_id: u64, bytes: u32 } = 8,
 * }
 * ```
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
    // Goodbye { reason: String } = 1
    {
      name: "Goodbye",
      discriminant: 1,
      fields: {
        reason: { kind: "string" },
      },
    },
    // Request { request_id: u64, method_id: u64, metadata: Vec<(String, MetadataValue)>, channels: Vec<u64>, payload: Vec<u8> } = 2
    {
      name: "Request",
      discriminant: 2,
      fields: {
        requestId: { kind: "u64" },
        methodId: { kind: "u64" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
        channels: { kind: "vec", element: { kind: "u64" } },
        payload: { kind: "bytes" },
      },
    },
    // Response { request_id: u64, metadata: Vec<(String, MetadataValue)>, payload: Vec<u8> } = 3
    {
      name: "Response",
      discriminant: 3,
      fields: {
        requestId: { kind: "u64" },
        metadata: { kind: "vec", element: { kind: "ref", name: "MetadataEntry" } },
        payload: { kind: "bytes" },
      },
    },
    // Cancel { request_id: u64 } = 4
    {
      name: "Cancel",
      discriminant: 4,
      fields: {
        requestId: { kind: "u64" },
      },
    },
    // Data { channel_id: u64, payload: Vec<u8> } = 5
    {
      name: "Data",
      discriminant: 5,
      fields: {
        channelId: { kind: "u64" },
        payload: { kind: "bytes" },
      },
    },
    // Close { channel_id: u64 } = 6
    {
      name: "Close",
      discriminant: 6,
      fields: {
        channelId: { kind: "u64" },
      },
    },
    // Reset { channel_id: u64 } = 7
    {
      name: "Reset",
      discriminant: 7,
      fields: {
        channelId: { kind: "u64" },
      },
    },
    // Credit { channel_id: u64, bytes: u32 } = 8
    {
      name: "Credit",
      discriminant: 8,
      fields: {
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
