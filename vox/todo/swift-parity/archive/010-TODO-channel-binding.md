# Phase 010: Channel Binding

**Status**: TODO

## Objective

Implement schema-driven channel binding in Swift, allowing `Tx` and `Rx` types
in method arguments to be bound with channel IDs and connected to the channel
registry.

## Background

Channel binding is the process of:
1. Finding `Tx<T>` and `Rx<T>` values in method arguments
2. Allocating unique channel IDs for each
3. Connecting them to the channel infrastructure (senders/receivers)

Rust does this with `Poke` reflection. TypeScript does it with schema-driven
walking. Swift needs a similar approach, but cannot use Mirror for mutation.

## Design

### The Problem

Swift's `Mirror` is read-only. We cannot mutate fields via reflection.
Therefore, we need a different approach:

**Option A: Protocol-based binding** — Types implement `ChannelBindable`

**Option B: Generated binding code** — Codegen emits binding logic per type

**Option C: Mutable wrapper** — Use reference types or inout parameters

### Recommended: Protocol + Generated Code (Option A + B)

Define a protocol that types implement, and generate the implementation:

```swift
/// Protocol for types that contain channels needing binding.
public protocol ChannelBindable {
    /// Bind all channels in this value.
    mutating func bindChannels(binder: inout ChannelBinder)
}
```

### ChannelBinder

```swift
/// Binder for allocating and connecting channels.
public struct ChannelBinder {
    private let allocator: ChannelIdAllocator
    private let registry: ChannelRegistry
    private let taskSender: TaskSender?
    private let mode: BindingMode
    
    public enum BindingMode {
        case client  // Caller side
        case server  // Callee side
    }
    
    public init(
        allocator: ChannelIdAllocator,
        registry: ChannelRegistry,
        taskSender: TaskSender? = nil,
        mode: BindingMode
    ) {
        self.allocator = allocator
        self.registry = registry
        self.taskSender = taskSender
        self.mode = mode
    }
    
    /// Allocate a channel ID.
    public func allocateId() -> UInt64 {
        allocator.next()
    }
    
    /// Register an incoming channel (we receive data on it).
    public func registerIncoming(channelId: UInt64, sender: ChannelSender<[UInt8]>) {
        registry.registerIncoming(channelId: channelId, sender: sender)
    }
    
    /// Register outgoing credit tracking.
    public func registerOutgoing(channelId: UInt64) {
        registry.registerOutgoingCredit(channelId: channelId)
    }
    
    /// Get task sender for server-side Tx binding.
    public func getTaskSender() -> TaskSender? {
        taskSender
    }
}
```

### Tx Binding

```swift
extension Tx: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        // Allocate ID if not already set
        if channelId == 0 {
            channelId = binder.allocateId()
        }
        
        switch binder.mode {
        case .client:
            // Client side: Tx in args means we're passing a receiver to callee
            // The Tx's internal sender will be drained by a task
            // (Already set up when channel() was called)
            binder.registerOutgoing(channelId: channelId)
            
        case .server:
            // Server side: Tx means we send to caller
            // Use taskSender to send Data messages directly
            if let taskSender = binder.getTaskSender() {
                self.taskSender = taskSender
            }
        }
    }
}
```

### Rx Binding

```swift
extension Rx: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        // Allocate ID if not already set
        if channelId == 0 {
            channelId = binder.allocateId()
        }
        
        switch binder.mode {
        case .client:
            // Client side: Rx in args means caller receives from callee
            // Create channel and register for incoming data
            let (sender, receiver) = makeChannel([UInt8].self)
            binder.registerIncoming(channelId: channelId, sender: sender)
            self.receiver = receiver
            
        case .server:
            // Server side: handled in dispatch (type is flipped to Tx)
            break
        }
    }
}
```

### Generated ChannelBindable for Structs

For structs containing Tx/Rx, generate the conformance:

```swift
// Generated
extension SumArgs: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        numbers.bindChannels(binder: &binder)
    }
}

extension TransformArgs: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        input.bindChannels(binder: &binder)
        output.bindChannels(binder: &binder)
    }
}
```

### Container Binding

```swift
extension Array: ChannelBindable where Element: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        for i in indices {
            self[i].bindChannels(binder: &binder)
        }
    }
}

extension Optional: ChannelBindable where Wrapped: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        if case .some(var value) = self {
            value.bindChannels(binder: &binder)
            self = .some(value)
        }
    }
}
```

### Types Without Channels

Types that don't contain channels can have a no-op implementation:

```swift
extension String: ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        // No channels in String
    }
}

// Or use a default implementation for primitives
extension ChannelBindable {
    public mutating func bindChannels(binder: inout ChannelBinder) {
        // Default: no channels
    }
}
```

### Client-Side Usage

```swift
// In ConnectionHandle.call()
public func call<Args: ChannelBindable & PostcardEncodable>(
    methodId: UInt64,
    args: inout Args,
    argsSchemas: [Schema]
) async throws -> [UInt8] {
    // Bind channels in args
    var binder = ChannelBinder(
        allocator: channelAllocator,
        registry: channelRegistry,
        mode: .client
    )
    args.bindChannels(binder: &binder)
    
    // Encode args (Tx/Rx now have channel IDs)
    let payload = args.encoded()
    
    // Make the call
    return try await callRaw(methodId: methodId, payload: payload)
}
```

### Server-Side Usage

```swift
// In generated dispatcher
private func dispatchSum(
    payload: [UInt8],
    requestId: UInt64,
    registry: ChannelRegistry,
    taskSender: TaskSender
) async {
    // Decode channel ID from payload
    var decoder = PostcardDecoder(data: payload)
    let channelId = try decoder.decodeVarint()
    
    // Create Rx (server receives what client sends via their Tx)
    let (sender, receiver) = makeChannel([UInt8].self)
    registry.registerIncoming(channelId: channelId, sender: sender)
    let rx = Rx<Int32>(channelId: channelId, receiver: receiver)
    
    // Call handler
    let result = try await handler.sum(numbers: rx)
    
    // Send response
    // ...
}
```

## Alternative: Schema-Driven Binding

If generating `ChannelBindable` for every type is too heavy, use schema-driven
binding with type erasure:

```swift
/// Bind channels in a value using its schema.
public func bindChannelsWithSchema(
    _ value: inout Any,
    schema: Schema,
    binder: inout ChannelBinder,
    registry: SchemaRegistry
) throws {
    switch schema {
    case .tx(let element):
        guard var tx = value as? AnyTx else { throw BindingError.typeMismatch }
        tx.bindChannel(binder: &binder, mode: .client)
        value = tx
        
    case .rx(let element):
        guard var rx = value as? AnyRx else { throw BindingError.typeMismatch }
        rx.bindChannel(binder: &binder, mode: .client)
        value = rx
        
    case .struct(let structSchema):
        // Walk fields using Mirror + rebuild
        // This is complex due to Swift's value semantics
        
    case .vec(let element):
        guard var arr = value as? [Any] else { throw BindingError.typeMismatch }
        for i in arr.indices {
            try bindChannelsWithSchema(&arr[i], schema: element, binder: &binder, registry: registry)
        }
        value = arr
        
    // ... other cases
    }
}
```

This is more complex and error-prone than the protocol approach.

## Files to Create/Modify

| File | Action |
|------|--------|
| `swift/roam-runtime/Sources/RoamRuntime/ChannelBinder.swift` | CREATE |
| `swift/roam-runtime/Sources/RoamRuntime/Channel.swift` | MODIFY (add ChannelBindable) |
| `rust/roam-codegen/src/targets/swift/binding.rs` | CREATE |
| `rust/roam-codegen/src/targets/swift/mod.rs` | Integrate binding generation |

## Implementation Steps

1. Define `ChannelBindable` protocol
2. Create `ChannelBinder` struct
3. Implement `ChannelBindable` for `Tx<T>`
4. Implement `ChannelBindable` for `Rx<T>`
5. Implement container conformances (Array, Optional)
6. Add binding generation to roam-codegen
7. Generate conformances for types containing channels
8. Integrate with client generation (phase 008)
9. Integrate with server generation (phase 009)
10. Test channel binding works correctly

## Success Criteria

1. `Tx` and `Rx` can be bound with channel IDs
2. Binding connects channels to registry
3. Client-side binding sets up drain tasks for outgoing Tx
4. Server-side binding connects to TaskSender
5. Generated types with channels implement `ChannelBindable`
6. Nested channels in containers are bound correctly

## Test Cases

```swift
func testTxBinding() {
    let (tx, rx) = channel(Int32.self)
    var args = tx
    
    let allocator = ChannelIdAllocator(role: .initiator)
    let registry = ChannelRegistry()
    var binder = ChannelBinder(allocator: allocator, registry: registry, mode: .client)
    
    args.bindChannels(binder: &binder)
    
    XCTAssertEqual(args.channelId, 1) // First odd ID
    XCTAssertTrue(registry.containsOutgoing(channelId: 1))
}

func testRxBinding() {
    let (tx, rx) = channel(Int32.self)
    var args = rx
    
    let allocator = ChannelIdAllocator(role: .initiator)
    let registry = ChannelRegistry()
    var binder = ChannelBinder(allocator: allocator, registry: registry, mode: .client)
    
    args.bindChannels(binder: &binder)
    
    XCTAssertEqual(args.channelId, 1)
    XCTAssertTrue(registry.containsIncoming(channelId: 1))
}

func testNestedBinding() {
    var args: [Tx<Int32>] = [
        channel(Int32.self).0,
        channel(Int32.self).0,
    ]
    
    let allocator = ChannelIdAllocator(role: .initiator)
    let registry = ChannelRegistry()
    var binder = ChannelBinder(allocator: allocator, registry: registry, mode: .client)
    
    args.bindChannels(binder: &binder)
    
    XCTAssertEqual(args[0].channelId, 1)
    XCTAssertEqual(args[1].channelId, 3) // Next odd ID
}
```

## Notes

- Swift's value semantics require `inout` or `mutating` for binding
- The `ChannelBindable` protocol approach is cleaner than schema-driven
- Generated code provides type safety and performance
- Container conformances handle nested channels automatically
- Server-side binding is different (type flipping, TaskSender)
- Consider using `@unchecked Sendable` for channel types with internal mutation

## Dependencies

- Phase 002 (Schema types)
- Phase 005-007 (Type and codec generation)

## Blocked By

- Core Tx/Rx types must exist and be modifiable
