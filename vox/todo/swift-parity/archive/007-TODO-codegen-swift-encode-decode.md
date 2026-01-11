# Phase 007: Code Generation - Swift Encode/Decode

**Status**: TODO

## Objective

Extend `roam-codegen` to generate `PostcardCodable` conformances for Swift types,
providing efficient typed encode/decode without runtime reflection.

## Background

While schema-driven encode/decode (phases 003-004) works with `Any`, generated
protocol conformances are:
1. **Type-safe** — compiler catches type errors
2. **Efficient** — no Mirror reflection, no type casting
3. **Ergonomic** — users can call `point.encode()` directly

## Design

### Protocol Definitions

```swift
// In RoamRuntime
public protocol PostcardEncodable {
    func encode(to encoder: inout PostcardEncoder)
}

public protocol PostcardDecodable {
    init(from decoder: inout PostcardDecoder) throws
}

public typealias PostcardCodable = PostcardEncodable & PostcardDecodable
```

### Generated Struct Conformance

Rust:
```rust
#[derive(Facet)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
```

Generated Swift:
```swift
extension Point: PostcardCodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encodeSignedVarint(Int64(x))
        encoder.encodeSignedVarint(Int64(y))
    }
    
    public init(from decoder: inout PostcardDecoder) throws {
        self.x = Int32(try decoder.decodeSignedVarint())
        self.y = Int32(try decoder.decodeSignedVarint())
    }
}
```

### Generated Enum Conformance

Rust:
```rust
#[derive(Facet)]
pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point,
}
```

Generated Swift:
```swift
extension Shape: PostcardCodable {
    public func encode(to encoder: inout PostcardEncoder) {
        switch self {
        case .circle(let radius):
            encoder.encodeVarint(0) // discriminant
            encoder.encodeF64(radius)
        case .rectangle(let width, let height):
            encoder.encodeVarint(1)
            encoder.encodeF64(width)
            encoder.encodeF64(height)
        case .point:
            encoder.encodeVarint(2)
        }
    }
    
    public init(from decoder: inout PostcardDecoder) throws {
        let discriminant = try decoder.decodeVarint()
        switch discriminant {
        case 0:
            let radius = try decoder.decodeF64()
            self = .circle(radius: radius)
        case 1:
            let width = try decoder.decodeF64()
            let height = try decoder.decodeF64()
            self = .rectangle(width: width, height: height)
        case 2:
            self = .point
        default:
            throw DecodeError.unknownVariant(Int(discriminant))
        }
    }
}
```

### Enum with Explicit Discriminants

Rust:
```rust
#[repr(u8)]
pub enum Message {
    Hello(Hello) = 0,
    Goodbye { reason: String } = 1,
    Cancel { request_id: u64 } = 4, // Note: gap in discriminants
}
```

Generated Swift:
```swift
extension Message: PostcardCodable {
    public func encode(to encoder: inout PostcardEncoder) {
        switch self {
        case .hello(let value):
            encoder.encodeVarint(0)
            value.encode(to: &encoder)
        case .goodbye(let reason):
            encoder.encodeVarint(1)
            encoder.encodeString(reason)
        case .cancel(let requestId):
            encoder.encodeVarint(4) // Explicit discriminant
            encoder.encodeVarint(requestId)
        }
    }
    
    public init(from decoder: inout PostcardDecoder) throws {
        let discriminant = try decoder.decodeVarint()
        switch discriminant {
        case 0:
            self = .hello(try Hello(from: &decoder))
        case 1:
            self = .goodbye(reason: try decoder.decodeString())
        case 4:
            self = .cancel(requestId: try decoder.decodeVarint())
        default:
            throw DecodeError.unknownVariant(Int(discriminant))
        }
    }
}
```

### Container Types

Vec:
```swift
extension Array: PostcardEncodable where Element: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encodeVarint(UInt64(count))
        for element in self {
            element.encode(to: &encoder)
        }
    }
}

extension Array: PostcardDecodable where Element: PostcardDecodable {
    public init(from decoder: inout PostcardDecoder) throws {
        let count = try decoder.decodeVarint()
        self = try (0..<Int(count)).map { _ in try Element(from: &decoder) }
    }
}
```

Option:
```swift
extension Optional: PostcardEncodable where Wrapped: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        switch self {
        case .none:
            encoder.encodeByte(0)
        case .some(let value):
            encoder.encodeByte(1)
            value.encode(to: &encoder)
        }
    }
}

extension Optional: PostcardDecodable where Wrapped: PostcardDecodable {
    public init(from decoder: inout PostcardDecoder) throws {
        let tag = try decoder.decodeByte()
        if tag == 0 {
            self = .none
        } else {
            self = .some(try Wrapped(from: &decoder))
        }
    }
}
```

### Tx/Rx Types

```swift
extension Tx: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encodeVarint(channelId)
    }
}

extension Tx: PostcardDecodable {
    public init(from decoder: inout PostcardDecoder) throws {
        let channelId = try decoder.decodeVarint()
        // Create hollow Tx - binding happens later
        self.init(channelId: channelId)
    }
}

// Same for Rx
```

### Convenience Methods

```swift
extension PostcardEncodable {
    public func encoded() -> [UInt8] {
        var encoder = PostcardEncoder()
        encode(to: &encoder)
        return encoder.bytes
    }
}

extension PostcardDecodable {
    public static func decode(from data: [UInt8]) throws -> Self {
        var decoder = PostcardDecoder(data: data)
        return try Self(from: &decoder)
    }
}
```

## Implementation in roam-codegen

### Encode Generation

```rust
// rust/roam-codegen/src/targets/swift/encode.rs

pub fn generate_encode(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Struct(s) => generate_struct_encode(s),
        ShapeKind::Enum(e) => generate_enum_encode(e),
        // ...
    }
}

fn generate_struct_encode(shape: &StructShape) -> String {
    let mut code = String::new();
    code.push_str("public func encode(to encoder: inout PostcardEncoder) {\n");
    for field in shape.fields {
        code.push_str(&format!("    {}\n", encode_expr(field.name, field.shape)));
    }
    code.push_str("}\n");
    code
}

fn encode_expr(name: &str, shape: &Shape) -> String {
    // Generate the encode call for a field
    match classify_shape(shape) {
        ShapeKind::Scalar(Scalar::I32) => format!("encoder.encodeSignedVarint(Int64({}))", name),
        ShapeKind::Scalar(Scalar::String) => format!("encoder.encodeString({})", name),
        // For nested types that implement PostcardEncodable:
        ShapeKind::Struct(_) | ShapeKind::Enum(_) => format!("{}.encode(to: &encoder)", name),
        // ...
    }
}
```

### Decode Generation

```rust
// rust/roam-codegen/src/targets/swift/decode.rs

pub fn generate_decode(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Struct(s) => generate_struct_decode(s),
        ShapeKind::Enum(e) => generate_enum_decode(e),
        // ...
    }
}

fn generate_struct_decode(shape: &StructShape) -> String {
    let mut code = String::new();
    code.push_str("public init(from decoder: inout PostcardDecoder) throws {\n");
    for field in shape.fields {
        code.push_str(&format!("    self.{} = {}\n", field.name, decode_expr(field.shape)));
    }
    code.push_str("}\n");
    code
}
```

## Files to Modify

| File | Action |
|------|--------|
| `rust/roam-codegen/src/targets/swift/encode.rs` | AUDIT + EXTEND |
| `rust/roam-codegen/src/targets/swift/decode.rs` | AUDIT + EXTEND |
| `swift/roam-runtime/Sources/RoamRuntime/PostcardCodable.swift` | CREATE |

## Implementation Steps

1. Define `PostcardEncodable` and `PostcardDecodable` protocols in runtime
2. Add encode/decode methods to `PostcardEncoder`/`PostcardDecoder`
3. Implement container conformances (Array, Optional, Dictionary)
4. Audit existing encode.rs — what does it generate?
5. Audit existing decode.rs — what does it generate?
6. Generate struct encode/decode implementations
7. Generate enum encode/decode with correct discriminants
8. Handle newtype wrappers
9. Test against golden vectors

## Success Criteria

1. Generated types conform to `PostcardCodable`
2. Encode produces identical bytes to Rust `facet_postcard`
3. Decode correctly parses Rust-encoded bytes
4. Golden vector tests pass
5. Roundtrip: `T.decode(t.encoded()) == t`

## Test Cases

```swift
func testPointEncode() {
    let point = Point(x: 10, y: -5)
    // x=10 zigzag=20, y=-5 zigzag=9
    XCTAssertEqual(point.encoded(), [20, 9])
}

func testPointDecode() throws {
    let point = try Point.decode(from: [20, 9])
    XCTAssertEqual(point.x, 10)
    XCTAssertEqual(point.y, -5)
}

func testPointRoundtrip() throws {
    let original = Point(x: 100, y: -200)
    let decoded = try Point.decode(from: original.encoded())
    XCTAssertEqual(original, decoded)
}

func testMessageEncode() {
    let msg = Message.cancel(requestId: 42)
    // discriminant=4, requestId=42
    XCTAssertEqual(msg.encoded(), [4, 42])
}

func testGoldenVectors() throws {
    // Load golden vectors from test-fixtures/golden-vectors/
    // For each vector, verify encode and decode match
}
```

## Notes

- Discriminants are encoded as varints (not raw bytes)
- Field order must match Rust declaration order
- Explicit discriminants must match Rust `#[repr(u8)]` values
- Nested types call their own encode/decode methods
- Tx/Rx encode as just their channel ID (u64)
- Error handling should produce clear messages

## Dependencies

- Phase 002 (Schema types)
- Phase 003 (PostcardEncoder)
- Phase 004 (PostcardDecoder)
- Phase 005 (Type definitions)

## Blocked By

- Phases 003-005 must be complete
