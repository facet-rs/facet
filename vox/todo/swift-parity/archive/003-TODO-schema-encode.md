# Phase 003: Schema-Driven Encode in Swift

**Status**: TODO

## Objective

Implement `encodeWithSchema()` that encodes any Swift value to postcard format using
a schema, without requiring the value to conform to any protocol.

## Background

TypeScript has `encodeWithSchema(value, schema, registry)` in `schema_codec.ts`.
This function walks the schema and value in parallel, encoding each piece according
to the schema.

Swift needs the same capability for:
1. Encoding generated types that may not have protocol conformances yet
2. Encoding values where the schema is only known at runtime
3. Testing schema-driven encoding against golden vectors

## Design

### Core Function

```swift
/// Encode a value to postcard format using a schema.
///
/// - Parameters:
///   - value: The value to encode (as `Any`)
///   - schema: Schema describing the value's wire format
///   - registry: Registry for resolving type references
/// - Returns: Encoded bytes
/// - Throws: `EncodeError` if encoding fails
public func encodeWithSchema(
    _ value: Any,
    schema: Schema,
    registry: SchemaRegistry = SchemaRegistry()
) throws -> [UInt8] {
    var encoder = PostcardEncoder()
    try encodeValue(value, schema: schema, encoder: &encoder, registry: registry)
    return encoder.bytes
}
```

### PostcardEncoder

```swift
/// Low-level postcard encoder that writes to a byte buffer.
public struct PostcardEncoder {
    public private(set) var bytes: [UInt8] = []
    
    public init() {}
    
    public mutating func writeByte(_ byte: UInt8) {
        bytes.append(byte)
    }
    
    public mutating func writeBytes(_ data: [UInt8]) {
        bytes.append(contentsOf: data)
    }
    
    public mutating func writeVarint(_ value: UInt64) {
        var v = value
        while v >= 0x80 {
            bytes.append(UInt8(v & 0x7F) | 0x80)
            v >>= 7
        }
        bytes.append(UInt8(v))
    }
    
    public mutating func writeSignedVarint(_ value: Int64) {
        // Zigzag encoding: (value << 1) ^ (value >> 63)
        let zigzag = UInt64(bitPattern: (value << 1) ^ (value >> 63))
        writeVarint(zigzag)
    }
}
```

### Encoding Logic

```swift
private func encodeValue(
    _ value: Any,
    schema: Schema,
    encoder: inout PostcardEncoder,
    registry: SchemaRegistry
) throws {
    // Resolve references first
    let resolvedSchema: Schema
    if case .ref(let name) = schema {
        guard let resolved = registry.resolve(name: name) else {
            throw EncodeError.unknownType(name)
        }
        resolvedSchema = resolved
    } else {
        resolvedSchema = schema
    }
    
    switch resolvedSchema {
    // Primitives
    case .bool:
        guard let v = value as? Bool else { throw EncodeError.typeMismatch(expected: "Bool") }
        encoder.writeByte(v ? 1 : 0)
        
    case .u8:
        guard let v = value as? UInt8 else { throw EncodeError.typeMismatch(expected: "UInt8") }
        encoder.writeByte(v)
        
    case .u16:
        guard let v = value as? UInt16 else { throw EncodeError.typeMismatch(expected: "UInt16") }
        encoder.writeVarint(UInt64(v))
        
    case .u32:
        guard let v = value as? UInt32 else { throw EncodeError.typeMismatch(expected: "UInt32") }
        encoder.writeVarint(UInt64(v))
        
    case .u64:
        guard let v = value as? UInt64 else { throw EncodeError.typeMismatch(expected: "UInt64") }
        encoder.writeVarint(v)
        
    case .i8:
        guard let v = value as? Int8 else { throw EncodeError.typeMismatch(expected: "Int8") }
        encoder.writeByte(UInt8(bitPattern: v))
        
    case .i16:
        guard let v = value as? Int16 else { throw EncodeError.typeMismatch(expected: "Int16") }
        encoder.writeSignedVarint(Int64(v))
        
    case .i32:
        guard let v = value as? Int32 else { throw EncodeError.typeMismatch(expected: "Int32") }
        encoder.writeSignedVarint(Int64(v))
        
    case .i64:
        guard let v = value as? Int64 else { throw EncodeError.typeMismatch(expected: "Int64") }
        encoder.writeSignedVarint(v)
        
    case .f32:
        guard let v = value as? Float else { throw EncodeError.typeMismatch(expected: "Float") }
        var bits = v.bitPattern.littleEndian
        withUnsafeBytes(of: &bits) { encoder.writeBytes(Array($0)) }
        
    case .f64:
        guard let v = value as? Double else { throw EncodeError.typeMismatch(expected: "Double") }
        var bits = v.bitPattern.littleEndian
        withUnsafeBytes(of: &bits) { encoder.writeBytes(Array($0)) }
        
    case .string:
        guard let v = value as? String else { throw EncodeError.typeMismatch(expected: "String") }
        let utf8 = Array(v.utf8)
        encoder.writeVarint(UInt64(utf8.count))
        encoder.writeBytes(utf8)
        
    case .bytes:
        guard let v = value as? [UInt8] else { throw EncodeError.typeMismatch(expected: "[UInt8]") }
        encoder.writeVarint(UInt64(v.count))
        encoder.writeBytes(v)
        
    // Containers
    case .vec(let element):
        guard let arr = value as? [Any] else { throw EncodeError.typeMismatch(expected: "Array") }
        encoder.writeVarint(UInt64(arr.count))
        for item in arr {
            try encodeValue(item, schema: element, encoder: &encoder, registry: registry)
        }
        
    case .option(let inner):
        if let opt = value as? OptionalProtocol {
            if let unwrapped = opt.wrappedValue {
                encoder.writeByte(1)
                try encodeValue(unwrapped, schema: inner, encoder: &encoder, registry: registry)
            } else {
                encoder.writeByte(0)
            }
        } else {
            // Value is not optional, treat as Some
            encoder.writeByte(1)
            try encodeValue(value, schema: inner, encoder: &encoder, registry: registry)
        }
        
    case .map(let keySchema, let valueSchema):
        guard let dict = value as? [AnyHashable: Any] else { 
            throw EncodeError.typeMismatch(expected: "Dictionary") 
        }
        encoder.writeVarint(UInt64(dict.count))
        for (k, v) in dict {
            try encodeValue(k, schema: keySchema, encoder: &encoder, registry: registry)
            try encodeValue(v, schema: valueSchema, encoder: &encoder, registry: registry)
        }
        
    // Composites
    case .struct(let structSchema):
        try encodeStruct(value, schema: structSchema, encoder: &encoder, registry: registry)
        
    case .enum(let enumSchema):
        try encodeEnum(value, schema: enumSchema, encoder: &encoder, registry: registry)
        
    case .tuple(let elements):
        try encodeTuple(value, elements: elements, encoder: &encoder, registry: registry)
        
    // Streaming (encode as channel ID)
    case .tx, .rx:
        // Tx and Rx serialize as their channel ID (u64)
        if let channelId = extractChannelId(from: value) {
            encoder.writeVarint(channelId)
        } else {
            throw EncodeError.typeMismatch(expected: "Tx or Rx with channelId")
        }
        
    case .ref:
        // Already handled above
        fatalError("Unreachable: ref should have been resolved")
    }
}
```

### Struct Encoding

```swift
private func encodeStruct(
    _ value: Any,
    schema: StructSchema,
    encoder: inout PostcardEncoder,
    registry: SchemaRegistry
) throws {
    // Try to access fields via protocol or Mirror
    let mirror = Mirror(reflecting: value)
    
    for (fieldName, fieldSchema) in schema.fields {
        guard let child = mirror.children.first(where: { $0.label == fieldName }) else {
            throw EncodeError.missingField(fieldName)
        }
        try encodeValue(child.value, schema: fieldSchema, encoder: &encoder, registry: registry)
    }
}
```

### Enum Encoding

```swift
private func encodeEnum(
    _ value: Any,
    schema: EnumSchema,
    encoder: inout PostcardEncoder,
    registry: SchemaRegistry
) throws {
    // For generated enums, use SchemaEncodable protocol
    if let encodable = value as? SchemaEncodable {
        let (variantName, fields) = encodable.schemaEncode()
        guard let variant = findVariant(in: schema, name: variantName) else {
            throw EncodeError.unknownVariant(variantName)
        }
        let discriminant = getDiscriminant(in: schema, variant: variant)
        encoder.writeVarint(UInt64(discriminant))
        try encodeVariantFields(fields, variant: variant, encoder: &encoder, registry: registry)
        return
    }
    
    // Fallback: use Mirror for Swift enums
    let mirror = Mirror(reflecting: value)
    guard let displayStyle = mirror.displayStyle, displayStyle == .enum else {
        throw EncodeError.typeMismatch(expected: "enum")
    }
    
    // Extract variant name from Mirror
    let variantName = String(describing: value).components(separatedBy: "(").first ?? String(describing: value)
    guard let variant = findVariant(in: schema, name: variantName) else {
        throw EncodeError.unknownVariant(variantName)
    }
    
    let discriminant = getDiscriminant(in: schema, variant: variant)
    encoder.writeVarint(UInt64(discriminant))
    
    // Encode associated values
    let children = Array(mirror.children)
    try encodeVariantFields(children.map { $0.value }, variant: variant, encoder: &encoder, registry: registry)
}
```

### Error Type

```swift
public enum EncodeError: Error, CustomStringConvertible {
    case typeMismatch(expected: String)
    case missingField(String)
    case unknownVariant(String)
    case unknownType(String)
    
    public var description: String {
        switch self {
        case .typeMismatch(let expected):
            return "Type mismatch: expected \(expected)"
        case .missingField(let field):
            return "Missing field: \(field)"
        case .unknownVariant(let variant):
            return "Unknown variant: \(variant)"
        case .unknownType(let name):
            return "Unknown type: \(name)"
        }
    }
}
```

### Protocol for Generated Types

```swift
/// Protocol for types that can encode themselves with schema information.
///
/// Generated types implement this to provide efficient encoding without Mirror.
public protocol SchemaEncodable {
    /// Returns the variant name and field values for encoding.
    /// For structs, returns ("", [field values in order]).
    /// For enums, returns (variantName, [associated values]).
    func schemaEncode() -> (variantName: String, fields: [Any])
}
```

## Files to Create/Modify

| File | Action |
|------|--------|
| `swift/roam-runtime/Sources/RoamRuntime/SchemaEncode.swift` | CREATE |
| `swift/roam-runtime/Tests/RoamRuntimeTests/SchemaEncodeTests.swift` | CREATE |

## Implementation Steps

1. Create `PostcardEncoder` struct
2. Implement primitive encoding (bool, integers, floats, string, bytes)
3. Implement container encoding (vec, option, map)
4. Implement struct encoding via Mirror
5. Implement enum encoding via Mirror + `SchemaEncodable` protocol
6. Implement tuple encoding
7. Implement Tx/Rx encoding (just the channel ID)
8. Add comprehensive tests

## Success Criteria

1. `encodeWithSchema()` compiles and handles all schema types
2. Primitive encoding matches Rust postcard format
3. Struct encoding via Mirror works for simple structs
4. Enum encoding works for generated types with `SchemaEncodable`
5. Golden vector tests pass for encodable types

## Test Cases

```swift
func testEncodePrimitives() throws {
    XCTAssertEqual(try encodeWithSchema(true, schema: .bool), [1])
    XCTAssertEqual(try encodeWithSchema(false, schema: .bool), [0])
    XCTAssertEqual(try encodeWithSchema(UInt8(42), schema: .u8), [42])
    XCTAssertEqual(try encodeWithSchema(UInt32(300), schema: .u32), [0xAC, 0x02])
    XCTAssertEqual(try encodeWithSchema("hello", schema: .string), [5, 104, 101, 108, 108, 111])
}

func testEncodeVec() throws {
    let schema = Schema.vec(element: .u8)
    XCTAssertEqual(try encodeWithSchema([UInt8(1), UInt8(2), UInt8(3)] as [Any], schema: schema), [3, 1, 2, 3])
}

func testEncodeOption() throws {
    let schema = Schema.option(inner: .u32)
    XCTAssertEqual(try encodeWithSchema(Optional<UInt32>.none as Any, schema: schema), [0])
    XCTAssertEqual(try encodeWithSchema(Optional<UInt32>.some(42) as Any, schema: schema), [1, 42])
}
```

## Notes

- Swift's `Mirror` is read-only but sufficient for encoding (we just read values)
- For enums, Mirror's behavior is tricky — test carefully
- Generated types should implement `SchemaEncodable` for efficiency
- The `as Any` casting in tests is needed because Swift's type system
- Tx/Rx encode as just their `channelId` (u64) — the binding happens separately

## Dependencies

- Phase 002 (Schema types)

## Blocked By

- Phase 002 must be complete
