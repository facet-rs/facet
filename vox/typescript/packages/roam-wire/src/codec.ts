// Wire codec wrappers for encoding/decoding Roam protocol messages.

import {
  decodeWithPlan,
  decodeWithTypeRef,
  encodeWithTypeRef,
  resolveWireTypeRef,
  type DecodeResult,
  type TranslationPlan,
  type WireSchemaKind,
  type WireSchemaRegistry,
} from "@bearcove/roam-postcard";

import type { Message } from "./types.ts";
import {
  wireMessageRootRef,
  wireMessageSchemaRegistry,
} from "./schemas.ts";

export function encodeMessage(message: Message): Uint8Array {
  return encodeWithTypeRef(message, wireMessageRootRef, wireMessageSchemaRegistry);
}

export function decodeMessage(buf: Uint8Array, offset = 0): DecodeResult<Message> {
  return decodeWithTypeRef(
    buf,
    offset,
    wireMessageRootRef,
    wireMessageSchemaRegistry,
  ) as DecodeResult<Message>;
}

export function decodeMessageWithPlan(
  buf: Uint8Array,
  offset: number,
  plan: TranslationPlan,
  remoteRootKind: WireSchemaKind,
  remoteRegistry: WireSchemaRegistry,
): DecodeResult<Message> {
  const localRootKind = resolveWireTypeRef(wireMessageRootRef, wireMessageSchemaRegistry);
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
      ...wireMessageSchemaRegistry,
      ...remoteRegistry,
    ]),
  ) as DecodeResult<Message>;
}
