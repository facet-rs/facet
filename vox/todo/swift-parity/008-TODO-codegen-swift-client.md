# Phase 008: Code Generation - Swift Client

**Status**: TODO

## Objective

Extend `roam-codegen` to generate Swift client stubs that provide typed async
methods for calling remote services.

## Background

TypeScript generates client classes like:
```typescript
class TestbedClient {
    async echo(message: string): Promise<string> { ... }
    async sum(numbers: Rx<number>): Promise<bigint> { ... }
}
```

Swift needs equivalent async client protocols and implementations.

## Design

### Client Protocol

Generate a protocol defining the service interface:

```swift
/// Protocol for calling Testbed service methods.
public protocol TestbedCaller: Sendable {
    /// Echo back the message.
    func echo(message: String) async throws -> String
    
    /// Sum numbers from a stream.
    func sum(numbers: Rx<Int32>) async throws -> Int64
    
    /// Generate numbers to a stream.
    func generate(count: UInt32, output: Tx<Int32>) async throws
    
    /// Transform strings bidirectionally.
    func transform(input: Rx<String>, output: Tx<String>) async throws
}
```

### Client Implementation

```swift
/// Client for calling Testbed service over a connection.
public final class TestbedClient: TestbedCaller, Sendable {
    private let handle: ConnectionHandle
    
    public init(handle: ConnectionHandle) {
        self.handle = handle
    }
    
    public func echo(message: String) async throws -> String {
        // Encode arguments
        var encoder = PostcardEncoder()
        encoder.encodeString(message)
        let payload = encoder.bytes
        
        // Call remote method
        let response = try await handle.call(
            methodId: TestbedMethodId.echo,
            payload: payload
        )
        
        // Decode response
        var decoder = PostcardDecoder(data: response)
        let result: Result<String, RoamError<Never>> = try .decode(from: &decoder)
        
        switch result {
        case .ok(let value):
            return value
        case .err(let error):
            throw error
        }
    }
    
    public func sum(numbers: Rx<Int32>) async throws -> Int64 {
        // Bind channels in arguments
        var args = (numbers,)
        handle.bindChannels(&args, schemas: TestbedSchemas.sum.argsSchema)
        
        // Encode arguments (Rx serializes as channel ID)
        var encoder = PostcardEncoder()
        encoder.encodeVarint(args.0.channelId)
        let payload = encoder.bytes
        
        // Call remote method
        let response = try await handle.call(
            methodId: TestbedMethodId.sum,
            payload: payload
        )
        
        // Decode response
        var decoder = PostcardDecoder(data: response)
        let result: Result<Int64, RoamError<Never>> = try .decode(from: &decoder)
        
        switch result {
        case .ok(let value):
            return value
        case .err(let error):
            throw error
        }
    }
    
    public func generate(count: UInt32, output: Tx<Int32>) async throws {
        // For Tx in args, caller keeps the Rx to receive
        var args = (count, output)
        handle.bindChannels(&args, schemas: TestbedSchemas.generate.argsSchema)
        
        var encoder = PostcardEncoder()
        encoder.encodeVarint(UInt64(args.0))
        encoder.encodeVarint(args.1.channelId)
        let payload = encoder.bytes
        
        let response = try await handle.call(
            methodId: TestbedMethodId.generate,
            payload: payload
        )
        
        var decoder = PostcardDecoder(data: response)
        let result: Result<(), RoamError<Never>> = try .decode(from: &decoder)
        
        switch result {
        case .ok:
            return
        case .err(let error):
            throw error
        }
    }
}
```

### Method ID Constants

```swift
public enum TestbedMethodId {
    public static let echo: UInt64 = 0x1234567890abcdef
    public static let sum: UInt64 = 0xfedcba0987654321
    public static let generate: UInt64 = 0xabcdef1234567890
    // ... computed from service/method/signature hash
}
```

### Channel Usage Patterns

**Rx in args (caller sends to callee)**:
```swift
// Create channel pair
let (tx, rx) = channel(Int32.self)

// Pass Rx to method (it gets bound with channel ID)
let result = try await client.sum(numbers: rx)

// Use Tx to send values
try await tx.send(1)
try await tx.send(2)
try await tx.send(3)
tx.close()

// Result available after tx is closed
print(result) // 6
```

**Tx in args (caller receives from callee)**:
```swift
// Create channel pair
let (tx, rx) = channel(Int32.self)

// Pass Tx to method
try await client.generate(count: 5, output: tx)

// Use Rx to receive values
for try await value in rx {
    print(value) // 0, 1, 2, 3, 4
}
```

### Error Handling

```swift
public func echo(message: String) async throws -> String {
    // ...
    switch result {
    case .ok(let value):
        return value
    case .err(.user(let e)):
        throw e // Application error
    case .err(.unknownMethod):
        throw RoamClientError.unknownMethod(TestbedMethodId.echo)
    case .err(.invalidPayload):
        throw RoamClientError.invalidPayload
    case .err(.cancelled):
        throw RoamClientError.cancelled
    }
}

public enum RoamClientError: Error {
    case unknownMethod(UInt64)
    case invalidPayload
    case cancelled
    case connectionClosed
}
```

## Implementation in roam-codegen

### Client Generation

```rust
// rust/roam-codegen/src/targets/swift/client.rs

pub fn generate_client(service: &ServiceDetail) -> String {
    let mut code = String::new();
    
    // Generate protocol
    code.push_str(&generate_caller_protocol(service));
    
    // Generate method IDs
    code.push_str(&generate_method_ids(service));
    
    // Generate client class
    code.push_str(&generate_client_class(service));
    
    code
}

fn generate_caller_protocol(service: &ServiceDetail) -> String {
    // Generate TestbedCaller protocol with async method signatures
}

fn generate_client_class(service: &ServiceDetail) -> String {
    // Generate TestbedClient with implementation of each method
}

fn generate_method_impl(method: &MethodDetail) -> String {
    // Generate implementation for a single method
    // Handle: encode args, bind channels, call, decode response
}
```

### Channel Detection

```rust
fn method_has_channels(method: &MethodDetail) -> bool {
    method.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty))
}

fn generate_channel_binding(method: &MethodDetail) -> String {
    // Generate bindChannels call if method has Tx/Rx args
}
```

## Files to Modify

| File | Action |
|------|--------|
| `rust/roam-codegen/src/targets/swift/client.rs` | AUDIT + EXTEND |
| `rust/roam-codegen/src/targets/swift/mod.rs` | Integrate client generation |

## Implementation Steps

1. Audit existing `client.rs` — what does it generate?
2. Generate caller protocol with async method signatures
3. Generate method ID enum/constants
4. Generate client class with ConnectionHandle
5. Implement unary method generation (encode, call, decode)
6. Implement streaming method generation (channel binding)
7. Handle error types properly
8. Test generated client compiles and works

## Success Criteria

1. Generated protocol defines all service methods
2. Generated client implements the protocol
3. Unary methods encode args and decode response correctly
4. Streaming methods bind channels before calling
5. Method IDs match Rust-computed values
6. Error handling is complete and type-safe

## Test Cases

```swift
func testEchoClient() async throws {
    let handle = MockConnectionHandle()
    let client = TestbedClient(handle: handle)
    
    // Mock returns encoded "hello"
    handle.mockResponse = Result<String, RoamError<Never>>.ok("hello").encoded()
    
    let result = try await client.echo(message: "hello")
    XCTAssertEqual(result, "hello")
    
    // Verify request was encoded correctly
    XCTAssertEqual(handle.lastMethodId, TestbedMethodId.echo)
    XCTAssertEqual(handle.lastPayload, "hello".encoded())
}

func testSumClientWithStreaming() async throws {
    let handle = MockConnectionHandle()
    let client = TestbedClient(handle: handle)
    
    // Create channel
    let (tx, rx) = channel(Int32.self)
    
    // Mock returns encoded result
    handle.mockResponse = Result<Int64, RoamError<Never>>.ok(6).encoded()
    
    // Start call (doesn't complete until we send data)
    async let result = client.sum(numbers: rx)
    
    // Send data
    try await tx.send(1)
    try await tx.send(2)
    try await tx.send(3)
    tx.close()
    
    // Now result completes
    let sum = try await result
    XCTAssertEqual(sum, 6)
}
```

## Notes

- Protocol allows for mock implementations in tests
- Client class is `final` for performance
- `ConnectionHandle` is the runtime type for making calls
- Channel binding modifies args in place (mutable)
- Method IDs are computed at codegen time from Rust
- Error types should match Rust `RoamError<E>` variants

## Dependencies

- Phase 005 (Types)
- Phase 006 (Schemas)
- Phase 007 (Encode/Decode)
- Phase 010 (Channel binding) — but can stub for now

## Blocked By

- Phases 005-007 for type and codec generation
