// Wire codec wrappers for encoding/decoding Roam protocol messages.

import { encodeWithSchema, decodeWithSchema, type DecodeResult } from "@bearcove/roam-postcard";

import type { Message } from "./types.ts";
import {
  MessageSchema,
  wireSchemaRegistry,
} from "./schemas.ts";

export function encodeMessage(message: Message): Uint8Array {
  return encodeWithSchema(message, MessageSchema, wireSchemaRegistry);
}

export function decodeMessage(buf: Uint8Array, offset = 0): DecodeResult<Message> {
  return decodeWithSchema(buf, offset, MessageSchema, wireSchemaRegistry) as DecodeResult<Message>;
}

