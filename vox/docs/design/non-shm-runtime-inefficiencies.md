# Non-SHM Runtime Inefficiencies

Design notes on waste in the current Swift and Rust implementations that is
unrelated to the shared-memory transport.

This is a report, not a spec change.

## Scope

- Shared-memory-specific issues are out of scope.
- The goal here is to identify broad architectural waste, not micro-optimizations.
- The focus is on steady-state request/response paths, schema handling, and byte ownership.

## Executive Summary

The biggest waste is on the Swift side, not the Rust side.

Swift currently pays for the same data multiple times:

- schema payloads are rebuilt and re-encoded on live request/response paths
- payloads bounce repeatedly between `[UInt8]` and `Data`
- postcard and wire encoding are built around many small temporary arrays
- decode paths also create avoidable copies

Rust has some real costs too, but they are narrower and more intentional:

- stable conduit duplicates frames for replay
- stable receive duplicates payload bytes again before deserializing
- persistent operation storage reserializes responses for the store

The architectural mismatch in Swift is more serious than any one isolated copy.
The system does not have a stable answer to "what owns bytes here?"

## Findings

### 1. Swift schema attachment is still per-message work

This is the highest-value issue outside SHM.

Generated server dispatchers build a fresh response schema payload on each
request:

- [swift/subject/Sources/subject-swift/TestbedServer.swift](/Users/amos/bearcove/vox/swift/subject/Sources/subject-swift/TestbedServer.swift#L537)

The client send path also rebuilds request schema payloads on demand:

- [swift/vox-runtime/Sources/VoxRuntime/Driver+Outgoing.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Driver+Outgoing.swift#L136)
- [swift/vox-runtime/Sources/VoxRuntime/Driver+Outgoing.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Driver+Outgoing.swift#L243)

The actual work is not cheap. `MethodSchemaInfo.buildPayload(...)` rebuilds a
`[Schema]` array by walking IDs through the registry:

- [swift/vox-runtime/Sources/VoxRuntime/Schema.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Schema.swift#L1171)

Then `SchemaSendTracker.filterForSending(...)` filters under a lock and returns
a new `SchemaPayload`:

- [swift/vox-runtime/Sources/VoxRuntime/Schema.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Schema.swift#L1213)

Then the payload is CBOR-encoded again.

This is exactly the sort of work that should be mostly connection-scoped and
method-scoped, not live on every request path.

By contrast, Rust already has a real per-method fast path:

- [rust/vox-types/src/schema.rs](/Users/amos/bearcove/vox/rust/vox-types/src/schema.rs#L102)
- [rust/vox-types/src/schema.rs](/Users/amos/bearcove/vox/rust/vox-types/src/schema.rs#L145)
- [rust/vox-core/src/session/mod.rs](/Users/amos/bearcove/vox/rust/vox-core/src/session/mod.rs#L1857)
- [rust/vox-core/src/session/mod.rs](/Users/amos/bearcove/vox/rust/vox-core/src/session/mod.rs#L2044)

Architectural conclusion:

- Swift is still doing schema attachment as message-time work.
- Rust mostly does it as connection-state work.

### 2. Swift has a pervasive `Data` / `[UInt8]` ownership mismatch

The public client API uses `Data`:

- [swift/vox-runtime/Sources/VoxRuntime/VoxRuntime.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/VoxRuntime.swift#L27)

But the internal runtime largely uses `[UInt8]`.

So a normal call path does this:

- generated client builds `[UInt8]`
- wraps it into `Data`
- `Connection.call(...)` converts `Data` back into `[UInt8]`
- the response comes back as `[UInt8]`
- `Connection.call(...)` wraps it back into `Data`

References:

- [swift/subject/Sources/subject-swift/TestbedClient.swift](/Users/amos/bearcove/vox/swift/subject/Sources/subject-swift/TestbedClient.swift#L302)
- [swift/vox-runtime/Sources/VoxRuntime/Connection.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Connection.swift#L24)

The retry path is worse. It does `[UInt8] -> Data -> [UInt8]` inside the same
closure:

- [swift/subject/Sources/subject-swift/TestbedClient.swift](/Users/amos/bearcove/vox/swift/subject/Sources/subject-swift/TestbedClient.swift#L490)

On inbound handling, the same mismatch appears again:

- conduit receive converts `[UInt8]` to `Data` in [swift/vox-runtime/Sources/VoxRuntime/Conduit.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Conduit.swift#L21)
- wire decode copies payload bytes back into `[UInt8]` in [swift/vox-runtime/Sources/VoxRuntime/Wire.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Wire.swift#L31)
- generated server dispatch converts payloads to `Data` again in [swift/subject/Sources/subject-swift/Subject.swift](/Users/amos/bearcove/vox/swift/subject/Sources/subject-swift/Subject.swift#L267)

Architectural conclusion:

- Swift does not have a single canonical byte container.
- The boundaries between codegen, runtime, conduit, and wire are copy-heavy
  because ownership is not modeled consistently.

### 3. Swift postcard and wire encoding are allocation-heavy by design

The encoder API shape itself creates churn.

Primitive encoders return new arrays:

- [swift/vox-runtime/Sources/VoxRuntime/Postcard.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Postcard.swift#L7)

String and byte encoders create temporary arrays and concatenate:

- [swift/vox-runtime/Sources/VoxRuntime/Postcard.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Postcard.swift#L51)

Vector encoding repeatedly appends small temporary arrays:

- [swift/vox-runtime/Sources/VoxRuntime/Postcard.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Postcard.swift#L68)

Generated clients follow the same pattern:

- [swift/subject/Sources/subject-swift/TestbedClient.swift](/Users/amos/bearcove/vox/swift/subject/Sources/subject-swift/TestbedClient.swift#L302)

The wire layer does too:

- [swift/vox-runtime/Sources/VoxRuntime/Wire.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Wire.swift#L21)

This means Swift often pays for:

- a temporary array per field encoder
- repeated `+=` growth on the outer payload
- another container conversion at the runtime boundary

Architectural conclusion:

- Swift encode APIs are shaped around "return a new `[UInt8]`" instead of
  "append into an existing buffer".
- That is convenient for codegen but expensive on hot paths.

### 4. Swift decode paths also allocate more than necessary

The decode side has similar issues.

Several postcard decoders create `subdata` slices and then decode from those
copied fragments:

- [swift/vox-runtime/Sources/VoxRuntime/Postcard.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Postcard.swift#L137)
- [swift/vox-runtime/Sources/VoxRuntime/Postcard.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Postcard.swift#L145)
- [swift/vox-runtime/Sources/VoxRuntime/Postcard.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Postcard.swift#L153)

Wire decoding also copies payload bytes out of `Data` into `[UInt8]`:

- [swift/vox-runtime/Sources/VoxRuntime/Wire.swift](/Users/amos/bearcove/vox/swift/vox-runtime/Sources/VoxRuntime/Wire.swift#L31)

Architectural conclusion:

- Swift inbound decode is not just parsing; it is also doing container churn.
- The runtime currently treats copying as the normal way to cross decode layers.

### 5. Rust stable conduit pays an explicit replay tax

Rust's normal transport path is comparatively disciplined.

The main non-SHM cost is in stable conduit.

On send, the frame is written into the transport slot and then cloned into the
replay buffer:

- [rust/vox-core/src/stable_conduit/mod.rs](/Users/amos/bearcove/vox/rust/vox-core/src/stable_conduit/mod.rs#L747)

On receive, the inner message bytes are cloned again before postcard
deserialization:

- [rust/vox-core/src/stable_conduit/mod.rs](/Users/amos/bearcove/vox/rust/vox-core/src/stable_conduit/mod.rs#L847)

This is real overhead, but it is localized and tied to stable/replay semantics.

Architectural conclusion:

- Rust stable mode is paying for durability/replay behavior, not for a general
  byte-ownership mismatch.
- This is a targeted tax, not a systemic one.

### 6. Rust persistent operation storage adds another serialization pass

When a response is persisted for replay, the driver strips schemas and
reserializes the response payload for the operation store:

- [rust/vox-core/src/driver.rs](/Users/amos/bearcove/vox/rust/vox-core/src/driver.rs#L445)

This means a successful persistent response may be:

- prepared for wire send
- serialized again for storage
- later reconstructed for replay

This is not inherently wrong, but it is a meaningful steady-state cost for
persistent retry semantics.

Architectural conclusion:

- Rust persistence overhead exists, but it is feature-driven and explicit.
- It is not the same class of waste as Swift's everyday request path.

## Highest-ROI Direction

If the goal is to remove the most waste without discussing SHM, the ranking is:

1. Fix Swift schema attachment so it is mostly cached or precomputed
2. Pick one primary Swift byte container and stop bouncing between `Data` and `[UInt8]`
3. Replace Swift "return fresh `[UInt8]`" encoders with append-into-buffer APIs
4. Revisit Swift decode APIs so parsing does not imply copying
5. Only then worry about Rust stable/persistence copies

## Bottom Line

The main problem is not that Swift has a few slow spots. The main problem is
that Swift's runtime, codegen, and transport boundaries do not agree on byte
ownership or schema attachment strategy.

Rust's remaining waste is much narrower. Most of it comes from stable/replay
and persistence features, where the extra copying is at least attached to a
clear semantic reason.
