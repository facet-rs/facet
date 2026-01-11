import Foundation

// MARK: - Schema

/// Schema representation for runtime channel binding.
public indirect enum Schema: Sendable {
    case bool
    case u8, u16, u32, u64
    case i8, i16, i32, i64
    case f32, f64
    case string
    case bytes
    case tx(element: Schema)
    case rx(element: Schema)
    case vec(element: Schema)
    case option(inner: Schema)
    case map(key: Schema, value: Schema)
    case tuple(elements: [Schema])
    case `struct`(fields: [(String, Schema)])
    case `enum`(variants: [(String, [Schema])])
}

// MARK: - Method Schema

/// Schema for a method's arguments.
public struct MethodSchema: Sendable {
    public let args: [Schema]

    public init(args: [Schema]) {
        self.args = args
    }
}

// MARK: - Binding Serializers Protocol

/// Protocol for type-specific serializers used during channel binding.
public protocol BindingSerializers: Sendable {
    func txSerializer(for schema: Schema) -> @Sendable (Any) -> [UInt8]
    func rxDeserializer(for schema: Schema) -> @Sendable ([UInt8]) throws -> Any
}
