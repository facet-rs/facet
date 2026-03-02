import { describe, expect, it } from "vitest";

import {
  type Message,
  MessageDiscriminant,
  MetadataValueDiscriminant,
  HelloDiscriminant,
  MetadataFlags,
  parityOdd,
  parityEven,
  connectionSettings,
  helloV7,
  helloYourself,
  metadataString,
  metadataBytes,
  metadataU64,
  metadataEntry,
  messageHello,
  messagePing,
  messagePong,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
} from "./types.ts";
import { HelloSchema, RequestBodySchema, MessageSchema } from "./schemas.ts";
import {
  encodeMessage,
  decodeMessage,
} from "./codec.ts";
import { encodeU32 } from "@bearcove/roam-postcard";

describe("wire discriminants", () => {
  it("has correct v7 message discriminants", () => {
    expect(MessageDiscriminant.Hello).toBe(0);
    expect(MessageDiscriminant.HelloYourself).toBe(1);
    expect(MessageDiscriminant.ProtocolError).toBe(2);
    expect(MessageDiscriminant.ConnectionOpen).toBe(3);
    expect(MessageDiscriminant.ConnectionAccept).toBe(4);
    expect(MessageDiscriminant.ConnectionReject).toBe(5);
    expect(MessageDiscriminant.ConnectionClose).toBe(6);
    expect(MessageDiscriminant.RequestMessage).toBe(7);
    expect(MessageDiscriminant.ChannelMessage).toBe(8);
    expect(MessageDiscriminant.Ping).toBe(9);
    expect(MessageDiscriminant.Pong).toBe(10);
  });

  it("has correct metadata and hello discriminants", () => {
    expect(MetadataValueDiscriminant.String).toBe(0);
    expect(MetadataValueDiscriminant.Bytes).toBe(1);
    expect(MetadataValueDiscriminant.U64).toBe(2);
    expect(HelloDiscriminant.V7).toBe(7);
  });
});

describe("factory helpers", () => {
  it("builds hello and hello-yourself", () => {
    const hello = helloV7(parityOdd(), 64);
    expect(hello.version).toBe(7);
    expect(hello.connection_settings).toEqual(connectionSettings(parityOdd(), 64));

    const hy = helloYourself(parityEven(), 32);
    expect(hy.connection_settings).toEqual(connectionSettings(parityEven(), 32));
  });

  it("builds nested request/channel/control messages", () => {
    const meta = [metadataEntry("k", metadataString("v"), MetadataFlags.NONE)];

    const request = messageRequest(11n, 42n, new Uint8Array([1, 2]), meta, [3n], 2n);
    expect(request.connection_id).toBe(2n);
    expect(request.payload.tag).toBe("RequestMessage");

    const response = messageResponse(11n, new Uint8Array([9]), meta, [7n], 2n);
    expect(response.payload.tag).toBe("RequestMessage");

    const cancel = messageCancel(11n, 2n, meta);
    expect(cancel.payload.tag).toBe("RequestMessage");

    const data = messageData(5n, new Uint8Array([8, 7]), 2n);
    const close = messageClose(5n, 2n, meta);
    const reset = messageReset(5n, 2n, meta);
    const credit = messageCredit(5n, 1024, 2n);
    expect(data.payload.tag).toBe("ChannelMessage");
    expect(close.payload.tag).toBe("ChannelMessage");
    expect(reset.payload.tag).toBe("ChannelMessage");
    expect(credit.payload.tag).toBe("ChannelMessage");

    const hello = messageHello(helloV7(parityOdd(), 64, meta));
    const ping = messagePing(123n);
    const pong = messagePong(123n);
    const goodbye = messageGoodbye(2n, meta);
    expect(hello.payload.tag).toBe("Hello");
    expect(ping.payload.tag).toBe("Ping");
    expect(pong.payload.tag).toBe("Pong");
    expect(goodbye.payload.tag).toBe("ConnectionClose");
  });
});

describe("schema-driven codec", () => {
  it("roundtrips message with nested request body", () => {
    const metadata = [
      metadataEntry("trace-id", metadataString("abc123"), MetadataFlags.NONE),
      metadataEntry("auth", metadataBytes(new Uint8Array([0xde, 0xad, 0xbe, 0xef])), 3n),
      metadataEntry("attempt", metadataU64(2n), MetadataFlags.NONE),
    ];

    const args = encodeU32(0x1234_5678);
    const message = messageRequest(11n, 0xE5A1_D6B2_C390_F001n, args, metadata, [3n, 5n], 2n);

    const encoded = encodeMessage(message);
    const decoded = decodeMessage(encoded);

    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual(message);
  });

  it("roundtrips multiple messages individually", () => {
    const messages: Message[] = [
      messageHello(helloV7(parityOdd(), 64)),
      messagePing(7n),
      messagePong(7n),
      messageGoodbye(2n, []),
      messageData(3n, new Uint8Array([0x4d]), 2n),
    ];

    for (const message of messages) {
      const encoded = encodeMessage(message);
      const decoded = decodeMessage(encoded);
      expect(decoded.next).toBe(encoded.length);
      expect(decoded.value).toEqual(message);
    }
  });
});

describe("generated wire schemas", () => {
  it("has v7 hello schema as struct", () => {
    expect(HelloSchema.kind).toBe("struct");
  });

  it("marks opaque payload bytes as trailing", () => {
    const requestBody = RequestBodySchema as any;
    const call = requestBody.variants.find((v: any) => v.name === "Call");
    const response = requestBody.variants.find((v: any) => v.name === "Response");
    const channelBody = (MessageSchema as any).fields.payload.variants.find(
      (v: any) => v.name === "ChannelMessage",
    ).fields.fields.body;
    const item = channelBody.variants.find((v: any) => v.name === "Item");

    expect(call.fields.fields.args).toEqual({ kind: "bytes", trailing: true });
    expect(response.fields.fields.ret).toEqual({ kind: "bytes", trailing: true });
    expect(item.fields.fields.item).toEqual({ kind: "bytes", trailing: true });
  });
});
