# Phase 012: Spec Compliance Tests

**Status**: TODO

## Objective

Make the Swift implementation pass the spec compliance test suite, achieving
full protocol compatibility with Rust and TypeScript implementations.

## Background

The spec test suite lives in `spec/` and is run with:
```bash
SUBJECT_CMD='./path/to/subject' cargo nextest run -p spec-tests
```

The test harness:
1. Spawns a "subject" (implementation under test)
2. Connects to it via the roam protocol
3. Exercises various protocol scenarios
4. Verifies correct behavior

## Swift Subject

The Swift subject lives in `swift/subject/`:
- `Subject.swift` — Main entry point
- `Server.swift` — Accepts connections and dispatches
- `subject-swift.sh` — Shell wrapper for running

## Running Tests

```bash
# Build Swift subject
cd swift/subject && swift build -c release

# Run spec tests against Swift
SUBJECT_CMD='swift/subject/subject-swift.sh' cargo nextest run -p spec-tests

# Run specific test
SUBJECT_CMD='swift/subject/subject-swift.sh' cargo nextest run -p spec-tests test_name
```

## Expected Test Categories

Based on the Rust spec tests, expect tests for:

### Hello Exchange
- Both peers send Hello
- Negotiation of max_payload_size
- Negotiation of initial_channel_credit
- Unknown Hello version handling

### Unary RPC
- Simple echo method
- Unknown method → UnknownMethod error
- Invalid payload → InvalidPayload error
- Request ID uniqueness
- Multiple concurrent requests (pipelining)

### Channeling
- Caller-to-callee streaming (Tx)
- Callee-to-caller streaming (Rx)
- Bidirectional streaming
- Channel ID allocation (parity)
- Close handling
- Data-after-close error
- Unknown channel ID error

### Flow Control
- Initial credit
- Credit grants
- Credit overrun error
- Zero credit blocking

### Error Handling
- Goodbye on protocol violation
- Connection closure

## Debugging Test Failures

### Enable Logging

```swift
// In Subject.swift
import os

let logger = Logger(subsystem: "roam", category: "subject")

// Log incoming messages
logger.debug("Received: \(message)")
```

### Capture Wire Traffic

```bash
# Run with verbose output
RUST_LOG=debug SUBJECT_CMD='swift/subject/subject-swift.sh' cargo nextest run -p spec-tests
```

### Compare with Rust Subject

```bash
# Run same test against Rust
SUBJECT_CMD='./target/release/subject-rust' cargo nextest run -p spec-tests test_name

# Compare behavior
```

## Common Failure Modes

### 1. Encoding Mismatch

**Symptom**: Tests fail with "invalid payload" or "decode error"

**Cause**: Swift encoding differs from Rust postcard format

**Fix**: Compare encoded bytes with golden vectors, fix encoder

### 2. Wrong Discriminant

**Symptom**: Wrong enum variant decoded

**Cause**: Discriminant values don't match Rust `#[repr(u8)]`

**Fix**: Verify generated discriminants match Rust

### 3. Channel ID Parity

**Symptom**: Tests fail with channel errors when both peers make calls

**Cause**: Wrong parity (initiator should use odd, acceptor even)

**Fix**: Check `ChannelIdAllocator` logic

### 4. Message Ordering

**Symptom**: Data arrives after Response, or Response before all Data

**Cause**: Task messages not sent in correct order

**Fix**: Ensure Data/Close sent before Response via single task channel

### 5. Hello Timing

**Symptom**: Connection fails immediately

**Cause**: Hello not sent/received before other messages

**Fix**: Check connection establishment sequence

### 6. Credit Tracking

**Symptom**: Credit overrun errors

**Cause**: Not tracking credit correctly

**Fix**: Verify credit accounting in ChannelRegistry

## Test-Driven Fixes

For each failing test:

1. **Identify the failure** — What error? What rule violated?
2. **Reproduce minimally** — Can you write a unit test?
3. **Compare with Rust** — What does Rust do differently?
4. **Fix the code** — Make the minimal change
5. **Add regression test** — Ensure it stays fixed
6. **Add spec annotation** — `// [verify r[rule.id]]`

## Verification Annotations

Once a test passes, add verify annotations:

```swift
// In test file or near implementation
// [verify r[channeling.id.parity]]
func testChannelIdParity() {
    // Test that initiator uses odd IDs
}
```

## Implementation Steps

1. Build Swift subject and verify it starts
2. Run full test suite, capture failures
3. Categorize failures by type
4. Fix encoding issues first (foundation)
5. Fix channel issues second
6. Fix flow control issues
7. Fix edge cases
8. Iterate until all tests pass
9. Add verify annotations

## Success Criteria

1. `cargo nextest run -p spec-tests` passes with Swift subject
2. All test categories pass:
   - Hello exchange
   - Unary RPC
   - Channeling
   - Flow control
   - Error handling
3. Tracey shows non-zero verify coverage
4. Swift subject behaves identically to Rust subject

## Tracking Progress

Update this file as tests pass:

| Category | Tests | Passing | Notes |
|----------|-------|---------|-------|
| Hello | ? | ? | |
| Unary | ? | ? | |
| Channeling | ? | ? | |
| Flow Control | ? | ? | |
| Errors | ? | ? | |
| **Total** | ? | ? | |

## Notes

- Start with basic tests (hello, echo) before complex ones
- Each fix may break other tests — run full suite after changes
- The Rust subject is the reference — when in doubt, match its behavior
- Some tests may require specific timing — use async properly
- Log liberally during debugging, remove logs for final version

## Dependencies

- All previous phases (complete implementation)

## Blocked By

- Need working Swift implementation to test
- Need spec test harness to work with Swift
