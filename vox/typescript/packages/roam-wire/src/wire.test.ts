import { describe, expect, it } from "vitest";
import { resolveTypeRef } from "@bearcove/roam-postcard";

import {
  MetadataFlagValues,
  metadataBytes,
  metadataEntry,
  metadataString,
  messageClose,
  messageCredit,
  messageData,
  messageRequest,
  parityOdd,
} from "./types.ts";
import { decodeMessage, encodeMessage } from "./codec.ts";
import { messageRootRef, messageSchemaRegistry } from "./schemas.ts";

describe("wire helpers", () => {
  it("builds nested request and channel helpers with the expected tags", () => {
    const metadata = [metadataEntry("trace-id", metadataString("abc123"), MetadataFlagValues.NONE)];
    const request = messageRequest(11n, 42n, new Uint8Array([1, 2]), metadata, [3n], 2n);
    const item = messageData(5n, new Uint8Array([8, 7]), 2n);
    const close = messageClose(5n, 2n, metadata);
    const credit = messageCredit(5n, 1024, 2n);

    expect(request.payload).toMatchObject({
      tag: "RequestMessage",
      value: { id: 11n, body: { tag: "Call" } },
    });
    expect(item.payload).toMatchObject({
      tag: "ChannelMessage",
      value: { id: 5n, body: { tag: "Item" } },
    });
    expect(close.payload.tag).toBe("ChannelMessage");
    expect(credit.payload.tag).toBe("ChannelMessage");

    if (close.payload.tag !== "ChannelMessage" || credit.payload.tag !== "ChannelMessage") {
      throw new Error("expected channel messages");
    }

    expect(close.payload.value.body.tag).toBe("Close");
    expect(credit.payload.value.body.tag).toBe("GrantCredit");
  });
});

describe("wire codec", () => {
  it("roundtrips arbitrary request payloads and consumes the full buffer", () => {
    const metadata = [
      metadataEntry("trace-id", metadataString("abc123"), MetadataFlagValues.NONE),
      metadataEntry(
        "payload",
        metadataBytes(new Uint8Array([0xde, 0xad, 0xbe, 0xef])),
        MetadataFlagValues.SENSITIVE,
      ),
    ];
    const message = messageRequest(99n, 0xE5A1_D6B2_C390_F001n, new Uint8Array([1, 2, 3, 4]), metadata, [
      3n,
      5n,
      8n,
    ], 2n);

    const encoded = encodeMessage(message);
    const decoded = decodeMessage(encoded);

    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual(message);
  });
});

describe("generated wire schemas", () => {
  it("marks opaque payload fields as canonical payload primitives", () => {
    const messageKind = resolveTypeRef(messageRootRef, messageSchemaRegistry);
    expect(messageKind?.tag).toBe("struct");
    if (!messageKind || messageKind.tag !== "struct") {
      throw new Error("expected Message root to resolve to a struct");
    }

    const payloadField = messageKind.fields.find((field) => field.name === "payload");
    expect(payloadField).toBeDefined();
    if (!payloadField) {
      throw new Error("expected Message.payload field");
    }

    const payloadKind = resolveTypeRef(payloadField.type_ref, messageSchemaRegistry);
    expect(payloadKind?.tag).toBe("enum");
    if (!payloadKind || payloadKind.tag !== "enum") {
      throw new Error("expected Message.payload to resolve to an enum");
    }

    const requestMessageVariant = payloadKind.variants.find((variant) => variant.name === "RequestMessage");
    const channelMessageVariant = payloadKind.variants.find((variant) => variant.name === "ChannelMessage");
    expect(requestMessageVariant?.payload.tag).toBe("newtype");
    expect(channelMessageVariant?.payload.tag).toBe("newtype");
    if (!requestMessageVariant || requestMessageVariant.payload.tag !== "newtype") {
      throw new Error("expected Message.payload.RequestMessage to be a newtype");
    }
    if (!channelMessageVariant || channelMessageVariant.payload.tag !== "newtype") {
      throw new Error("expected Message.payload.ChannelMessage to be a newtype");
    }

    const requestMessageKind = resolveTypeRef(
      requestMessageVariant.payload.type_ref,
      messageSchemaRegistry,
    );
    expect(requestMessageKind?.tag).toBe("struct");
    if (!requestMessageKind || requestMessageKind.tag !== "struct") {
      throw new Error("expected RequestMessage to resolve to a struct");
    }

    const requestBodyField = requestMessageKind.fields.find((field) => field.name === "body");
    expect(requestBodyField).toBeDefined();
    if (!requestBodyField) {
      throw new Error("expected RequestMessage.body field");
    }

    const requestBodyKind = resolveTypeRef(requestBodyField.type_ref, messageSchemaRegistry);
    expect(requestBodyKind?.tag).toBe("enum");
    if (!requestBodyKind || requestBodyKind.tag !== "enum") {
      throw new Error("expected RequestBody to resolve to an enum");
    }

    const callVariant = requestBodyKind.variants.find((variant) => variant.name === "Call");
    expect(callVariant?.payload.tag).toBe("newtype");
    if (!callVariant || callVariant.payload.tag !== "newtype") {
      throw new Error("expected RequestBody.Call to be a newtype");
    }

    const requestCallKind = resolveTypeRef(callVariant.payload.type_ref, messageSchemaRegistry);
    expect(requestCallKind?.tag).toBe("struct");
    if (!requestCallKind || requestCallKind.tag !== "struct") {
      throw new Error("expected RequestCall to resolve to a struct");
    }

    const argsField = requestCallKind.fields.find((field) => field.name === "args");
    expect(argsField).toBeDefined();
    if (!argsField) {
      throw new Error("expected RequestCall.args field");
    }

    const argsKind = resolveTypeRef(argsField.type_ref, messageSchemaRegistry);
    expect(argsKind).toEqual({ tag: "primitive", primitive_type: "payload" });

    const channelMessageKind = resolveTypeRef(
      channelMessageVariant.payload.type_ref,
      messageSchemaRegistry,
    );
    expect(channelMessageKind?.tag).toBe("struct");
    if (!channelMessageKind || channelMessageKind.tag !== "struct") {
      throw new Error("expected ChannelMessage to resolve to a struct");
    }

    const channelBodyField = channelMessageKind.fields.find((field) => field.name === "body");
    expect(channelBodyField).toBeDefined();
    if (!channelBodyField) {
      throw new Error("expected ChannelMessage.body field");
    }

    const channelBodyKind = resolveTypeRef(channelBodyField.type_ref, messageSchemaRegistry);
    expect(channelBodyKind?.tag).toBe("enum");
    if (!channelBodyKind || channelBodyKind.tag !== "enum") {
      throw new Error("expected ChannelBody to resolve to an enum");
    }

    const itemVariant = channelBodyKind.variants.find((variant) => variant.name === "Item");
    expect(itemVariant?.payload.tag).toBe("newtype");
    if (!itemVariant || itemVariant.payload.tag !== "newtype") {
      throw new Error("expected ChannelBody.Item to be a newtype");
    }

    const channelItemKind = resolveTypeRef(itemVariant.payload.type_ref, messageSchemaRegistry);
    expect(channelItemKind?.tag).toBe("struct");
    if (!channelItemKind || channelItemKind.tag !== "struct") {
      throw new Error("expected ChannelItem to resolve to a struct");
    }

    const itemField = channelItemKind.fields.find((field) => field.name === "item");
    expect(itemField).toBeDefined();
    if (!itemField) {
      throw new Error("expected ChannelItem.item field");
    }

    const itemKind = resolveTypeRef(itemField.type_ref, messageSchemaRegistry);
    expect(itemKind).toEqual({ tag: "primitive", primitive_type: "payload" });
  });
});
