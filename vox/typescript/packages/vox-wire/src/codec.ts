// Wire codec wrappers for encoding/decoding Vox protocol messages.

import {
  decodeWithPlan,
  decodeWithTypeRef,
  encodeWithTypeRef,
  resolveTypeRef,
  type DecodeResult,
  type TranslationPlan,
  type SchemaKind,
  type SchemaRegistry,
} from "@bearcove/vox-postcard";

import type { Message } from "./types.ts";
import {
  messageRootRef,
  messageSchemaRegistry,
} from "./schemas.ts";

export function encodeMessage(message: Message): Uint8Array {
  return encodeWithTypeRef(message, messageRootRef, messageSchemaRegistry);
}

export function decodeMessage(buf: Uint8Array, offset = 0): DecodeResult<Message> {
  return decodeWithTypeRef(
    buf,
    offset,
    messageRootRef,
    messageSchemaRegistry,
  ) as DecodeResult<Message>;
}

export function decodeMessageWithPlan(
  buf: Uint8Array,
  offset: number,
  plan: TranslationPlan,
  remoteRootKind: SchemaKind,
  remoteRegistry: SchemaRegistry,
): DecodeResult<Message> {
  const localRootKind = resolveTypeRef(messageRootRef, messageSchemaRegistry);
  if (!localRootKind) {
    throw new Error("wire message root schema not found");
  }
  return decodeWithPlan(
    buf,
    offset,
    plan,
    localRootKind,
    remoteRootKind,
    new Map([
      ...messageSchemaRegistry,
      ...remoteRegistry,
    ]),
  ) as DecodeResult<Message>;
}
