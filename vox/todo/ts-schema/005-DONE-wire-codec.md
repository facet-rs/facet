# Phase 005: Wire Type Encode/Decode Functions

**Status**: TODO

## Objective

Implement thin wrapper functions `encodeMessage()` and `decodeMessage()` in `roam-wire`
that provide a convenient API for encoding/decoding wire protocol messages using
the **generated** types and schemas from Phase 004.

## Background

With Phase 004 complete, we have:
- Generated TypeScript types in `roam-wire/src/generated/types.ts`
- Generated schemas in `roam-wire/src/generated/schemas.ts`

Now we need wrapper functions that combine these with the schema-driven
`encodeWithSchema()` and `decodeWithSchema()` from `roam-postcard`.

## Design

### Core Functions

Create `roam-wire/src/codec.ts`:

```typescript
import { encodeWithSchema, decodeWithSchema, type DecodeResult } from "@bearcove/roam-postcard";
import { MessageSchema, HelloSchema, MetadataValueSchema, wireSchemaRegistry } from "./generated/schemas.ts";
import type { Message, Hello, MetadataValue } from "./generated/types.ts";

// ============================================================================
// Message Encoding/Decoding
// ============================================================================

/**
 * Encode a Message to bytes.
 * 
 * @param message - The message to encode
 * @returns Encoded bytes in postcard format
 * @throws Error if message structure is invalid
 * 
 * @example
 * ```typescript
 * const bytes = encodeMessage({
 *   tag: "Request",
 *   requestId: 1n,
 *   methodId: 42n,
 *   metadata: [],
 *   payload: new Uint8Array([]),
 * });
 * ```
 */
export function encodeMessage(message: Message): Uint8Array {
  return encodeWithSchema(MessageSchema, message, wireSchemaRegistry);
}

/**
 * Decode bytes to a Message.
 * 
 * @param buf - The buffer to decode from
 * @param offset - Starting offset (default: 0)
 * @returns Decoded message and next offset
 * @throws Error if buffer is invalid or truncated
 * 
 * @example
 * ```typescript
 * const result = decodeMessage(bytes);
 * if (result.value.tag === "Request") {
 *   console.log(result.value.methodId);
 * }
 * ```
 */
export function decodeMessage(buf: Uint8Array, offset: number = 0): DecodeResult<Message> {
  return decodeWithSchema<Message>(MessageSchema, buf, offset, wireSchemaRegistry);
}

// ============================================================================
// Hello Encoding/Decoding (for handshake)
// ============================================================================

/**
 * Encode a Hello message to bytes.
 * 
 * Used during connection handshake. Note: This encodes just the Hello enum,
 * not wrapped in a Message. For Message::Hello, use encodeMessage().
 */
export function encodeHello(hello: Hello): Uint8Array {
  return encodeWithSchema(HelloSchema, hello, wireSchemaRegistry);
}

/**
 * Decode bytes to a Hello message.
 */
export function decodeHello(buf: Uint8Array, offset: number = 0): DecodeResult<Hello> {
  return decodeWithSchema<Hello>(HelloSchema, buf, offset, wireSchemaRegistry);
}

// ============================================================================
// MetadataValue Encoding/Decoding (for advanced use)
// ============================================================================

/**
 * Encode a MetadataValue to bytes.
 */
export function encodeMetadataValue(value: MetadataValue): Uint8Array {
  return encodeWithSchema(MetadataValueSchema, value, wireSchemaRegistry);
}

/**
 * Decode bytes to a MetadataValue.
 */
export function decodeMetadataValue(buf: Uint8Array, offset: number = 0): DecodeResult<MetadataValue> {
  return decodeWithSchema<MetadataValue>(MetadataValueSchema, buf, offset, wireSchemaRegistry);
}
```

### Re-exports

Update `roam-wire/src/index.ts`:

```typescript
// Generated types
export type { Message, Hello, MetadataValue } from "./generated/types.ts";

// Generated schemas and registry
export { MessageSchema, HelloSchema, MetadataValueSchema, wireSchemaRegistry } from "./generated/schemas.ts";

// Codec functions
export {
  encodeMessage,
  decodeMessage,
  encodeHello,
  decodeHello,
  encodeMetadataValue,
  decodeMetadataValue,
} from "./codec.ts";

// Existing RPC error types
export { RpcError, RpcErrorCode, decodeRpcResult, decodeUserError } from "./rpc_error.ts";
```

## Implementation Steps

1. Create `roam-wire/src/codec.ts` with wrapper functions
2. Update `roam-wire/src/index.ts` to export generated types and codec
3. Add unit tests for codec functions
4. Verify types flow correctly through the API

## Files to Create/Modify

| File | Action |
|------|--------|
| `typescript/packages/roam-wire/src/codec.ts` | CREATE |
| `typescript/packages/roam-wire/src/index.ts` | MODIFY (add exports) |
| `typescript/packages/roam-wire/src/codec.test.ts` | CREATE |

## Dependencies

- Phase 001-003 (Schema types and encode/decode functions)
- Phase 004 (Generated types and schemas)

## Success Criteria

1. ✅ `encodeMessage()` compiles and produces bytes
2. ✅ `decodeMessage()` compiles and produces typed messages
3. ✅ Functions are exported from `roam-wire` package
4. ✅ Type inference works:
   - `decodeMessage()` returns `DecodeResult<Message>`
   - After narrowing with `msg.tag === "Request"`, TypeScript knows the fields
5. ✅ Roundtrip works: `decodeMessage(encodeMessage(msg)).value` equals `msg`

## Test Cases

```typescript
import { describe, it, expect } from "vitest";
import {
  encodeMessage,
  decodeMessage,
  encodeHello,
  decodeHello,
  type Message,
  type Hello,
} from "@bearcove/roam-wire";

describe("Message codec", () => {
  it("roundtrips Hello message", () => {
    const msg: Message = {
      tag: "Hello",
      hello: { tag: "V1", maxPayloadSize: 1024, initialChannelCredit: 64 },
    };
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value).toEqual(msg);
    expect(decoded.next).toBe(encoded.length);
  });

  it("roundtrips Goodbye message", () => {
    const msg: Message = { tag: "Goodbye", reason: "test shutdown" };
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value).toEqual(msg);
  });

  it("roundtrips Request message", () => {
    const msg: Message = {
      tag: "Request",
      requestId: 1n,
      methodId: 42n,
      metadata: [["key", { tag: "String", value: "val" }]],
      payload: new Uint8Array([0xDE, 0xAD]),
    };
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);
    expect(decoded.value.tag).toBe("Request");
    if (decoded.value.tag === "Request") {
      expect(decoded.value.requestId).toBe(1n);
      expect(decoded.value.methodId).toBe(42n);
    }
  });

  it("roundtrips all channel messages", () => {
    const messages: Message[] = [
      { tag: "Cancel", requestId: 99n },
      { tag: "Data", channelId: 1n, payload: new Uint8Array([1, 2, 3]) },
      { tag: "Close", channelId: 7n },
      { tag: "Reset", channelId: 5n },
      { tag: "Credit", channelId: 3n, bytes: 4096 },
    ];

    for (const msg of messages) {
      const encoded = encodeMessage(msg);
      const decoded = decodeMessage(encoded);
      expect(decoded.value).toEqual(msg);
    }
  });
});

describe("Hello codec", () => {
  it("roundtrips Hello V1", () => {
    const hello: Hello = { tag: "V1", maxPayloadSize: 1024 * 1024, initialChannelCredit: 64 * 1024 };
    const encoded = encodeHello(hello);
    const decoded = decodeHello(encoded);
    expect(decoded.value).toEqual(hello);
  });
});
```

## Notes

- These are intentionally thin wrappers - the real work is in `encodeWithSchema`/`decodeWithSchema`
- The generated types provide type safety
- The generated schemas provide wire format correctness
- The `wireSchemaRegistry` is passed to resolve any `RefSchema` references
- This layer just connects them together with a clean API