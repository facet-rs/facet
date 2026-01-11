# Phase 011: Spec Annotations

**Status**: TODO

## Objective

Add tracey spec annotations (`// [impl ...]`) throughout the Swift codebase to
document which spec rules are implemented where.

## Background

Tracey tracks spec coverage by scanning for comments like:
```swift
// [impl channeling.id.parity]
```

Currently, `roam/swift` shows 0% coverage. This phase adds annotations to
all relevant code to properly track implementation status.

## Spec Rules

The roam spec (`docs/content/spec/_index.md`) defines rules with IDs like:
- `core.call`
- `core.channel`
- `channeling.id.parity`
- `channeling.allocation.caller`
- `flow.channel.credit-based`
- `message.hello.timing`
- etc.

Each rule that has a Swift implementation should be annotated.

## Annotation Format

```swift
// [impl r[rule.id]]
// Single rule

// [impl r[rule.id.one]]
// [impl r[rule.id.two]]
// Multiple rules on consecutive lines

/// Documentation comment
/// 
/// [impl r[rule.id]]
// In doc comments
```

## Files to Annotate

### Channel.swift

```swift
/// Stream ID type.
public typealias ChannelId = UInt64

/// Connection role - determines stream ID parity.
///
/// [impl channeling.id.parity]
public enum Role {
    /// Initiator uses odd stream IDs (1, 3, 5, ...).
    case initiator
    /// Acceptor uses even stream IDs (2, 4, 6, ...).
    case acceptor
}

/// Allocates unique stream IDs with correct parity.
///
/// [impl channeling.id.uniqueness]
/// [impl channeling.id.parity]
public class ChannelIdAllocator {
    // ...
}

/// Tx stream handle - caller sends data to callee.
///
/// [impl channeling.caller-pov]
/// [impl channeling.type]
/// [impl channeling.holder-semantics]
public struct Tx<T> {
    // ...
}
```

### Wire.swift

```swift
/// Wire protocol messages.
///
/// [impl message.unknown-variant]
public enum Message {
    case hello(Hello)
    case goodbye(reason: String)
    case request(...)
    // ...
}

/// Hello message for connection establishment.
///
/// [impl message.hello.timing]
/// [impl message.hello.structure]
public enum Hello {
    case v1(maxPayloadSize: UInt32, initialChannelCredit: UInt32)
}
```

### Driver.swift

```swift
/// Connection driver.
///
/// [impl message.hello.ordering]
public actor Driver {
    // ...
    
    /// Handle incoming Request message.
    ///
    /// [impl unary.request-id.duplicate-detection]
    func handleRequest(...) {
        // Check for duplicate request ID
        if inFlightRequests.contains(requestId) {
            // [impl unary.request-id.duplicate-detection]
            sendGoodbye(reason: "unary.request-id.duplicate-detection")
            return
        }
    }
}
```

### Postcard.swift

```swift
/// Encode a varint.
///
/// [impl flow.channel.byte-accounting]
public func encodeVarint(_ value: UInt64) -> [UInt8] {
    // ...
}
```

### Transport.swift

```swift
/// COBS-framed transport.
///
/// [impl transport.bytestream.cobs]
public struct NIOTransport: MessageTransport {
    // ...
}
```

## Rule Categories to Cover

### Core Semantics
- `core.call` — Request/Response lifecycle
- `core.call.request-id` — Request ID uniqueness
- `core.call.cancel` — Cancel semantics
- `core.channel` — Tx/Rx direction semantics
- `core.channel.return-forbidden` — No Tx/Rx in return types
- `core.error.roam-error` — Error wrapping
- `core.error.connection` — Connection errors
- `core.error.goodbye-reason` — Goodbye message format
- `core.metadata` — Metadata handling

### Unary RPC
- `unary.request-id.uniqueness`
- `unary.request-id.duplicate-detection`
- `unary.request-id.in-flight`
- `unary.initiate`
- `unary.complete`
- `unary.request.payload-encoding`
- `unary.response.encoding`
- `unary.metadata.*`
- `unary.error.*`
- `unary.lifecycle.*`
- `unary.cancel.*`

### Channeling
- `channeling.type`
- `channeling.caller-pov`
- `channeling.holder-semantics`
- `channeling.allocation.caller`
- `channeling.id.uniqueness`
- `channeling.id.zero-reserved`
- `channeling.id.parity`
- `channeling.lifecycle.*`
- `channeling.data`
- `channeling.data.size-limit`
- `channeling.data.invalid`
- `channeling.close`
- `channeling.data-after-close`
- `channeling.reset`
- `channeling.reset.effect`
- `channeling.reset.credit`
- `channeling.unknown`
- `channeling.call-complete`
- `channeling.channels-outlive-response`

### Flow Control
- `flow.channel.credit-based`
- `flow.channel.all-transports`
- `flow.channel.byte-accounting`
- `flow.channel.initial-credit`
- `flow.channel.credit-grant`
- `flow.channel.credit-additive`
- `flow.channel.credit-prompt`
- `flow.channel.credit-consume`
- `flow.channel.credit-overrun`
- `flow.channel.zero-credit`
- `flow.channel.close-exempt`
- `flow.channel.infinite-credit`
- `flow.unary.payload-limit`

### Messages
- `message.unknown-variant`
- `message.decode-error`
- `message.hello.timing`
- `message.hello.structure`
- `message.hello.unknown-version`
- `message.hello.ordering`
- `message.hello.negotiation`
- `message.hello.enforcement`
- `message.goodbye.send`
- `message.goodbye.receive`

### Transport
- `transport.message.one-to-one`
- `transport.message.binary`
- `transport.message.multiplexing`
- `transport.bytestream.cobs`

## Implementation Steps

1. Read the spec and make a checklist of all rules
2. For each Swift file, identify which rules it implements
3. Add `// [impl r[...]]` annotations
4. Run tracey to verify annotations are recognized
5. Iterate until coverage is reasonable

## Verification

```bash
# Check tracey status
# (via MCP or CLI)

# Expected output after this phase:
# roam/swift: impl XX%, verify 0% (N/87 rules)
```

## Success Criteria

1. All implemented rules have annotations
2. Tracey shows non-zero impl coverage for `roam/swift`
3. Annotations are in the correct locations (near the implementing code)
4. No false annotations (don't claim to implement something that isn't)

## Notes

- Focus on impl annotations first, verify annotations come with tests
- Some rules may not be implemented yet — that's fine, don't annotate them
- Be conservative — only annotate if the code actually implements the rule
- Use consistent formatting: `// [impl r[rule.id]]`
- Group related annotations together where they implement the same code

## Dependencies

- All previous phases (need code to annotate)

## Blocked By

- Should have working implementation before annotating
