# Phase 004: Schema-Driven Decode in Swift

**Status**: TODO

## Objective

Implement `decodeWithSchema()` that decodes postcard bytes into Swift values using
a schema, returning type-erased `Any` values that can be cast to concrete types.

## Background

TypeScript has `decodeWithSchema(buf, offset, schema, registry)` in `schema_codec.ts`.
This function walks the schema, reading bytes and constructing values.

Swift needs this for:
1. Decoding wire messages before generated types exist
2. Testing schema-driven decoding against golden vectors
3. Generic decoding where the concrete type isn't known at compile time

## Design

### Core Function

```swift
/// Result of decoding a value.
public struct DecodeResult<T> {
    /// The decoded value.
    public let value: T
    /// Number of bytes consumed.
    public let bytesRead: Int
}

/// Decode a value from postcard format using a schema.
///
/// - Parameters:
///   - data: Bytes to decode from
///   - offset: Starting offset in data
///   - schema: Schema describing the expected wire format
///   - registry: Registry for resolving type references
/// - Returns: Decoded value and bytes consumed
/// - Throws: `DecodeError` if decoding fails
public func decodeWithSchema(
    _ data: [UInt8],
    offset: Int = 0,
    schema: Schema,
    registry: SchemaRegistry = SchemaRegistry()
) throws -> DecodeResult<Any> {
    var decoder = PostcardDecoder(data: data, offset: offset)
    let value = try decodeValue(schema: schema, decoder: &decoder, registry: registry)
    return DecodeResult(value: value, bytesRead: decoder.offset - offset)
}
```

### PostcardDecoder

```swift
/// Low-level postcard decoder that reads from a byte buffer.
public struct PostcardDecoder {
    public let data: [UInt8]
    public private(set) var offset: Int
    
    public init(data: [UInt8], offset: Int = 0) {
        self.data = data
        self.offset = offset
    }
    
    public var remaining: Int { data.count - offset }
    
    public mutating func readByte() throws -> UInt8 {
        guard offset < data.count else {
            throw DecodeError.truncated
        }
        let byte = data[offset]
        offset += 1
        return byte
    }
    
    public mutating func readBytes(_ count: Int) throws -> [UInt8] {
        guard offset + count <= data.count else {
            throw DecodeError.truncated
        }
        let bytes = Array(data[offset..<offset + count])
        offset += count
        return bytes
    }
    
    public mutating func readVarint() throws -> UInt64 {
        var result: UInt64 = 0
        var shift: UInt64 = 0
        
        while true {
            let byte = try readByte()
            result |= UInt64(byte & 0x7F) << shift
            
            if byte & 0x80 == 0 {
                break
            }
            
            shift += 7
            if shift >= 64 {
                throw DecodeError.varintOverflow
            }
        }
        
        return result
    }
    
    public mutating func readSignedVarint() throws -> Int64 {
        let zigzag = try readVarint()
        // Zigzag decode: (zigzag >> 1) ^ -(zigzag & 1)
        return Int64(bitPattern: (zigzag >> 1) ^ (UInt64(bitPattern: -Int64(zigzag & 1))))
    }
}
```

### Decoding Logic

```swift
private func decodeValue(
    schema: Schema,
    decoder: inout PostcardDecoder,
    registry: SchemaRegistry
) throws -> Any {
    // Resolve references first
    let resolvedSchema: Schema
    if case .ref(let name) = schema {
        guard let resolved = registry.resolve(name: name) else {
            throw DecodeError.unknownType(name)
        }
        resolvedSchema = resolved
    } else {
        resolvedSchema = schema
    }
    
    switch resolvedSchema {
    // Primitives
    case .bool:
        let byte = try decoder.readByte()
        return byte != 0
        
    case .u8:
        return try decoder.readByte()
        
    case .u16:
        let v = try decoder.readVarint()
        guard v <= UInt64(UInt16.max) else { throw DecodeError.overflow("u16") }
        return UInt16(v)
        
    case .u32:
        let v = try decoder.readVarint()
        guard v <= UInt64(UInt32.max) else { throw DecodeError.overflow("u32") }
        return UInt32(v)
        
    case .u64:
        return try decoder.readVarint()
        
    case .i8:
        let byte = try decoder.readByte()
        return Int8(bitPattern: byte)
        
    case .i16:
        let v = try decoder.readSignedVarint()
        guard v >= Int64(Int16.min) && v <= Int64(Int16.max) else { 
            throw DecodeError.overflow("i16") 
        }
        return Int16(v)
        
    case .i32:
        let v = try decoder.readSignedVarint()
        guard v >= Int64(Int32.min) && v <= Int64(Int32.max) else { 
            throw DecodeError.overflow("i32") 
        }
        return Int32(v)
        
    case .i64:
        return try decoder.readSignedVarint()
        
    case .f32:
        let bytes = try decoder.readBytes(4)
        let bits = UInt32(bytes[0]) | UInt32(bytes[1]) << 8 | UInt32(bytes[2]) << 16 | UInt32(bytes[3]) << 24
        return Float(bitPattern: bits)
        
    case .f64:
        let bytes = try decoder.readBytes(8)
        var bits: UInt64 = 0
        for i in 0..<8 {
            bits |= UInt64(bytes[i]) << (i * 8)
        }
        return Double(bitPattern: bits)
        
    case .string:
        let len = try decoder.readVarint()
        guard len <= Int.max else { throw DecodeError.overflow("string length") }
        let bytes = try decoder.readBytes(Int(len))
        guard let string = String(bytes: bytes, encoding: .utf8) else {
            throw DecodeError.invalidUtf8
        }
        return string
        
    case .bytes:
        let len = try decoder.readVarint()
        guard len <= Int.max else { throw DecodeError.overflow("bytes length") }
        return try decoder.readBytes(Int(len))
        
    // Containers
    case .vec(let element):
        let count = try decoder.readVarint()
        guard count <= Int.max else { throw DecodeError.overflow("vec length") }
        var items: [Any] = []
        for _ in 0..<Int(count) {
            items.append(try decodeValue(schema: element, decoder: &decoder, registry: registry))
        }
        return items
        
    case .option(let inner):
        let tag = try decoder.readByte()
        if tag == 0 {
            return Optional<Any>.none as Any
        } else {
            let value = try decodeValue(schema: inner, decoder: &decoder, registry: registry)
            return Optional<Any>.some(value) as Any
        }
        
    case .map(let keySchema, let valueSchema):
        let count = try decoder.readVarint()
        guard count <= Int.max else { throw DecodeError.overflow("map length") }
        var dict: [AnyHashable: Any] = [:]
        for _ in 0..<Int(count) {
            let key = try decodeValue(schema: keySchema, decoder: &decoder, registry: registry)
            let value = try decodeValue(schema: valueSchema, decoder: &decoder, registry: registry)
            guard let hashableKey = key as? AnyHashable else {
                throw DecodeError.nonHashableKey
            }
            dict[hashableKey] = value
        }
        return dict
        
    // Composites
    case .struct(let structSchema):
        return try decodeStruct(schema: structSchema, decoder: &decoder, registry: registry)
        
    case .enum(let enumSchema):
        return try decodeEnum(schema: enumSchema, decoder: &decoder, registry: registry)
        
    case .tuple(let elements):
        return try decodeTuple(elements: elements, decoder: &decoder, registry: registry)
        
    // Streaming (decode as channel ID)
    case .tx(let element):
        let channelId = try decoder.readVarint()
        return DecodedTx(channelId: channelId, elementSchema: element)
        
    case .rx(let element):
        let channelId = try decoder.readVarint()
        return DecodedRx(channelId: channelId, elementSchema: element)
        
    case .ref:
        fatalError("Unreachable: ref should have been resolved")
    }
}
```

### Struct Decoding

```swift
/// Decoded struct as a dictionary of field name to value.
public struct DecodedStruct {
    public let name: String
    public let fields: [String: Any]
    
    public subscript(key: String) -> Any? {
        fields[key]
    }
}

private func decodeStruct(
    schema: StructSchema,
    decoder: inout PostcardDecoder,
    registry: SchemaRegistry
) throws -> DecodedStruct {
    var fields: [String: Any] = [:]
    
    for (fieldName, fieldSchema) in schema.fields {
        fields[fieldName] = try decodeValue(schema: fieldSchema, decoder: &decoder, registry: registry)
    }
    
    return DecodedStruct(name: schema.name, fields: fields)
}
```

### Enum Decoding

```swift
/// Decoded enum variant.
public struct DecodedEnum {
    public let enumName: String
    public let variantName: String
    public let fields: VariantFieldValues
}

/// Decoded variant field values.
public enum VariantFieldValues {
    case unit
    case newtype(Any)
    case tuple([Any])
    case `struct`([String: Any])
}

private func decodeEnum(
    schema: EnumSchema,
    decoder: inout PostcardDecoder,
    registry: SchemaRegistry
) throws -> DecodedEnum {
    let discriminant = try decoder.readVarint()
    guard discriminant <= UInt32.max else {
        throw DecodeError.overflow("discriminant")
    }
    
    guard let variant = findVariant(in: schema, discriminant: UInt32(discriminant)) else {
        throw DecodeError.unknownVariant(Int(discriminant))
    }
    
    let fields: VariantFieldValues
    switch variant.fields {
    case .unit:
        fields = .unit
        
    case .newtype(let innerSchema):
        let value = try decodeValue(schema: innerSchema, decoder: &decoder, registry: registry)
        fields = .newtype(value)
        
    case .tuple(let schemas):
        var values: [Any] = []
        for s in schemas {
            values.append(try decodeValue(schema: s, decoder: &decoder, registry: registry))
        }
        fields = .tuple(values)
        
    case .struct(let fieldSchemas):
        var fieldValues: [String: Any] = [:]
        for (name, s) in fieldSchemas {
            fieldValues[name] = try decodeValue(schema: s, decoder: &decoder, registry: registry)
        }
        fields = .struct(fieldValues)
    }
    
    return DecodedEnum(enumName: schema.name, variantName: variant.name, fields: fields)
}
```

### Tuple Decoding

```swift
private func decodeTuple(
    elements: [Schema],
    decoder: inout PostcardDecoder,
    registry: SchemaRegistry
) throws -> [Any] {
    var values: [Any] = []
    for schema in elements {
        values.append(try decodeValue(schema: schema, decoder: &decoder, registry: registry))
    }
    return values
}
```

### Decoded Channel Types

```swift
/// Placeholder for a decoded Tx channel (just the ID, needs binding).
public struct DecodedTx {
    public let channelId: UInt64
    public let elementSchema: Schema
}

/// Placeholder for a decoded Rx channel (just the ID, needs binding).
public struct DecodedRx {
    public let channelId: UInt64
    public let elementSchema: Schema
}
```

### Error Type

```swift
public enum DecodeError: Error, CustomStringConvertible {
    case truncated
    case varintOverflow
    case overflow(String)
    case invalidUtf8
    case unknownType(String)
    case unknownVariant(Int)
    case nonHashableKey
    
    public var description: String {
        switch self {
        case .truncated:
            return "Unexpected end of data"
        case .varintOverflow:
            return "Varint overflow"
        case .overflow(let type):
            return "Value overflow for \(type)"
        case .invalidUtf8:
            return "Invalid UTF-8 string"
        case .unknownType(let name):
            return "Unknown type: \(name)"
        case .unknownVariant(let discriminant):
            return "Unknown variant with discriminant \(discriminant)"
        case .nonHashableKey:
            return "Map key is not hashable"
        }
    }
}
```

## Files to Create/Modify

| File | Action |
|------|--------|
| `swift/roam-runtime/Sources/RoamRuntime/SchemaDecode.swift` | CREATE |
| `swift/roam-runtime/Tests/RoamRuntimeTests/SchemaDecodeTests.swift` | CREATE |

## Implementation Steps

1. Create `PostcardDecoder` struct
2. Implement primitive decoding
3. Implement container decoding (vec, option, map)
4. Implement struct decoding to `DecodedStruct`
5. Implement enum decoding to `DecodedEnum`
6. Implement tuple decoding
7. Implement Tx/Rx decoding to placeholder types
8. Add comprehensive tests
9. Add roundtrip tests (encode then decode)

## Success Criteria

1. `decodeWithSchema()` handles all schema types
2. Primitive decoding matches Rust postcard format
3. Struct decoding returns field dictionary
4. Enum decoding returns variant name and fields
5. Golden vector tests pass
6. Roundtrip tests pass (encode → decode → compare)

## Test Cases

```swift
func testDecodePrimitives() throws {
    XCTAssertEqual(try decodeWithSchema([1], schema: .bool).value as? Bool, true)
    XCTAssertEqual(try decodeWithSchema([0], schema: .bool).value as? Bool, false)
    XCTAssertEqual(try decodeWithSchema([42], schema: .u8).value as? UInt8, 42)
    XCTAssertEqual(try decodeWithSchema([0xAC, 0x02], schema: .u32).value as? UInt32, 300)
    XCTAssertEqual(try decodeWithSchema([5, 104, 101, 108, 108, 111], schema: .string).value as? String, "hello")
}

func testDecodeVec() throws {
    let schema = Schema.vec(element: .u8)
    let result = try decodeWithSchema([3, 1, 2, 3], schema: schema)
    let arr = result.value as! [Any]
    XCTAssertEqual(arr.count, 3)
    XCTAssertEqual(arr[0] as? UInt8, 1)
    XCTAssertEqual(arr[1] as? UInt8, 2)
    XCTAssertEqual(arr[2] as? UInt8, 3)
}

func testDecodeStruct() throws {
    let schema = Schema.struct(StructSchema(name: "Point", fields: [
        ("x", .i32),
        ("y", .i32),
    ]))
    // x = 10 (zigzag: 20), y = -5 (zigzag: 9)
    let result = try decodeWithSchema([20, 9], schema: schema)
    let decoded = result.value as! DecodedStruct
    XCTAssertEqual(decoded["x"] as? Int32, 10)
    XCTAssertEqual(decoded["y"] as? Int32, -5)
}

func testDecodeEnum() throws {
    let schema = Schema.enum(EnumSchema(name: "Message", variants: [
        EnumVariant(name: "Hello", discriminant: 0, fields: .unit),
        EnumVariant(name: "Goodbye", discriminant: 1, fields: .struct([("reason", .string)])),
    ]))
    
    // Hello variant (discriminant 0)
    let hello = try decodeWithSchema([0], schema: schema).value as! DecodedEnum
    XCTAssertEqual(hello.variantName, "Hello")
    
    // Goodbye variant (discriminant 1, reason = "bye")
    let goodbye = try decodeWithSchema([1, 3, 98, 121, 101], schema: schema).value as! DecodedEnum
    XCTAssertEqual(goodbye.variantName, "Goodbye")
    if case .struct(let fields) = goodbye.fields {
        XCTAssertEqual(fields["reason"] as? String, "bye")
    } else {
        XCTFail("Expected struct fields")
    }
}

func testRoundtrip() throws {
    let schema = Schema.struct(StructSchema(name: "Point", fields: [
        ("x", .i32),
        ("y", .i32),
    ]))
    
    struct Point { var x: Int32; var y: Int32 }
    let original = Point(x: 100, y: -50)
    
    let encoded = try encodeWithSchema(original, schema: schema)
    let decoded = try decodeWithSchema(encoded, schema: schema).value as! DecodedStruct
    
    XCTAssertEqual(decoded["x"] as? Int32, 100)
    XCTAssertEqual(decoded["y"] as? Int32, -50)
}
```

## Notes

- Decoding returns type-erased values (`Any`, `DecodedStruct`, `DecodedEnum`)
- Generated types can provide typed wrappers that cast the decoded values
- `DecodedTx`/`DecodedRx` are placeholders — real channel binding happens in phase 010
- Option decoding returns `Optional<Any>` which is tricky to work with
- Map keys must be `AnyHashable` — this limits what key types work

## Dependencies

- Phase 002 (Schema types)

## Blocked By

- Phase 002 must be complete
- Phase 003 (for roundtrip tests)
