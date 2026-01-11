# Phase 007: Migrate connection.ts to Generated Types

**Status**: TODO

## Objective

Replace the hand-coded encoding functions in `roam-core/src/connection.ts` with
calls to the `roam-wire` codec functions that use **generated** types and schemas,
eliminating duplicated wire format logic.

## Background

Currently, `connection.ts` has hand-coded encode functions:

```typescript
// Current hand-coded functions in connection.ts

function encodeHello(hello: Hello): Uint8Array {
  return concat(
    encodeVarint(MSG_HELLO),
    encodeVarint(hello.variant),
    encodeVarint(hello.maxPayloadSize),
    encodeVarint(hello.initialStreamCredit),
  );
}

function encodeGoodbye(reason: string): Uint8Array {
  return concat(encodeVarint(MSG_GOODBYE), encodeString(reason));
}

function encodeResponse(requestId: bigint, payload: Uint8Array): Uint8Array {
  return concat(
    encodeVarint(MSG_RESPONSE),
    encodeVarint(requestId),
    encodeVarint(0), // empty metadata vec
    encodeBytes(payload),
  );
}

function encodeRequest(requestId: bigint, methodId: bigint, payload: Uint8Array): Uint8Array {
  return concat(
    encodeVarint(MSG_REQUEST),
    encodeVarint(requestId),
    encodeVarint(methodId),
    encodeVarint(0), // empty metadata vec
    encodeBytes(payload),
  );
}

function encodeData(streamId: bigint, payload: Uint8Array): Uint8Array {
  return concat(encodeVarint(MSG_DATA), encodeVarint(streamId), encodeBytes(payload));
}

function encodeClose(streamId: bigint): Uint8Array {
  return concat(encodeVarint(MSG_CLOSE), encodeVarint(streamId));
}
```

These should be replaced with calls to `roam-wire` functions using generated types:

```typescript
import {
  encodeMessage,
  decodeMessage,
  type Message,
  type Hello,
} from "@bearcove/roam-wire";
```

## Design

### Remove Hand-Coded Functions

Delete the following from `connection.ts`:
- `encodeHello()`
- `encodeGoodbye()`
- `encodeResponse()`
- `encodeRequest()`
- `encodeData()`
- `encodeClose()`
- `MSG_*` discriminant constants

### Replace with roam-wire Imports

```typescript
// Before
import { concat, encodeBytes, encodeString } from "../../roam-postcard/src/binary/bytes.ts";
import { encodeVarint } from "../../roam-postcard/src/binary/varint.ts";

const MSG_HELLO = 0;
const MSG_GOODBYE = 1;
// etc.

function encodeHello(hello: Hello): Uint8Array {
  // hand-coded...
}

// After
import {
  encodeMessage,
  decodeMessage,
  messageHello,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
  helloV1,
  type Message,
  type Hello,
} from "@bearcove/roam-wire";
```

### Update Call Sites

#### Hello Exchange

```typescript
// Before
await this.io.send(encodeHello({
  variant: 0,
  maxPayloadSize: ourHello.maxPayloadSize,
  initialStreamCredit: ourHello.initialStreamCredit,
}));

// After
const hello: Hello = { tag: "V1", maxPayloadSize: ourHello.maxPayloadSize, initialChannelCredit: ourHello.initialChannelCredit };
const msg: Message = { tag: "Hello", hello };
await this.io.send(encodeMessage(msg));
```

#### Goodbye

```typescript
// Before
await this.io.send(encodeGoodbye(ruleId));

// After
await this.io.send(encodeMessage({ tag: "Goodbye", reason: ruleId }));
```

#### Request/Response

```typescript
// Before
const requestBytes = encodeRequest(requestId, methodId, payload);
await this.io.send(requestBytes);

const responseBytes = encodeResponse(requestId, responsePayload);
await this.io.send(responseBytes);

// After
await this.io.send(encodeMessage({ tag: "Request", requestId, methodId, metadata: [], payload }));
await this.io.send(encodeMessage({ tag: "Response", requestId, metadata: [], payload: responsePayload }));
```

#### Channel Messages

```typescript
// Before
await this.io.send(encodeData(channelId, payload));
await this.io.send(encodeClose(channelId));

// After
await this.io.send(encodeMessage({ tag: "Data", channelId, payload }));
await this.io.send(encodeMessage({ tag: "Close", channelId }));
```

### Update Decoding

Currently, decoding is also hand-coded:

```typescript
// Before (hand-coded decode)
const msgType = decodeVarintNumber(buf, offset);
switch (msgType.value) {
  case MSG_HELLO: {
    const variant = decodeVarintNumber(buf, msgType.next);
    const maxPayloadSize = decodeVarintNumber(buf, variant.next);
    const initialCredit = decodeVarintNumber(buf, maxPayloadSize.next);
    // ...
  }
  case MSG_REQUEST: {
    const requestId = decodeVarint(buf, msgType.next);
    const methodId = decodeVarint(buf, requestId.next);
    // ...
  }
}

// After (using generated types)
const result = decodeMessage(buf, offset);
switch (result.value.tag) {
  case "Hello": {
    // TypeScript knows result.value is MessageHello
    const { maxPayloadSize, initialChannelCredit } = result.value.hello;
    // ...
  }
  case "Request": {
    // TypeScript knows result.value is MessageRequest
    const { requestId, methodId, metadata, payload } = result.value;
    // ...
  }
}
```

### Type Changes

The internal `Hello` type in connection.ts needs to align with `roam-wire` types:

```typescript
// Before (connection.ts internal type)
interface HelloV1 {
  variant: 0;
  maxPayloadSize: number;
  initialStreamCredit: number;
}

type Hello = HelloV1;

// After (use generated roam-wire types)
import type { Hello } from "@bearcove/roam-wire";
// Hello = { tag: "V1"; maxPayloadSize: number; initialChannelCredit: number }
```

Note the field name change: `initialStreamCredit` → `initialChannelCredit`
(matching Rust naming).

## Implementation Steps

1. Add `@bearcove/roam-wire` as a dependency of `roam-core` (may already exist)
2. Import wire types and codec functions
3. Remove hand-coded encode functions
4. Remove `MSG_*` constants (use `MESSAGE_DISCRIMINANT` if needed)
5. Update all encoding call sites
6. Update all decoding call sites
7. Update internal type definitions to match wire types
8. Run existing tests to verify no regressions
9. Remove unused imports

## Files to Modify

| File | Action |
|------|--------|
| `typescript/packages/roam-core/src/connection.ts` | MAJOR REFACTOR |
| `typescript/packages/roam-core/package.json` | VERIFY dependency on roam-wire |

## Dependencies

- Phase 001-006 must be complete
- `roam-wire` must export all necessary functions and types

## Success Criteria

1. ✅ All hand-coded encode functions removed from `connection.ts`
2. ✅ All encoding uses `encodeMessage()` with generated types
3. ✅ All decoding uses `decodeMessage()` with generated types
4. ✅ No `MSG_*` constants in `connection.ts`
5. ✅ Types use generated `roam-wire` definitions (no duplicate type definitions)
6. ✅ All existing tests pass
7. ✅ Wire compatibility verified (can communicate with Rust peer)

## Test Plan

### Unit Tests

Existing `connection.ts` tests should continue to pass:
- Hello exchange tests
- Message encoding tests
- Protocol error handling tests

### Integration Tests

If integration tests exist with Rust peers:
- Verify hello exchange works
- Verify request/response works
- Verify streaming (Data/Close) works

### Manual Verification

1. Run TypeScript client against Rust server
2. Run Rust client against TypeScript server
3. Verify bidirectional communication works

## Migration Checklist

- [ ] Add roam-wire import
- [ ] Remove `encodeHello()` function
- [ ] Remove `encodeGoodbye()` function
- [ ] Remove `encodeRequest()` function
- [ ] Remove `encodeResponse()` function
- [ ] Remove `encodeData()` function
- [ ] Remove `encodeClose()` function
- [ ] Remove `MSG_*` constants
- [ ] Remove internal `Hello` type definition
- [ ] Update `helloExchangeAcceptor()` to use new types
- [ ] Update `helloExchangeInitiator()` to use new types
- [ ] Update `goodbye()` method
- [ ] Update `flushOutgoing()` method
- [ ] Update request encoding in client code
- [ ] Update response encoding in server dispatch
- [ ] Update message decoding in message loop
- [ ] Run tests
- [ ] Clean up unused imports

## Potential Issues

### 1. Field Name Mismatches

The current code uses `initialStreamCredit` but Rust uses `initial_channel_credit`.
The wire types use `initialChannelCredit`. Need to update all references.

### 2. Type Representation

Current code uses `variant: 0` for Hello V1, but wire types use `tag: "V1"`.
Need to update construction and matching.

### 3. Metadata Handling

Current code always sends empty metadata (`encodeVarint(0)`). The new approach
passes an empty array `[]`. Verify this encodes identically.

### 4. BigInt vs Number

Current code may mix `bigint` and `number` for IDs. Wire types use `bigint`
consistently. Ensure call sites use `bigint`.

## Rollback Plan

If issues arise:
1. Keep old functions as `_legacyEncodeHello()` etc.
2. Add feature flag to switch between old and new
3. Compare outputs during testing
4. Remove legacy code once verified

## Notes

- This is a refactoring task - behavior should not change
- Wire format must remain identical
- Existing tests are the safety net
- If tests fail, the migration has a bug
- Consider adding temporary assertions that old and new encode identically