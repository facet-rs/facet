# Swift Implementation Parity

## Goal

Bring the Swift implementation up to parity with Rust and TypeScript in terms of:
- Schema-driven serialization/deserialization
- Channel binding
- Connection logic
- Code generation from `roam-codegen`
- Spec compliance (tracey rule coverage)

## Current State

The Swift implementation has solid foundations but is missing key pieces:

| Component | Rust | TypeScript | Swift |
|-----------|------|------------|-------|
| Postcard primitives | ✅ `facet_postcard` | ✅ `roam-postcard` | ✅ `Postcard.swift` |
| Wire protocol | ✅ `roam-wire` | ✅ `roam-wire` | ✅ `Wire.swift` |
| COBS framing | ✅ `roam-stream` | ✅ `roam-stream` | ✅ `COBS.swift` |
| SwiftNIO transport | N/A | N/A | ✅ `Transport.swift` |
| Channel types (Tx/Rx) | ✅ | ✅ | ✅ `Channel.swift` |
| Channel registry | ✅ | ✅ | ✅ `Channel.swift` |
| Driver/Connection | ✅ | ✅ | ✅ `Driver.swift` |
| **Schema types** | ✅ `facet::Shape` | ✅ `Schema` | ⚠️ Partial in `Schema.swift` |
| **Schema-driven encode** | ✅ reflection | ✅ `encodeWithSchema` | ❌ Manual only |
| **Schema-driven decode** | ✅ reflection | ✅ `decodeWithSchema` | ❌ Manual only |
| **Channel binding** | ✅ `Poke` reflection | ✅ `bindChannels()` | ⚠️ Incomplete |
| **Code generation** | N/A (uses reflection) | ✅ `roam-codegen` | ❌ Not implemented |
| **Spec rule coverage** | 80% impl | 47% impl | 0% impl |
| **Golden vector tests** | ✅ Generator | ✅ 25 tests | ✅ Wire encoding tests |

## Architecture

### How Rust Works (reflection-based)

```
#[derive(Facet)] on types
        │
        ▼
facet::Shape (compile-time metadata)
        │
        ├─► facet_postcard::to_vec() — reflection-based serialization
        │
        └─► Poke::new(&mut value) — reflection-based mutation
                    │
                    └─► bind_streams_recursive() walks and mutates Tx/Rx
```

### How TypeScript Works (schema + codegen)

```
roam-codegen (Rust)
        │
        ▼
Generated TypeScript:
  - Type definitions
  - Schema constants
  - Client stubs
  - Server dispatchers
        │
        ▼
Runtime:
  - encodeWithSchema(schema, value)
  - decodeWithSchema(schema, buf)
  - bindChannels(schemas, args, ...)
```

### How Swift Should Work

Swift cannot do Rust-style reflection (Mirror is read-only and type-erased).
Options:

1. **Code generation** (like TypeScript) — generate Swift from `roam-codegen`
2. **Swift macros** (Swift 5.9+) — generate conformances at compile time
3. **Protocol-based** — manual conformances for `PostcardCodable`, `ChannelBindable`

**Recommended**: Code generation from `roam-codegen`, same as TypeScript.

## Phases

| Phase | File | Status | Description |
|-------|------|--------|-------------|
| 001 | [001-TODO-assess-swift-baseline.md](./001-TODO-assess-swift-baseline.md) | TODO | Audit current Swift code, identify gaps |
| 002 | [002-TODO-schema-types.md](./002-TODO-schema-types.md) | TODO | Port Schema type hierarchy to Swift |
| 003 | [003-TODO-schema-encode.md](./003-TODO-schema-encode.md) | TODO | Implement `encodeWithSchema()` in Swift |
| 004 | [004-TODO-schema-decode.md](./004-TODO-schema-decode.md) | TODO | Implement `decodeWithSchema()` in Swift |
| 005 | [005-TODO-codegen-swift-types.md](./005-TODO-codegen-swift-types.md) | TODO | Extend `roam-codegen` to emit Swift types |
| 006 | [006-TODO-codegen-swift-schemas.md](./006-TODO-codegen-swift-schemas.md) | TODO | Generate Swift schema constants |
| 007 | [007-TODO-codegen-swift-encode-decode.md](./007-TODO-codegen-swift-encode-decode.md) | TODO | Generate Swift encode/decode conformances |
| 008 | [008-TODO-codegen-swift-client.md](./008-TODO-codegen-swift-client.md) | TODO | Generate Swift client stubs |
| 009 | [009-TODO-codegen-swift-server.md](./009-TODO-codegen-swift-server.md) | TODO | Generate Swift server dispatchers |
| 010 | [010-TODO-channel-binding.md](./010-TODO-channel-binding.md) | TODO | Schema-driven channel binding |
| 011 | [011-TODO-spec-annotations.md](./011-TODO-spec-annotations.md) | TODO | Add tracey rule annotations |
| 012 | [012-TODO-spec-tests.md](./012-TODO-spec-tests.md) | TODO | Pass spec compliance tests |

## Key Design Decisions

### 1. Swift Cannot Do Rust-Style Reflection

Swift's `Mirror` is:
- **Read-only** — cannot mutate fields
- **Type-erased** — values are `Any`, no typed access
- **No type identity** — can't check "is this `roam.Tx`?"

Therefore, we cannot replicate Rust's `Poke`-based approach in Swift.

### 2. Code Generation is the Path Forward

`roam-codegen` already has a `targets/swift/` directory with:
- `types.rs` — generates Swift types from `Shape`
- `schema.rs` — generates Swift schema constants
- `encode.rs` — generates encode expressions
- `decode.rs` — generates decode expressions
- `client.rs` — generates client stubs
- `server.rs` — generates server dispatchers

This infrastructure exists but may be incomplete or out of date.

### 3. Protocol-Based Serialization

Swift should use protocols for encode/decode:

```swift
protocol PostcardEncodable {
    func encode(to encoder: inout PostcardEncoder)
}

protocol PostcardDecodable {
    init(from decoder: inout PostcardDecoder) throws
}

typealias PostcardCodable = PostcardEncodable & PostcardDecodable
```

Generated types conform to these protocols.

### 4. Schema-Driven Channel Binding

For types containing `Tx`/`Rx`, generate a `ChannelBindable` conformance:

```swift
protocol ChannelBindable {
    mutating func bindChannels(binder: inout ChannelBinder)
}

// Generated for types with channels
extension SumArgs: ChannelBindable {
    mutating func bindChannels(binder: inout ChannelBinder) {
        numbers.bindChannels(binder: &binder)
    }
}
```

## Phase Dependencies

```
001 Assess Baseline
        │
        ▼
002 Schema Types ─────────────────────┐
        │                             │
        ├───────────┐                 │
        ▼           ▼                 │
003 Encode    004 Decode              │
        │           │                 │
        └─────┬─────┘                 │
              │                       │
              ▼                       │
005 Codegen Types ◄───────────────────┘
              │
              ▼
006 Codegen Schemas
              │
              ▼
007 Codegen Encode/Decode
              │
              ├───────────┐
              ▼           ▼
008 Client      009 Server
              │           │
              └─────┬─────┘
                    │
                    ▼
010 Channel Binding
                    │
                    ▼
011 Spec Annotations
                    │
                    ▼
012 Spec Tests
```

## Success Criteria

1. Swift can encode/decode all types that Rust and TypeScript can
2. Swift golden vector tests pass (wire format compatibility)
3. Swift spec tests pass: `SUBJECT_CMD='./subject-swift' cargo nextest run -p spec-tests`
4. Tracey shows non-zero impl coverage for `roam/swift`
5. Generated Swift code compiles and works with SwiftNIO transport

## Related Files

### Rust (codegen source)
- `rust/roam-codegen/src/targets/swift/` — Swift code generation
- `rust/roam-schema/src/lib.rs` — `ServiceDetail`, `MethodDetail`, `Shape` helpers

### Swift (implementation)
- `swift/roam-runtime/Sources/RoamRuntime/` — Core runtime
- `swift/roam-runtime/Tests/RoamRuntimeTests/` — Tests
- `swift/subject/` — Spec test subject

### TypeScript (reference)
- `typescript/packages/roam-postcard/src/schema.ts` — Schema types
- `typescript/packages/roam-postcard/src/schema_codec.ts` — Schema encode/decode
- `typescript/packages/roam-core/src/channeling/` — Channel binding

### Spec
- `docs/content/spec/_index.md` — roam specification
- `.config/tracey/config.kdl` — Tracey configuration

## Commands

```bash
# Run Swift tests
cd swift/roam-runtime && swift test

# Run Swift subject against spec tests
SUBJECT_CMD='swift/subject/subject-swift.sh' cargo nextest run -p spec-tests

# Check tracey coverage
# (via MCP tools or CLI)

# Generate Swift code (once codegen is ready)
cargo xtask codegen --swift

# Type check TypeScript (for reference)
cd typescript && pnpm check
```

## Estimated Effort

| Phase | Complexity | Est. Time |
|-------|------------|-----------|
| 001 | Low | 2-3 hours |
| 002 | Medium | 3-4 hours |
| 003 | Medium | 4-6 hours |
| 004 | Medium | 4-6 hours |
| 005 | Medium | 4-6 hours |
| 006 | Medium | 3-4 hours |
| 007 | High | 6-8 hours |
| 008 | Medium | 4-6 hours |
| 009 | Medium | 4-6 hours |
| 010 | Medium | 4-6 hours |
| 011 | Low | 2-3 hours |
| 012 | Medium | 4-6 hours |
| **Total** | | **44-64 hours** |

This is a multi-session effort. Each phase is designed to be completable in one session.
