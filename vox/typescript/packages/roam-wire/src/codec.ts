// Wire codec wrappers for encoding/decoding Roam protocol messages.
//
// These functions provide type-safe encoding/decoding of wire protocol types
// using the schema-driven approach.

import {
  encodeWithSchema,
  decodeWithSchema,
  type DecodeResult,
} from "@bearcove/roam-postcard";

import type { Hello, MetadataValue, MetadataEntry, Message } from "./types.ts";
import {
  HelloSchema,
  MetadataValueSchema,
  MetadataEntrySchema,
  MessageSchema,
  wireSchemaRegistry,
} from "./schemas.ts";

// ============================================================================
// Hello Encoding/Decoding
// ============================================================================

/**
 * Encode a Hello message to bytes.
 *
 * @param hello - The Hello message to encode
 * @returns Encoded bytes
 */
export function encodeHello(hello: Hello): Uint8Array {
  return encodeWithSchema(hello, HelloSchema, wireSchemaRegistry);
}

/**
 * Decode a Hello message from bytes.
 *
 * @param buf - Buffer to decode from
 * @param offset - Starting offset (default: 0)
 * @returns Decoded Hello and next offset
 */
export function decodeHello(buf: Uint8Array, offset = 0): DecodeResult<Hello> {
  return decodeWithSchema(buf, offset, HelloSchema, wireSchemaRegistry) as DecodeResult<Hello>;
}

// ============================================================================
// MetadataValue Encoding/Decoding
// ============================================================================

/**
 * Encode a MetadataValue to bytes.
 *
 * @param value - The MetadataValue to encode
 * @returns Encoded bytes
 */
export function encodeMetadataValue(value: MetadataValue): Uint8Array {
  return encodeWithSchema(value, MetadataValueSchema, wireSchemaRegistry);
}

/**
 * Decode a MetadataValue from bytes.
 *
 * @param buf - Buffer to decode from
 * @param offset - Starting offset (default: 0)
 * @returns Decoded MetadataValue and next offset
 */
export function decodeMetadataValue(
  buf: Uint8Array,
  offset = 0
): DecodeResult<MetadataValue> {
  return decodeWithSchema(
    buf,
    offset,
    MetadataValueSchema,
    wireSchemaRegistry
  ) as DecodeResult<MetadataValue>;
}

// ============================================================================
// MetadataEntry Encoding/Decoding
// ============================================================================

/**
 * Encode a MetadataEntry (key-value pair) to bytes.
 *
 * @param entry - The MetadataEntry to encode
 * @returns Encoded bytes
 */
export function encodeMetadataEntry(entry: MetadataEntry): Uint8Array {
  return encodeWithSchema(entry, MetadataEntrySchema, wireSchemaRegistry);
}

/**
 * Decode a MetadataEntry from bytes.
 *
 * @param buf - Buffer to decode from
 * @param offset - Starting offset (default: 0)
 * @returns Decoded MetadataEntry and next offset
 */
export function decodeMetadataEntry(
  buf: Uint8Array,
  offset = 0
): DecodeResult<MetadataEntry> {
  return decodeWithSchema(
    buf,
    offset,
    MetadataEntrySchema,
    wireSchemaRegistry
  ) as DecodeResult<MetadataEntry>;
}

// ============================================================================
// Message Encoding/Decoding
// ============================================================================

/**
 * Encode a Message to bytes.
 *
 * This is the main entry point for encoding wire protocol messages.
 *
 * @param message - The Message to encode
 * @returns Encoded bytes
 */
export function encodeMessage(message: Message): Uint8Array {
  return encodeWithSchema(message, MessageSchema, wireSchemaRegistry);
}

/**
 * Decode a Message from bytes.
 *
 * This is the main entry point for decoding wire protocol messages.
 *
 * @param buf - Buffer to decode from
 * @param offset - Starting offset (default: 0)
 * @returns Decoded Message and next offset
 */
export function decodeMessage(buf: Uint8Array, offset = 0): DecodeResult<Message> {
  return decodeWithSchema(buf, offset, MessageSchema, wireSchemaRegistry) as DecodeResult<Message>;
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Decode multiple Messages from a buffer.
 *
 * Useful for processing a stream of messages.
 *
 * @param buf - Buffer containing multiple messages
 * @returns Array of decoded messages
 */
export function decodeMessages(buf: Uint8Array): Message[] {
  const messages: Message[] = [];
  let offset = 0;
  while (offset < buf.length) {
    const result = decodeMessage(buf, offset);
    messages.push(result.value);
    offset = result.next;
  }
  return messages;
}

/**
 * Encode multiple Messages to a single buffer.
 *
 * @param messages - Array of messages to encode
 * @returns Concatenated encoded bytes
 */
export function encodeMessages(messages: Message[]): Uint8Array {
  const parts: Uint8Array[] = messages.map(encodeMessage);
  const totalLength = parts.reduce((sum, p) => sum + p.length, 0);
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}
