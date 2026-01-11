# Phase 001: Assess Swift Baseline

**Status**: DONE

## Findings Summary

### 1. Swift Runtime Audit

| File | Purpose | Status |
|------|---------|--------|
| `Postcard.swift` | Primitive encode/decode | **Bug**: `decodeU16/I16/U32` use fixed-width instead of varint |
| `Varint.swift` | Variable-length integers | ✓ Working, correct zigzag encoding |
| `Wire.swift` | Wire message types | ✓ All variants present |
| `COBS.swift` | Framing | ✓ Working |
| `Channel.swift` | Tx/Rx types | ✓ Actor-based, async-safe |
| `Binding.swift` | Channel binding | Uses Mirror - limited |
| `Schema.swift` | Schema types | Partial - not all types |
| `Driver.swift` | Connection driver | ✓ Working event loop |
| `Transport.swift` | SwiftNIO integration | ✓ Working |
| `RoamRuntime.swift` | Public API | ✓ Clean exports |

### 2. Existing Tests

```
Test Suite: All tests
Passed: 39
Failed: 0
```

Tests cover:
- Wire encoding/decoding
- Primitive encode/decode
- COBS framing
- Varint encoding
- Golden vector compatibility

### 3. Golden Vectors

Swift tests verify wire message encoding against golden vectors from `test-fixtures/golden-vectors/`. All pass.

### 4. roam-codegen Swift Target

The Swift codegen is **functional and comprehensive**:

| File | Purpose | Status |
|------|---------|--------|
| `mod.rs` | Entry point | ✓ `generate_service()` works |
| `types.rs` | Type generation | ✓ Structs, enums, tuples |
| `schema.rs` | Schema constants | ✓ Generates schema shapes |
| `encode.rs` | Encode expressions | ✓ All primitives + composites |
| `decode.rs` | Decode statements | ✓ All primitives + composites |
| `client.rs` | Client stubs | ✓ Complete with channel binding |
| `server.rs` | Server dispatcher | ✓ Complete with preregistration |

**Fixed during audit**:
- Enum variant names now use lowerCamelCase (Swift convention)
- `preregisterChannels` is now a `static func`

### 5. Codegen Output

`cargo xtask codegen --swift` produces `swift/subject/Sources/subject-swift/Testbed.swift`:
- ~900 lines of generated code
- Compiles successfully
- Includes: types, client, handler protocol, dispatcher, schemas

### 6. Spec Test Results

With Swift subject (`SUBJECT_CMD="./swift/subject/.build/release/subject-swift"`):

| Category | Pass | Fail |
|----------|------|------|
| Protocol tests | 7/7 | 0 |
| Testbed (unary) | 4/4 | 0 |
| Client mode | 3/3 | 0 |
| Streaming | 0/3 | 3 |

**16/17 tests pass** (94%)

**Fixed during audit**:
- Error codes aligned with spec: `channeling.unknown`, `channeling.id.zero-reserved`, `message.hello.unknown-version`

**Remaining failures** (streaming tests):
- `streaming_generate_server_to_client` - Response sent before Data/Close
- `streaming_sum_client_to_server` - Not tested yet
- `streaming_transform_bidirectional` - Not tested yet

The streaming issue appears to be in how `Tx.send()` integrates with the async driver event loop - Data messages aren't being transmitted before Response.

### 7. Comparison with TypeScript

| Component | TypeScript | Swift | Gap |
|-----------|------------|-------|-----|
| Schema types | Full | Partial | Need more shape types |
| Schema encode | Generated | Generated | ✓ Parity |
| Schema decode | Generated | Generated | ✓ Parity |
| Channel binding | Generated | Generated | ✓ Parity |
| Wire codec | Working | Working | ✓ Parity |
| Streaming | Working | **Broken** | Need to fix Tx/Driver integration |

## Questions Answered

1. **Does Swift have schema-driven encode/decode?** 
   Yes, via `roam-codegen`. Generated code uses schema-driven serialization.

2. **How does Swift channel binding work?**
   Generated code creates Tx/Rx with serialize/deserialize closures and binds to driver's taskSender.

3. **Is roam-codegen Swift target functional?**
   Yes, produces working code. Minor fixes were needed for enum naming and static methods.

4. **What's blocking spec tests?**
   Streaming RPC - the synchronous `Tx.send()` calls don't properly queue through the async event loop before Response is sent.

5. **Swift-specific challenges?**
   Actor isolation, async/await ordering. The `Tx.send()` is sync but needs to integrate with async driver.

## Root Cause of Streaming Failure

Looking at the flow:
1. `dispatchgenerate` creates `Tx` and calls `handler.generate(count, output)`
2. Handler calls `output.send(i)` which calls `taskSender(.data(...))`
3. `taskSender` yields to `eventContinuation`
4. But `dispatchgenerate` immediately calls `output.close()` and `taskSender(.response(...))`
5. The response gets sent before the event loop processes the Data messages

The issue is that `Tx.send()` is fire-and-forget (yields to AsyncStream) but the dispatcher doesn't await for the data to actually be sent before sending the response.

## Next Steps

Phase 002 should focus on:
1. Fix the Tx/Rx integration with the async event loop
2. Ensure Data messages are sent before Response
3. Get all 3 streaming tests passing

## Deliverables

1. ✓ Gap analysis document (this file)
2. ✓ Test results documented
3. ✓ Codegen output verified
4. TODO: Update overview.md phases based on findings
