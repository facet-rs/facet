# Phase 009: Code Generation - Swift Server

**Status**: TODO

## Objective

Extend `roam-codegen` to generate Swift server dispatchers that route incoming
requests to handler implementations.

## Background

TypeScript generates server dispatchers like:
```typescript
class TestbedDispatcher implements ServiceDispatcher {
    async dispatch(methodId, payload, requestId, registry, taskSender) { ... }
}
```

Swift needs equivalent dispatcher generation.

## Design

### Handler Protocol

Generate a protocol that users implement:

```swift
/// Protocol for handling Testbed service requests.
///
/// Note: Tx/Rx types are flipped from the caller's perspective.
/// Where caller has Rx (receives), handler has Tx (sends).
/// Where caller has Tx (sends), handler has Rx (receives).
public protocol TestbedHandler: Sendable {
    /// Echo back the message.
    func echo(message: String) async throws -> String
    
    /// Sum numbers from a stream (receives from caller).
    func sum(numbers: Rx<Int32>) async throws -> Int64
    
    /// Generate numbers to a stream (sends to caller).
    func generate(count: UInt32, output: Tx<Int32>) async throws
    
    /// Transform strings bidirectionally.
    func transform(input: Rx<String>, output: Tx<String>) async throws
}
```

### Dispatcher Implementation

```swift
/// Dispatcher for Testbed service requests.
public final class TestbedDispatcher: ServiceDispatcher, Sendable {
    private let handler: any TestbedHandler
    
    public init(handler: any TestbedHandler) {
        self.handler = handler
    }
    
    public func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskSender: TaskSender
    ) async {
        switch methodId {
        case TestbedMethodId.echo:
            await dispatchEcho(payload: payload, requestId: requestId, taskSender: taskSender)
        case TestbedMethodId.sum:
            await dispatchSum(payload: payload, requestId: requestId, registry: registry, taskSender: taskSender)
        case TestbedMethodId.generate:
            await dispatchGenerate(payload: payload, requestId: requestId, registry: registry, taskSender: taskSender)
        default:
            await sendUnknownMethodError(requestId: requestId, taskSender: taskSender)
        }
    }
    
    private func dispatchEcho(
        payload: [UInt8],
        requestId: UInt64,
        taskSender: TaskSender
    ) async {
        // Decode arguments
        var decoder = PostcardDecoder(data: payload)
        let message: String
        do {
            message = try decoder.decodeString()
        } catch {
            await sendInvalidPayloadError(requestId: requestId, taskSender: taskSender)
            return
        }
        
        // Call handler
        do {
            let result = try await handler.echo(message: message)
            
            // Encode and send response
            let response = Result<String, RoamError<Never>>.ok(result).encoded()
            await taskSender.response(requestId: requestId, payload: response)
        } catch {
            // Send error response
            let response = Result<String, RoamError<Never>>.err(.user(/* ??? */)).encoded()
            await taskSender.response(requestId: requestId, payload: response)
        }
    }
    
    private func dispatchSum(
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskSender: TaskSender
    ) async {
        // Decode arguments (channel ID)
        var decoder = PostcardDecoder(data: payload)
        let channelId: UInt64
        do {
            channelId = try decoder.decodeVarint()
        } catch {
            await sendInvalidPayloadError(requestId: requestId, taskSender: taskSender)
            return
        }
        
        // Create server-side Rx (receives from caller's Tx)
        let (tx, rx) = channel(Int32.self)
        registry.registerIncoming(channelId: channelId, sender: tx)
        
        // Call handler
        do {
            let result = try await handler.sum(numbers: rx)
            
            // Encode and send response
            let response = Result<Int64, RoamError<Never>>.ok(result).encoded()
            await taskSender.response(requestId: requestId, payload: response)
        } catch {
            let response = Result<Int64, RoamError<Never>>.err(.cancelled).encoded()
            await taskSender.response(requestId: requestId, payload: response)
        }
    }
    
    private func dispatchGenerate(
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskSender: TaskSender
    ) async {
        // Decode arguments
        var decoder = PostcardDecoder(data: payload)
        let count: UInt32
        let channelId: UInt64
        do {
            count = UInt32(try decoder.decodeVarint())
            channelId = try decoder.decodeVarint()
        } catch {
            await sendInvalidPayloadError(requestId: requestId, taskSender: taskSender)
            return
        }
        
        // Create server-side Tx (sends to caller's Rx)
        let output = Tx<Int32>(channelId: channelId, taskSender: taskSender)
        
        // Call handler
        do {
            try await handler.generate(count: count, output: output)
            
            // Response sent after handler completes (Tx dropped sends Close)
            let response = Result<(), RoamError<Never>>.ok(()).encoded()
            await taskSender.response(requestId: requestId, payload: response)
        } catch {
            let response = Result<(), RoamError<Never>>.err(.cancelled).encoded()
            await taskSender.response(requestId: requestId, payload: response)
        }
    }
    
    private func sendUnknownMethodError(requestId: UInt64, taskSender: TaskSender) async {
        let response = [UInt8]([1, 1]) // Err(UnknownMethod)
        await taskSender.response(requestId: requestId, payload: response)
    }
    
    private func sendInvalidPayloadError(requestId: UInt64, taskSender: TaskSender) async {
        let response = [UInt8]([1, 2]) // Err(InvalidPayload)
        await taskSender.response(requestId: requestId, payload: response)
    }
}
```

### ServiceDispatcher Protocol

```swift
/// Protocol for dispatching incoming requests.
public protocol ServiceDispatcher: Sendable {
    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskSender: TaskSender
    ) async
}
```

### TaskSender

```swift
/// Sender for task messages (Data, Close, Response).
public actor TaskSender {
    private let channel: AsyncChannel<TaskMessage>
    
    public func data(channelId: UInt64, payload: [UInt8]) async {
        await channel.send(.data(channelId: channelId, payload: payload))
    }
    
    public func close(channelId: UInt64) async {
        await channel.send(.close(channelId: channelId))
    }
    
    public func response(requestId: UInt64, payload: [UInt8]) async {
        await channel.send(.response(requestId: requestId, payload: payload))
    }
}
```

### Type Flipping

Key insight: In the handler, Tx/Rx are flipped from the service definition.

Service definition (caller's perspective):
```swift
func sum(numbers: Tx<Int32>) -> Int64  // caller sends via Tx
```

Handler (callee's perspective):
```swift
func sum(numbers: Rx<Int32>) -> Int64  // callee receives via Rx
```

The codegen must:
1. Flip `Tx<T>` → `Rx<T>` in handler protocol
2. Flip `Rx<T>` → `Tx<T>` in handler protocol
3. Create appropriate channel types when dispatching

## Implementation in roam-codegen

### Server Generation

```rust
// rust/roam-codegen/src/targets/swift/server.rs

pub fn generate_server(service: &ServiceDetail) -> String {
    let mut code = String::new();
    
    // Generate handler protocol (with flipped Tx/Rx)
    code.push_str(&generate_handler_protocol(service));
    
    // Generate dispatcher class
    code.push_str(&generate_dispatcher(service));
    
    code
}

fn generate_handler_protocol(service: &ServiceDetail) -> String {
    // Generate TestbedHandler with async methods
    // Flip Tx<T> to Rx<T> and vice versa
}

fn generate_dispatcher(service: &ServiceDetail) -> String {
    // Generate TestbedDispatcher implementing ServiceDispatcher
}

fn generate_dispatch_method(method: &MethodDetail) -> String {
    // Generate dispatchMethodName function
    // Handle argument decoding, channel binding, handler call, response encoding
}

fn flip_stream_type(shape: &Shape) -> &Shape {
    // Tx<T> becomes Rx<T>, Rx<T> becomes Tx<T>
}
```

## Files to Modify

| File | Action |
|------|--------|
| `rust/roam-codegen/src/targets/swift/server.rs` | AUDIT + EXTEND |
| `rust/roam-codegen/src/targets/swift/mod.rs` | Integrate server generation |

## Implementation Steps

1. Audit existing `server.rs` — what does it generate?
2. Generate handler protocol with flipped Tx/Rx types
3. Generate dispatcher class with method switch
4. Implement dispatch function generation for each method
5. Handle channel binding for streaming methods
6. Handle error responses (UnknownMethod, InvalidPayload)
7. Test generated dispatcher compiles and works

## Success Criteria

1. Generated handler protocol has correct method signatures
2. Tx/Rx types are correctly flipped in handler
3. Dispatcher routes to correct dispatch function
4. Arguments are decoded correctly
5. Channels are bound before calling handler
6. Response is encoded and sent via TaskSender
7. Error cases handled properly

## Test Cases

```swift
class MockTestbedHandler: TestbedHandler {
    var echoMessages: [String] = []
    var sumResults: [Int64] = []
    
    func echo(message: String) async throws -> String {
        echoMessages.append(message)
        return message
    }
    
    func sum(numbers: Rx<Int32>) async throws -> Int64 {
        var total: Int64 = 0
        for try await n in numbers {
            total += Int64(n)
        }
        sumResults.append(total)
        return total
    }
    
    func generate(count: UInt32, output: Tx<Int32>) async throws {
        for i in 0..<Int32(count) {
            try await output.send(i)
        }
    }
}

func testEchoDispatch() async throws {
    let handler = MockTestbedHandler()
    let dispatcher = TestbedDispatcher(handler: handler)
    let taskSender = MockTaskSender()
    let registry = ChannelRegistry()
    
    // Encode "hello"
    let payload = "hello".encoded()
    
    await dispatcher.dispatch(
        methodId: TestbedMethodId.echo,
        payload: payload,
        requestId: 1,
        registry: registry,
        taskSender: taskSender
    )
    
    XCTAssertEqual(handler.echoMessages, ["hello"])
    XCTAssertEqual(taskSender.responses.count, 1)
    
    // Decode response
    let response = taskSender.responses[0]
    let result = try Result<String, RoamError<Never>>.decode(from: response.payload)
    XCTAssertEqual(result, .ok("hello"))
}

func testUnknownMethod() async throws {
    let handler = MockTestbedHandler()
    let dispatcher = TestbedDispatcher(handler: handler)
    let taskSender = MockTaskSender()
    let registry = ChannelRegistry()
    
    await dispatcher.dispatch(
        methodId: 0xDEADBEEF, // Unknown
        payload: [],
        requestId: 1,
        registry: registry,
        taskSender: taskSender
    )
    
    let response = taskSender.responses[0]
    let result = try Result<(), RoamError<Never>>.decode(from: response.payload)
    XCTAssertEqual(result, .err(.unknownMethod))
}
```

## Notes

- Handler protocol is what users implement
- Dispatcher is generated and handles all the plumbing
- Tx/Rx flipping is crucial for correct channel semantics
- TaskSender ensures correct message ordering (Data/Close before Response)
- Errors should not crash — always send an error response
- Consider spawning handler calls as separate tasks for concurrency

## Dependencies

- Phase 005 (Types)
- Phase 006 (Schemas)
- Phase 007 (Encode/Decode)
- Phase 010 (Channel binding)

## Blocked By

- Phases 005-007 for type and codec generation
