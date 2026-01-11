// Golden vector tests for wire compatibility with Rust
//
// These tests verify that TypeScript encoding/decoding produces bytes
// identical to Rust's facet_postcard serialization.

import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import {
  type Hello,
  type MetadataValue,
  type MetadataEntry,
  type Message,
  helloV1,
  metadataString,
  messageHello,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
  encodeHello,
  decodeHello,
  encodeMessage,
  decodeMessage,
} from "./index.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Load a golden vector from the test-fixtures directory.
 */
function loadGoldenVector(path: string): Uint8Array {
  // Navigate from roam/typescript/packages/roam-wire/src to roam/test-fixtures
  // Path: src -> roam-wire -> packages -> typescript -> roam (project root)
  const projectRoot = join(__dirname, "..", "..", "..", "..");
  const vectorPath = join(projectRoot, "test-fixtures", "golden-vectors", path);
  return new Uint8Array(readFileSync(vectorPath));
}

/**
 * Format bytes as hex string for debugging.
 */
function hexDump(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join(" ");
}

// ============================================================================
// Hello Golden Vector Tests
// ============================================================================

describe("Hello golden vectors", () => {
  it("encodes Hello V1 (small values) matching Rust", () => {
    const hello = helloV1(1024, 64);
    const encoded = encodeHello(hello);
    const expected = loadGoldenVector("wire/hello_v1_small.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("encodes Hello V1 (typical values) matching Rust", () => {
    const hello = helloV1(1024 * 1024, 64 * 1024);
    const encoded = encodeHello(hello);
    const expected = loadGoldenVector("wire/hello_v1_typical.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Hello V1 (small) from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/hello_v1_small.bin");
    const decoded = decodeHello(bytes);
    expect(decoded.value).toEqual(helloV1(1024, 64));
    expect(decoded.next).toBe(bytes.length);
  });

  it("decodes Hello V1 (typical) from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/hello_v1_typical.bin");
    const decoded = decodeHello(bytes);
    expect(decoded.value).toEqual(helloV1(1024 * 1024, 64 * 1024));
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Hello Golden Vector Tests
// ============================================================================

describe("Message::Hello golden vectors", () => {
  it("encodes Message::Hello (small) matching Rust", () => {
    const msg = messageHello(helloV1(1024, 64));
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_hello_small.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("encodes Message::Hello (typical) matching Rust", () => {
    const msg = messageHello(helloV1(1024 * 1024, 64 * 1024));
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_hello_typical.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Message::Hello from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_hello_typical.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value.tag).toBe("Hello");
    if (decoded.value.tag === "Hello") {
      expect(decoded.value.value).toEqual(helloV1(1024 * 1024, 64 * 1024));
    }
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Goodbye Golden Vector Tests
// ============================================================================

describe("Message::Goodbye golden vectors", () => {
  it("encodes Message::Goodbye matching Rust", () => {
    const msg = messageGoodbye("test");
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_goodbye.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Message::Goodbye from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_goodbye.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value).toEqual(messageGoodbye("test"));
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Request Golden Vector Tests
// ============================================================================

describe("Message::Request golden vectors", () => {
  it("encodes empty Request matching Rust", () => {
    const msg = messageRequest(1n, 42n, new Uint8Array([]));
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_request_empty.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("encodes Request with payload matching Rust", () => {
    const msg = messageRequest(1n, 42n, new Uint8Array([0xde, 0xad, 0xbe, 0xef]));
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_request_with_payload.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("encodes Request with metadata matching Rust", () => {
    const metadata: MetadataEntry[] = [["key", metadataString("value")]];
    const msg = messageRequest(5n, 100n, new Uint8Array([]), metadata);
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_request_with_metadata.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Request from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_request_with_metadata.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value.tag).toBe("Request");
    if (decoded.value.tag === "Request") {
      expect(decoded.value.requestId).toBe(5n);
      expect(decoded.value.methodId).toBe(100n);
      expect(decoded.value.metadata.length).toBe(1);
      expect(decoded.value.metadata[0][0]).toBe("key");
      expect(decoded.value.metadata[0][1]).toEqual(metadataString("value"));
    }
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Response Golden Vector Tests
// ============================================================================

describe("Message::Response golden vectors", () => {
  it("encodes Response matching Rust", () => {
    const msg = messageResponse(1n, new Uint8Array([0x42]));
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_response.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Response from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_response.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value.tag).toBe("Response");
    if (decoded.value.tag === "Response") {
      expect(decoded.value.requestId).toBe(1n);
      expect(Array.from(decoded.value.payload)).toEqual([0x42]);
    }
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Cancel Golden Vector Tests
// ============================================================================

describe("Message::Cancel golden vectors", () => {
  it("encodes Cancel matching Rust", () => {
    const msg = messageCancel(99n);
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_cancel.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Cancel from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_cancel.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value).toEqual(messageCancel(99n));
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Data Golden Vector Tests
// ============================================================================

describe("Message::Data golden vectors", () => {
  it("encodes Data matching Rust", () => {
    const msg = messageData(1n, new Uint8Array([1, 2, 3]));
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_data.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Data from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_data.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value.tag).toBe("Data");
    if (decoded.value.tag === "Data") {
      expect(decoded.value.channelId).toBe(1n);
      expect(Array.from(decoded.value.payload)).toEqual([1, 2, 3]);
    }
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Close Golden Vector Tests
// ============================================================================

describe("Message::Close golden vectors", () => {
  it("encodes Close matching Rust", () => {
    const msg = messageClose(7n);
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_close.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Close from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_close.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value).toEqual(messageClose(7n));
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Reset Golden Vector Tests
// ============================================================================

describe("Message::Reset golden vectors", () => {
  it("encodes Reset matching Rust", () => {
    const msg = messageReset(5n);
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_reset.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Reset from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_reset.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value).toEqual(messageReset(5n));
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Message::Credit Golden Vector Tests
// ============================================================================

describe("Message::Credit golden vectors", () => {
  it("encodes Credit matching Rust", () => {
    const msg = messageCredit(3n, 4096);
    const encoded = encodeMessage(msg);
    const expected = loadGoldenVector("wire/message_credit.bin");

    if (!arraysEqual(encoded, expected)) {
      console.log("Expected:", hexDump(expected));
      console.log("Actual:  ", hexDump(encoded));
    }

    expect(Array.from(encoded)).toEqual(Array.from(expected));
  });

  it("decodes Credit from Rust bytes", () => {
    const bytes = loadGoldenVector("wire/message_credit.bin");
    const decoded = decodeMessage(bytes);
    expect(decoded.value).toEqual(messageCredit(3n, 4096));
    expect(decoded.next).toBe(bytes.length);
  });
});

// ============================================================================
// Helper Functions
// ============================================================================

function arraysEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}
