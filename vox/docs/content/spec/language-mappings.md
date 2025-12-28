+++
title = "Language Mappings"
description = "Type mappings for Rust, Swift, TypeScript, Go, and Java"
weight = 70
+++

This document defines how Rapace types map to different programming languages. These mappings are implemented by the code generators and ensure consistent semantics across language boundaries.

> **Implementation Status**: Swift and TypeScript codegen exist. Go and Java mappings are specified for future implementation.

## Design Principles

r[langmap.semantic]
Types MUST preserve the same encoding/decoding behavior across all languages.

r[langmap.idiomatic]
Generated code SHOULD follow target language conventions.

r[langmap.roundtrip]
A value encoded in one language MUST decode identically in another.

r[langmap.lossy]
Lossy mappings (e.g., i128 → bigint) MUST be documented.

## Naming Conventions

### Identifier Transformation

| Source (Rust) | Swift | TypeScript | Go | Java |
|---------------|-------|------------|-----|------|
| `snake_case` field | `snake_case` | `snake_case` | `PascalCase` | `camelCase` |
| `snake_case` method | `camelCase` | `camelCase` | `PascalCase` | `camelCase` |
| `SCREAMING_SNAKE` variant | `camelCase` | `camelCase` | `PascalCase` | `SCREAMING_SNAKE` |
| `PascalCase` type | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` |

Go uses PascalCase for exported fields/methods. Java preserves SCREAMING_SNAKE for enum constants.

The `to_camel_case` transformation:
- Removes underscores
- Capitalizes the character following each underscore
- First character is lowercase for methods/variants

Example: `get_user_by_id` → `getUserById`

### Module Path Handling

Type identifiers include the full Rust path (e.g., `my_proto::messages::UserInfo`). Code generators extract only the final component:

```rust
fn clean_type_name(name: &str) -> String {
    name.rsplit("::").next().unwrap_or(name).to_string()
}
```

This means types from different modules with the same name will collide. Rapace does not currently namespace generated types.

## Scalar Type Mappings

### Integer Types

| Rust | Swift | TypeScript | Go | Java | Notes |
|------|-------|------------|-----|------|-------|
| `i8` | `Int8` | `number` | `int8` | `byte` | Java byte is signed |
| `i16` | `Int16` | `number` | `int16` | `short` | |
| `i32` | `Int32` | `number` | `int32` | `int` | |
| `i64` | `Int64` | `bigint` | `int64` | `long` | |
| `i128` | `Int64` | `bigint` | `*big.Int` | `BigInteger` | **Lossy** in Swift |
| `isize` | `Int` | `bigint` | `int` | `long` | **Internal only** (see note) |
| `u8` | `UInt8` | `number` | `uint8` | `int` | Java lacks unsigned |
| `u16` | `UInt16` | `number` | `uint16` | `int` | Java lacks unsigned |
| `u32` | `UInt32` | `number` | `uint32` | `long` | Java lacks unsigned |
| `u64` | `UInt64` | `bigint` | `uint64` | `BigInteger` | Java lacks unsigned |
| `u128` | `UInt64` | `bigint` | `*big.Int` | `BigInteger` | **Lossy** in Swift |
| `usize` | `UInt` | `bigint` | `uint` | `long` | **Internal only** (see note) |

r[langmap.java.unsigned]
Since Java lacks unsigned types, u8/u16 use wider signed types. Code MUST mask values appropriately when encoding.

r[langmap.usize.prohibited]
`usize` and `isize` types are prohibited in public service APIs because they have platform-dependent sizes (see [Data Model: Explicitly Unsupported](@/spec/data-model.md#explicitly-unsupported)). The mappings above exist only for internal implementation use (e.g., container lengths). Code generators MUST reject `usize`/`isize` in method signatures and public struct fields.

r[langmap.i128.swift]
Swift's `Int64`/`UInt64` cannot represent the full range of 128-bit integers. Implementations MUST:
1. **On encode**: If the value exceeds the target type's range, the encoder MUST fail with an error (not silently truncate)
2. **On decode**: The decoder reads a 128-bit value; if it exceeds `Int64.max`/`UInt64.max`, decoding MUST fail with an error
3. **Alternative**: Consider using `Decimal` or a custom `Int128` struct if full range is required

TypeScript's `bigint` and Go's `*big.Int` support the full 128-bit range without loss.

### Floating Point

| Rust | Swift | TypeScript | Go | Java | Notes |
|------|-------|------------|-----|------|-------|
| `f32` | `Float` | `number` | `float32` | `float` | IEEE 754 single |
| `f64` | `Double` | `number` | `float64` | `double` | IEEE 754 double |

NaN values are canonicalized on the wire (see [Data Model](@/spec/data-model.md#floating-point)).

### Other Primitives

| Rust | Swift | TypeScript | Go | Java | Notes |
|------|-------|------------|-----|------|-------|
| `bool` | `Bool` | `boolean` | `bool` | `boolean` | |
| `char` | `UInt32` | `number` | `rune` | `int` | Unicode scalar (varint u32) |
| `String` | `String` | `string` | `string` | `String` | UTF-8 |
| `&str` | `String` | `string` | `string` | `String` | UTF-8 |
| `Cow<str>` | `String` | `string` | `string` | `String` | UTF-8 |
| `()` | `Void` | `void` | `struct{}` | `Void`/`void` | Unit type |

**Note on `char`**: Rust's `char` is a Unicode scalar value (U+0000–U+D7FF or U+E000–U+10FFFF), encoded on the wire as a varint of its u32 value. This is NOT the same as a single-character string. Languages that lack a native "Unicode scalar" type should use an integer type and validate the range.

## Container Type Mappings

### Option

| Rust | Swift | TypeScript | Go | Java |
|------|-------|------------|-----|------|
| `Option<T>` | `T?` | `T \| null` | `*T` | `Optional<T>` or `@Nullable T` |

Wire encoding: `0x00` for `None`, `0x01` + value for `Some`.

**Go**: Uses pointer types for optional values. Nil pointer = None.

**Java**: Can use `Optional<T>` for object types, `@Nullable` annotations, or boxed primitives (`Integer` instead of `int`).

### Vec / Slice

| Rust | Swift | TypeScript | Go | Java | Notes |
|------|-------|------------|-----|------|-------|
| `Vec<T>` | `[T]` | `T[]` | `[]T` | `List<T>` | General case |
| `Vec<u8>` | `[UInt8]` | `Uint8Array` | `[]byte` | `byte[]` | Optimized bytes |
| `&[T]` | `[T]` | `T[]` | `[]T` | `List<T>` | Borrowed → owned |

Wire encoding: varint length + elements.

### Arrays (Fixed-Size)

| Rust | Swift | TypeScript | Go | Java | Notes |
|------|-------|------------|-----|------|-------|
| `[T; N]` | `[T]` | `T[]` | `[N]T` | `T[]` | Go has fixed arrays |

Wire encoding: N elements, no length prefix.

**Go** is unique in having true fixed-size arrays. Generated code SHOULD use `[N]T` for fixed arrays.

### HashMap / BTreeMap

| Rust | Swift | TypeScript | Go | Java |
|------|-------|------------|-----|------|
| `HashMap<K, V>` | `[K: V]` | `Map<K, V>` | `map[K]V` | `Map<K, V>` |
| `BTreeMap<K, V>` | `[K: V]` | `Map<K, V>` | `map[K]V` | `TreeMap<K, V>` |

Wire encoding: varint count + key-value pairs. **Order is NOT canonical.**

### Result

| Rust | Swift | TypeScript | Go | Java |
|------|-------|------------|-----|------|
| `Result<T, E>` | `Result<T, E>` | `{ ok: true; value: T } \| { ok: false; error: E }` | `(T, error)` | `Result<T, E>` class |

**Go**: Uses idiomatic `(value, error)` return tuples. The generated Result struct can wrap this pattern.

**Java**: Requires a custom `Result<T, E>` class with `isOk()`, `value()`, `error()` methods, or use a library like Vavr.

TypeScript uses discriminated unions since it lacks a native Result type.

## Struct Mappings

Rust structs map to:
- **Swift**: `struct` with public fields
- **TypeScript**: `interface`
- **Go**: `struct` with exported fields
- **Java**: `class` with public fields or `record` (Java 16+)

### Swift Struct Generation

```swift
public struct UserInfo: PostcardEncodable, Sendable {
    public var name: String
    public var age: UInt32
    
    public init(name: String, age: UInt32) {
        self.name = name
        self.age = age
    }
    
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(name)
        encoder.encode(age)
    }
}
```

Key points:
- Conforms to `PostcardEncodable` and `Sendable`
- Public memberwise initializer
- Fields encoded in declaration order

### TypeScript Interface Generation

```typescript
export interface UserInfo {
    name: string;
    age: number;
}

export function encodeUserInfo(encoder: PostcardEncoder, value: UserInfo): void {
    encoder.string(value.name);
    encoder.u32(value.age);
}

export function decodeUserInfo(decoder: PostcardDecoder): UserInfo {
    return {
        name: decoder.string(),
        age: decoder.u32(),
    };
}
```

Key points:
- Separate encode/decode functions (not methods)
- Fields decoded in declaration order

### Go Struct Generation

```go
type UserInfo struct {
    Name string
    Age  uint32
}

func (u *UserInfo) Encode(enc *postcard.Encoder) error {
    enc.String(u.Name)
    enc.Uint32(u.Age)
    return nil
}

func DecodeUserInfo(dec *postcard.Decoder) (*UserInfo, error) {
    name, err := dec.String()
    if err != nil {
        return nil, err
    }
    age, err := dec.Uint32()
    if err != nil {
        return nil, err
    }
    return &UserInfo{Name: name, Age: age}, nil
}
```

Key points:
- Exported fields (PascalCase)
- Pointer receiver for encode method
- Constructor function returns pointer and error

### Java Struct Generation

```java
public record UserInfo(String name, int age) {
    public void encode(PostcardEncoder encoder) {
        encoder.string(name);
        encoder.uint32(age);
    }

    public static UserInfo decode(PostcardDecoder decoder) {
        String name = decoder.string();
        int age = decoder.uint32();
        return new UserInfo(name, age);
    }
}
```

For pre-Java 16, use a class:

```java
public final class UserInfo {
    private final String name;
    private final int age;

    public UserInfo(String name, int age) {
        this.name = name;
        this.age = age;
    }

    public String getName() { return name; }
    public int getAge() { return age; }

    // encode/decode methods...
}
```

Key points:
- Prefer `record` for Java 16+
- Immutable by default
- Static `decode` factory method

## Enum Mappings

### Unit Variants

```rust
enum Status { Pending, Active, Closed }
```

**Swift:**
```swift
public enum Status: PostcardEncodable, Sendable {
    case pending
    case active
    case closed
    
    public func encode(to encoder: inout PostcardEncoder) {
        switch self {
        case .pending: encoder.encode(UInt32(0))
        case .active: encoder.encode(UInt32(1))
        case .closed: encoder.encode(UInt32(2))
        }
    }
}
```

**TypeScript:**
```typescript
export type Status =
    { type: "pending" } |
    { type: "active" } |
    { type: "closed" };
```

TypeScript uses discriminated unions with a `type` field for all enums.

**Go:**
```go
type Status int

const (
    StatusPending Status = iota
    StatusActive
    StatusClosed
)

func (s Status) Encode(enc *postcard.Encoder) error {
    return enc.Uint32(uint32(s))
}
```

**Java:**
```java
public enum Status {
    PENDING(0),
    ACTIVE(1),
    CLOSED(2);

    private final int discriminant;
    Status(int discriminant) { this.discriminant = discriminant; }

    public void encode(PostcardEncoder encoder) {
        encoder.uint32(discriminant);
    }
}
```

### Newtype Variants

```rust
enum Message { Text(String), Binary(Vec<u8>) }
```

**Swift:**
```swift
public enum Message: PostcardEncodable, Sendable {
    case text(String)
    case binary([UInt8])
}
```

**TypeScript:**
```typescript
export type Message =
    { type: "text"; value: string } |
    { type: "binary"; value: Uint8Array };
```

### Struct Variants

```rust
enum Event { 
    Click { x: i32, y: i32 },
    KeyPress { key: String, modifiers: u32 }
}
```

**Swift:**
```swift
public enum Event: PostcardEncodable, Sendable {
    case click(x: Int32, y: Int32)
    case keyPress(key: String, modifiers: UInt32)
}
```

**TypeScript:**
```typescript
export type Event =
    { type: "click"; x: number; y: number } |
    { type: "keyPress"; key: string; modifiers: number };
```

**Go** (using interfaces for sum types):
```go
type Event interface {
    isEvent()
    Encode(enc *postcard.Encoder) error
}

type EventClick struct {
    X int32
    Y int32
}
func (EventClick) isEvent() {}

type EventKeyPress struct {
    Key       string
    Modifiers uint32
}
func (EventKeyPress) isEvent() {}
```

**Java** (sealed interfaces, Java 17+):
```java
public sealed interface Event permits Event.Click, Event.KeyPress {
    void encode(PostcardEncoder encoder);

    record Click(int x, int y) implements Event {
        public void encode(PostcardEncoder encoder) {
            encoder.uint32(0);  // discriminant
            encoder.int32(x);
            encoder.int32(y);
        }
    }

    record KeyPress(String key, int modifiers) implements Event {
        public void encode(PostcardEncoder encoder) {
            encoder.uint32(1);  // discriminant
            encoder.string(key);
            encoder.uint32(modifiers);
        }
    }
}
```

For pre-Java 17, use an abstract class with subclasses.

### Discriminant Encoding

r[langmap.enum.discriminant]
Enum variants MUST be encoded as varint discriminants (0, 1, 2, ...) followed by payload fields. The discriminant MUST be the declaration order, NOT any explicit `#[repr]` value.

## Client Generation

### Swift Client

Clients are generated as Swift `actor` types for safe concurrency:

```swift
public actor InventoryClient {
    private let client: RapaceClient
    
    public init(client: RapaceClient) {
        self.client = client
    }
    
    public init(host: String, port: UInt16) async throws {
        self.client = try await RapaceClient(host: host, port: port)
    }
    
    public func getItem(_ id: UInt64) async throws -> Item {
        var encoder = PostcardEncoder()
        encoder.encode(id)
        let response = try await client.call(
            methodId: 0x12345678,
            requestPayload: encoder.bytes
        )
        // ... decode response
    }
}
```

Key points:
- `actor` for thread safety
- `async throws` methods
- Method ID is computed at codegen time

### TypeScript Client

Clients are generated as ES6 classes:

```typescript
export class InventoryClient {
    private client: RapaceClient;
    
    constructor(client: RapaceClient) {
        this.client = client;
    }
    
    static async connect(url: string): Promise<InventoryClient> {
        const client = await RapaceClient.connect(url);
        return new InventoryClient(client);
    }
    
    async getItem(id: bigint): Promise<Item> {
        const encoder = new PostcardEncoder();
        encoder.u64(id);
        const response = await this.client.call(0x12345678, encoder.bytes);
        const decoder = new PostcardDecoder(response);
        return decodeItem(decoder);
    }
    
    close(): void {
        this.client.close();
    }
}
```

Key points:
- Static `connect` factory method
- Explicit `close()` for resource cleanup
- `async` methods returning `Promise<T>`

### Go Client

```go
type InventoryClient struct {
    client *rapace.Client
}

func NewInventoryClient(client *rapace.Client) *InventoryClient {
    return &InventoryClient{client: client}
}

func DialInventory(ctx context.Context, addr string) (*InventoryClient, error) {
    client, err := rapace.Dial(ctx, addr)
    if err != nil {
        return nil, err
    }
    return NewInventoryClient(client), nil
}

func (c *InventoryClient) GetItem(ctx context.Context, id uint64) (*Item, error) {
    enc := postcard.NewEncoder()
    enc.Uint64(id)

    resp, err := c.client.Call(ctx, 0x12345678, enc.Bytes())
    if err != nil {
        return nil, err
    }

    return DecodeItem(postcard.NewDecoder(resp))
}

func (c *InventoryClient) Close() error {
    return c.client.Close()
}
```

Key points:
- `context.Context` for cancellation/deadlines
- `Dial` function for connection
- Methods return `(T, error)` tuples

### Java Client

```java
public class InventoryClient implements AutoCloseable {
    private final RapaceClient client;

    public InventoryClient(RapaceClient client) {
        this.client = client;
    }

    public static CompletableFuture<InventoryClient> connect(String url) {
        return RapaceClient.connect(url)
            .thenApply(InventoryClient::new);
    }

    public CompletableFuture<Item> getItem(long id) {
        PostcardEncoder encoder = new PostcardEncoder();
        encoder.uint64(id);

        return client.call(0x12345678, encoder.bytes())
            .thenApply(response -> {
                PostcardDecoder decoder = new PostcardDecoder(response);
                return Item.decode(decoder);
            });
    }

    @Override
    public void close() {
        client.close();
    }
}
```

Key points:
- `CompletableFuture<T>` for async operations
- Implements `AutoCloseable` for try-with-resources
- Method IDs as hex literals

## Streaming Methods

Streaming uses attached STREAM channels as defined in [Core Protocol: STREAM Channels](@/spec/core.md#stream-channels). Each language maps this to idiomatic async iteration patterns:

| Language | Server Streaming | Client Streaming | Bidirectional |
|----------|------------------|------------------|---------------|
| Swift | `AsyncThrowingStream<T, Error>` | `AsyncStream<T>` consumer | Both |
| TypeScript | `AsyncIterable<T>` | `AsyncIterable<T>` | Both |
| Go | `<-chan T` or iterator | `chan<- T` | Both via channels |
| Java | `Flow.Publisher<T>` | `Flow.Subscriber<T>` | Reactive Streams |

### Protocol Mapping

All languages implement the same wire protocol:

1. **Server streaming** (`-> Stream<T>`): Server opens STREAM channel on port 101+, sends items, closes with EOS
2. **Client streaming** (`Stream<T>` arg): Client opens STREAM channel on port 1+, sends items, closes with EOS
3. **Bidirectional**: Both sides can send items; each closes independently with EOS

### Swift Streaming

```swift
public func subscribe(topic: String) async throws -> AsyncThrowingStream<Event, Error> {
    // Opens CALL channel, server opens attached STREAM channel
    // Returns stream that yields items from the STREAM channel
}
```

### TypeScript Streaming

```typescript
async *subscribe(topic: string): AsyncIterable<Event> {
    // Opens CALL channel, server opens attached STREAM channel
    // Yields items from the STREAM channel
}
```

### Go Streaming

```go
func (c *EventClient) Subscribe(ctx context.Context, topic string) (<-chan Event, error) {
    // Opens CALL channel, server opens attached STREAM channel
    // Returns channel that receives items from the STREAM channel
}
```

### Java Streaming

```java
public Flow.Publisher<Event> subscribe(String topic) {
    // Opens CALL channel, server opens attached STREAM channel
    // Returns publisher that emits items from the STREAM channel
}
```

## Type Aliases

Type aliases are resolved at codegen time. For example:

```rust
type ItemId = u64;
```

Both generators recognize `ItemId` and map it:
- **Swift**: `UInt64`
- **TypeScript**: `bigint`

Custom type aliases should be handled in `arg_type_to_swift` / `arg_type_to_ts`.

## Tuple Types

Tuple types (e.g., `(u32, String)`) are NOT generated as named types. The generator collects their inner types but skips generating the tuple itself:

```rust
if shape.type_identifier.starts_with('(') {
    // collect inner types, but don't generate
    return;
}
```

Methods with tuple arguments should use struct types instead for cross-language compatibility.

## Error Handling

Encoding/decoding errors are handled differently per language:

| Language | Error Mechanism | RPC Errors |
|----------|-----------------|------------|
| Swift | `throws` | `async throws` with `RpcError` |
| TypeScript | `throw Error(...)` | `Promise` rejection |
| Go | `(T, error)` return | Same pattern |
| Java | Checked exceptions or `Result<T, E>` | `CompletableFuture` failure |

### Unknown Enum Variants

| Language | Behavior |
|----------|----------|
| Swift | `fatalError("Unknown Foo variant: \(index)")` |
| TypeScript | `throw new Error(\`Unknown Foo discriminant: ${d}\`)` |
| Go | `return nil, fmt.Errorf("unknown Foo variant: %d", d)` |
| Java | `throw new IllegalArgumentException("Unknown Foo variant: " + d)` |

### RPC Error Mapping

The `Status` type from [Error Handling](@/spec/errors.md) maps to:

| Language | Type |
|----------|------|
| Swift | `RpcError` enum with associated values |
| TypeScript | `RpcError` class with `code`, `message`, `details` |
| Go | `*RpcError` struct implementing `error` |
| Java | `RpcException` extending `Exception` |

## Next Steps

- [Code Generation](@/spec/codegen.md) – Architecture of the code generators
- [Data Model](@/spec/data-model.md) – Source type definitions
- [Payload Encoding](@/spec/payload-encoding.md) – Wire format details
