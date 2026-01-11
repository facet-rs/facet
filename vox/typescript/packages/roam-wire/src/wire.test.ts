// Comprehensive tests for wire types, schemas, and codec

import { describe, it, expect } from "vitest";
import {
  // Types
  type Hello,
  type MetadataValue,
  type MetadataEntry,
  type Message,
  // Discriminants
  MessageDiscriminant,
  MetadataValueDiscriminant,
  HelloDiscriminant,
  // Factory functions
  helloV1,
  metadataString,
  metadataBytes,
  metadataU64,
  messageHello,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
  // Schemas
  HelloSchema,
  MetadataValueSchema,
  MetadataEntrySchema,
  MessageSchema,
  wireSchemaRegistry,
  // Codec
  encodeHello,
  decodeHello,
  encodeMetadataValue,
  decodeMetadataValue,
  encodeMetadataEntry,
  decodeMetadataEntry,
  encodeMessage,
  decodeMessage,
  encodeMessages,
  decodeMessages,
} from "./index.ts";

// ============================================================================
// Discriminant Tests
// ============================================================================

describe("wire discriminants", () => {
  it("has correct Message discriminants", () => {
    expect(MessageDiscriminant.Hello).toBe(0);
    expect(MessageDiscriminant.Goodbye).toBe(1);
    expect(MessageDiscriminant.Request).toBe(2);
    expect(MessageDiscriminant.Response).toBe(3);
    expect(MessageDiscriminant.Cancel).toBe(4);
    expect(MessageDiscriminant.Data).toBe(5);
    expect(MessageDiscriminant.Close).toBe(6);
    expect(MessageDiscriminant.Reset).toBe(7);
    expect(MessageDiscriminant.Credit).toBe(8);
  });

  it("has correct MetadataValue discriminants", () => {
    expect(MetadataValueDiscriminant.String).toBe(0);
    expect(MetadataValueDiscriminant.Bytes).toBe(1);
    expect(MetadataValueDiscriminant.U64).toBe(2);
  });

  it("has correct Hello discriminants", () => {
    expect(HelloDiscriminant.V1).toBe(0);
  });
});

// ============================================================================
// Factory Function Tests
// ============================================================================

describe("factory functions", () => {
  describe("Hello", () => {
    it("creates Hello.V1", () => {
      const hello = helloV1(65536, 1024);
      expect(hello.tag).toBe("V1");
      expect(hello.maxPayloadSize).toBe(65536);
      expect(hello.initialChannelCredit).toBe(1024);
    });
  });

  describe("MetadataValue", () => {
    it("creates MetadataValue.String", () => {
      const value = metadataString("hello");
      expect(value.tag).toBe("String");
      expect(value.value).toBe("hello");
    });

    it("creates MetadataValue.Bytes", () => {
      const bytes = new Uint8Array([1, 2, 3]);
      const value = metadataBytes(bytes);
      expect(value.tag).toBe("Bytes");
      expect(value.value).toEqual(bytes);
    });

    it("creates MetadataValue.U64", () => {
      const value = metadataU64(12345n);
      expect(value.tag).toBe("U64");
      expect(value.value).toBe(12345n);
    });
  });

  describe("Message", () => {
    it("creates Message.Hello", () => {
      const hello = helloV1(65536, 1024);
      const msg = messageHello(hello);
      expect(msg.tag).toBe("Hello");
      if (msg.tag === "Hello") {
        expect(msg.value).toEqual(hello);
      }
    });

    it("creates Message.Goodbye", () => {
      const msg = messageGoodbye("shutting down");
      expect(msg.tag).toBe("Goodbye");
      if (msg.tag === "Goodbye") {
        expect(msg.reason).toBe("shutting down");
      }
    });

    it("creates Message.Request with default empty metadata", () => {
      const msg = messageRequest(1n, 0x123456n, new Uint8Array([1, 2, 3]));
      expect(msg.tag).toBe("Request");
      if (msg.tag === "Request") {
        expect(msg.requestId).toBe(1n);
        expect(msg.methodId).toBe(0x123456n);
        expect(msg.payload).toEqual(new Uint8Array([1, 2, 3]));
        expect(msg.metadata).toEqual([]);
      }
    });

    it("creates Message.Request with metadata", () => {
      const metadata: MetadataEntry[] = [["key", metadataString("value")]];
      const msg = messageRequest(1n, 0x123456n, new Uint8Array([1, 2, 3]), metadata);
      if (msg.tag === "Request") {
        expect(msg.metadata).toEqual(metadata);
      }
    });

    it("creates Message.Response", () => {
      const msg = messageResponse(1n, new Uint8Array([4, 5, 6]));
      expect(msg.tag).toBe("Response");
      if (msg.tag === "Response") {
        expect(msg.requestId).toBe(1n);
        expect(msg.payload).toEqual(new Uint8Array([4, 5, 6]));
        expect(msg.metadata).toEqual([]);
      }
    });

    it("creates Message.Cancel", () => {
      const msg = messageCancel(42n);
      expect(msg.tag).toBe("Cancel");
      if (msg.tag === "Cancel") {
        expect(msg.requestId).toBe(42n);
      }
    });

    it("creates Message.Data", () => {
      const msg = messageData(100n, new Uint8Array([1, 2, 3]));
      expect(msg.tag).toBe("Data");
      if (msg.tag === "Data") {
        expect(msg.channelId).toBe(100n);
        expect(msg.payload).toEqual(new Uint8Array([1, 2, 3]));
      }
    });

    it("creates Message.Close", () => {
      const msg = messageClose(100n);
      expect(msg.tag).toBe("Close");
      if (msg.tag === "Close") {
        expect(msg.channelId).toBe(100n);
      }
    });

    it("creates Message.Reset", () => {
      const msg = messageReset(100n);
      expect(msg.tag).toBe("Reset");
      if (msg.tag === "Reset") {
        expect(msg.channelId).toBe(100n);
      }
    });

    it("creates Message.Credit", () => {
      const msg = messageCredit(100n, 65536);
      expect(msg.tag).toBe("Credit");
      if (msg.tag === "Credit") {
        expect(msg.channelId).toBe(100n);
        expect(msg.bytes).toBe(65536);
      }
    });
  });
});

// ============================================================================
// Schema Tests
// ============================================================================

describe("wire schemas", () => {
  it("HelloSchema has correct structure", () => {
    expect(HelloSchema.kind).toBe("enum");
    expect(HelloSchema.variants).toHaveLength(1);
    expect(HelloSchema.variants[0].name).toBe("V1");
    expect(HelloSchema.variants[0].discriminant).toBe(0);
  });

  it("MetadataValueSchema has correct structure", () => {
    expect(MetadataValueSchema.kind).toBe("enum");
    expect(MetadataValueSchema.variants).toHaveLength(3);
    expect(MetadataValueSchema.variants[0].name).toBe("String");
    expect(MetadataValueSchema.variants[0].discriminant).toBe(0);
    expect(MetadataValueSchema.variants[1].name).toBe("Bytes");
    expect(MetadataValueSchema.variants[1].discriminant).toBe(1);
    expect(MetadataValueSchema.variants[2].name).toBe("U64");
    expect(MetadataValueSchema.variants[2].discriminant).toBe(2);
  });

  it("MetadataEntrySchema has correct structure", () => {
    expect(MetadataEntrySchema.kind).toBe("tuple");
    expect(MetadataEntrySchema.elements).toHaveLength(2);
    expect(MetadataEntrySchema.elements[0]).toEqual({ kind: "string" });
    expect(MetadataEntrySchema.elements[1]).toEqual({ kind: "ref", name: "MetadataValue" });
  });

  it("MessageSchema has correct structure", () => {
    expect(MessageSchema.kind).toBe("enum");
    expect(MessageSchema.variants).toHaveLength(9);

    // Check all variant names and discriminants
    const variants = MessageSchema.variants;
    expect(variants[0].name).toBe("Hello");
    expect(variants[0].discriminant).toBe(0);
    expect(variants[1].name).toBe("Goodbye");
    expect(variants[1].discriminant).toBe(1);
    expect(variants[2].name).toBe("Request");
    expect(variants[2].discriminant).toBe(2);
    expect(variants[3].name).toBe("Response");
    expect(variants[3].discriminant).toBe(3);
    expect(variants[4].name).toBe("Cancel");
    expect(variants[4].discriminant).toBe(4);
    expect(variants[5].name).toBe("Data");
    expect(variants[5].discriminant).toBe(5);
    expect(variants[6].name).toBe("Close");
    expect(variants[6].discriminant).toBe(6);
    expect(variants[7].name).toBe("Reset");
    expect(variants[7].discriminant).toBe(7);
    expect(variants[8].name).toBe("Credit");
    expect(variants[8].discriminant).toBe(8);
  });

  it("wireSchemaRegistry contains all wire types", () => {
    expect(wireSchemaRegistry.has("Hello")).toBe(true);
    expect(wireSchemaRegistry.has("MetadataValue")).toBe(true);
    expect(wireSchemaRegistry.has("MetadataEntry")).toBe(true);
    expect(wireSchemaRegistry.has("Message")).toBe(true);
  });
});

// ============================================================================
// Hello Codec Tests
// ============================================================================

describe("Hello codec", () => {
  it("roundtrips Hello.V1", () => {
    const hello = helloV1(65536, 1024);
    const encoded = encodeHello(hello);
    const decoded = decodeHello(encoded);
    expect(decoded.value).toEqual(hello);
    expect(decoded.next).toBe(encoded.length);
  });

  it("roundtrips Hello.V1 with different values", () => {
    const testCases = [
      { maxPayloadSize: 0, initialChannelCredit: 0 },
      { maxPayloadSize: 1, initialChannelCredit: 1 },
      { maxPayloadSize: 0xffffffff, initialChannelCredit: 0xffffffff },
      { maxPayloadSize: 1024 * 1024, initialChannelCredit: 10000 },
    ];

    for (const { maxPayloadSize, initialChannelCredit } of testCases) {
      const hello = helloV1(maxPayloadSize, initialChannelCredit);
      const encoded = encodeHello(hello);
      const decoded = decodeHello(encoded);
      expect(decoded.value).toEqual(hello);
    }
  });

  it("encodes Hello.V1 with discriminant 0", () => {
    const hello = helloV1(65536, 1024);
    const encoded = encodeHello(hello);
    expect(encoded[0]).toBe(0); // First byte is discriminant
  });
});

// ============================================================================
// MetadataValue Codec Tests
// ============================================================================

describe("MetadataValue codec", () => {
  it("roundtrips MetadataValue.String", () => {
    const value = metadataString("hello world");
    const encoded = encodeMetadataValue(value);
    const decoded = decodeMetadataValue(encoded);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips MetadataValue.String with empty string", () => {
    const value = metadataString("");
    const encoded = encodeMetadataValue(value);
    const decoded = decodeMetadataValue(encoded);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips MetadataValue.String with unicode", () => {
    const value = metadataString("こんにちは 🎉");
    const encoded = encodeMetadataValue(value);
    const decoded = decodeMetadataValue(encoded);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips MetadataValue.Bytes", () => {
    const value = metadataBytes(new Uint8Array([1, 2, 3, 4, 5]));
    const encoded = encodeMetadataValue(value);
    const decoded = decodeMetadataValue(encoded);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips MetadataValue.Bytes empty", () => {
    const value = metadataBytes(new Uint8Array([]));
    const encoded = encodeMetadataValue(value);
    const decoded = decodeMetadataValue(encoded);
    expect(decoded.value).toEqual(value);
  });

  it("roundtrips MetadataValue.U64", () => {
    const testValues = [0n, 1n, 255n, 65535n, 0xffffffffn, 0xffffffffffffffffn];
    for (const n of testValues) {
      const value = metadataU64(n);
      const encoded = encodeMetadataValue(value);
      const decoded = decodeMetadataValue(encoded);
      expect(decoded.value).toEqual(value);
    }
  });

  it("encodes with correct discriminants", () => {
    expect(encodeMetadataValue(metadataString("test"))[0]).toBe(0);
    expect(encodeMetadataValue(metadataBytes(new Uint8Array([])))[0]).toBe(1);
    expect(encodeMetadataValue(metadataU64(0n))[0]).toBe(2);
  });
});

// ============================================================================
// MetadataEntry Codec Tests
// ============================================================================

describe("MetadataEntry codec", () => {
  it("roundtrips entry with String value", () => {
    const entry: MetadataEntry = ["content-type", metadataString("application/json")];
    const encoded = encodeMetadataEntry(entry);
    const decoded = decodeMetadataEntry(encoded);
    expect(decoded.value).toEqual(entry);
  });

  it("roundtrips entry with Bytes value", () => {
    const entry: MetadataEntry = [
      "binary-data",
      metadataBytes(new Uint8Array([0xde, 0xad, 0xbe, 0xef])),
    ];
    const encoded = encodeMetadataEntry(entry);
    const decoded = decodeMetadataEntry(encoded);
    expect(decoded.value).toEqual(entry);
  });

  it("roundtrips entry with U64 value", () => {
    const entry: MetadataEntry = ["content-length", metadataU64(1024n)];
    const encoded = encodeMetadataEntry(entry);
    const decoded = decodeMetadataEntry(encoded);
    expect(decoded.value).toEqual(entry);
  });
});

// ============================================================================
// Message Codec Tests
// ============================================================================

describe("Message codec", () => {
  describe("Message.Hello", () => {
    it("roundtrips", () => {
      const msg = messageHello(helloV1(65536, 1024));
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 0", () => {
      const msg = messageHello(helloV1(65536, 1024));
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(0);
    });
  });

  describe("Message.Goodbye", () => {
    it("roundtrips", () => {
      const msg = messageGoodbye("server shutting down");
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("roundtrips with empty reason", () => {
      const msg = messageGoodbye("");
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 1", () => {
      const msg = messageGoodbye("bye");
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(1);
    });
  });

  describe("Message.Request", () => {
    it("roundtrips with empty metadata", () => {
      const msg = messageRequest(1n, 0x123456789abcdef0n, new Uint8Array([1, 2, 3, 4]));
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("roundtrips with metadata", () => {
      const metadata: MetadataEntry[] = [
        ["key1", metadataString("value1")],
        ["key2", metadataU64(42n)],
        ["key3", metadataBytes(new Uint8Array([1, 2, 3]))],
      ];
      const msg = messageRequest(1n, 0x123456789abcdef0n, new Uint8Array([1, 2, 3, 4]), metadata);
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("roundtrips with empty payload", () => {
      const msg = messageRequest(100n, 200n, new Uint8Array([]));
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 2", () => {
      const msg = messageRequest(1n, 2n, new Uint8Array([]));
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(2);
    });
  });

  describe("Message.Response", () => {
    it("roundtrips with empty metadata", () => {
      const msg = messageResponse(1n, new Uint8Array([5, 6, 7, 8]));
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("roundtrips with metadata", () => {
      const metadata: MetadataEntry[] = [["status", metadataString("ok")]];
      const msg = messageResponse(1n, new Uint8Array([5, 6, 7, 8]), metadata);
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 3", () => {
      const msg = messageResponse(1n, new Uint8Array([]));
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(3);
    });
  });

  describe("Message.Cancel", () => {
    it("roundtrips", () => {
      const msg = messageCancel(42n);
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 4", () => {
      const msg = messageCancel(1n);
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(4);
    });
  });

  describe("Message.Data", () => {
    it("roundtrips", () => {
      const msg = messageData(100n, new Uint8Array([0xde, 0xad, 0xbe, 0xef]));
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("roundtrips with empty payload", () => {
      const msg = messageData(100n, new Uint8Array([]));
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 5", () => {
      const msg = messageData(1n, new Uint8Array([]));
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(5);
    });
  });

  describe("Message.Close", () => {
    it("roundtrips", () => {
      const msg = messageClose(100n);
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 6", () => {
      const msg = messageClose(1n);
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(6);
    });
  });

  describe("Message.Reset", () => {
    it("roundtrips", () => {
      const msg = messageReset(100n);
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 7", () => {
      const msg = messageReset(1n);
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(7);
    });
  });

  describe("Message.Credit", () => {
    it("roundtrips", () => {
      const msg = messageCredit(100n, 65536);
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    });

    it("encodes with discriminant 8", () => {
      const msg = messageCredit(1n, 0);
      const encoded = encodeMessage(msg);
      expect(encoded[0]).toBe(8);
    });
  });
});

// ============================================================================
// Multiple Messages Tests
// ============================================================================

describe("multiple messages codec", () => {
  it("encodes and decodes empty array", () => {
    const messages: Message[] = [];
    const encoded = encodeMessages(messages);
    expect(encoded.length).toBe(0);
    const decoded = decodeMessages(encoded);
    expect(decoded).toEqual([]);
  });

  it("encodes and decodes single message", () => {
    const messages = [messageGoodbye("bye")];
    const encoded = encodeMessages(messages);
    const decoded = decodeMessages(encoded);
    expect(decoded).toEqual(messages);
  });

  it("encodes and decodes multiple messages", () => {
    const messages: Message[] = [
      messageHello(helloV1(65536, 1024)),
      messageRequest(1n, 100n, new Uint8Array([1, 2, 3])),
      messageResponse(1n, new Uint8Array([4, 5, 6])),
      messageData(10n, new Uint8Array([7, 8, 9])),
      messageClose(10n),
      messageGoodbye("done"),
    ];
    const encoded = encodeMessages(messages);
    const decoded = decodeMessages(encoded);
    expect(decoded).toEqual(messages);
  });

  it("encodes and decodes all message types", () => {
    const messages: Message[] = [
      messageHello(helloV1(1000, 100)),
      messageGoodbye("reason"),
      messageRequest(1n, 2n, new Uint8Array([1])),
      messageResponse(1n, new Uint8Array([2])),
      messageCancel(1n),
      messageData(1n, new Uint8Array([3])),
      messageClose(1n),
      messageReset(1n),
      messageCredit(1n, 1000),
    ];
    const encoded = encodeMessages(messages);
    const decoded = decodeMessages(encoded);
    expect(decoded).toEqual(messages);
  });
});

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

describe("edge cases", () => {
  it("handles large payloads", () => {
    const largePayload = new Uint8Array(100000);
    for (let i = 0; i < largePayload.length; i++) {
      largePayload[i] = i % 256;
    }
    const msg = messageData(1n, largePayload);
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value).toEqual(msg);
  });

  it("handles max u64 values", () => {
    const maxU64 = 0xffffffffffffffffn;
    const msg = messageRequest(maxU64, maxU64, new Uint8Array([]));
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value).toEqual(msg);
  });

  it("handles max u32 values", () => {
    const msg = messageCredit(1n, 0xffffffff);
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value).toEqual(msg);
  });

  it("decodes at non-zero offset", () => {
    const msg = messageGoodbye("test");
    const encoded = encodeMessage(msg);

    // Prepend some garbage bytes
    const padded = new Uint8Array(encoded.length + 5);
    padded.set([0xff, 0xff, 0xff, 0xff, 0xff], 0);
    padded.set(encoded, 5);

    const decoded = decodeMessage(padded, 5);
    expect(decoded.value).toEqual(msg);
    expect(decoded.next).toBe(padded.length);
  });

  it("handles many metadata entries", () => {
    const metadata: MetadataEntry[] = [];
    for (let i = 0; i < 100; i++) {
      metadata.push([`key-${i}`, metadataString(`value-${i}`)]);
    }
    const msg = messageRequest(1n, 2n, new Uint8Array([]), metadata);
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value).toEqual(msg);
  });
});
