import { describe, expect, it } from "vitest";

import {
  MetadataFlagValues,
  connectionSettings,
  helloV7,
  helloYourself,
  metadataBytes,
  metadataEntry,
  metadataString,
  messageClose,
  messageCredit,
  messageData,
  messageHello,
  messageRequest,
  parityEven,
  parityOdd,
} from "./types.ts";
import { decodeMessage, encodeMessage } from "./codec.ts";
import { HelloSchema, MessageSchema, RequestBodySchema } from "./schemas.ts";
import { encodeU32 } from "@bearcove/roam-postcard";

describe("wire helpers", () => {
  it("builds handshake helpers with expected defaults", () => {
    const hello = helloV7(parityOdd(), 64);
    const yourself = helloYourself(parityEven(), 32);

    expect(hello).toEqual({
      version: 7,
      connection_settings: connectionSettings(parityOdd(), 64),
      metadata: [],
    });
    expect(yourself).toEqual({
      connection_settings: connectionSettings(parityEven(), 32),
      metadata: [],
    });
  });

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
    const message = messageRequest(99n, 0xE5A1_D6B2_C390_F001n, encodeU32(0x1234_5678), metadata, [
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
  it("marks opaque payload bytes as trailing", () => {
    const requestBody = RequestBodySchema as any;
    const call = requestBody.variants.find((v: any) => v.name === "Call");
    const channelBody = (MessageSchema as any).fields.payload.variants.find(
      (v: any) => v.name === "ChannelMessage",
    ).fields.fields.body;
    const item = channelBody.variants.find((v: any) => v.name === "Item");

    expect(HelloSchema.kind).toBe("struct");
    expect(call.fields.fields.args).toEqual({ kind: "bytes", trailing: true });
    expect(item.fields.fields.item).toEqual({ kind: "bytes", trailing: true });
  });

  it("encodes handshake messages with the generated schema", () => {
    const encoded = encodeMessage(messageHello(helloV7(parityOdd(), 16)));
    const decoded = decodeMessage(encoded);

    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value.payload.tag).toBe("Hello");
  });
});
