import Foundation

// MARK: - Binding Schema

/// Schema representation used only for runtime channel binding.
public indirect enum BindingSchema: Sendable {
    case bool
    case u8, u16, u32, u64
    case i8, i16, i32, i64
    case f32, f64
    case string
    case bytes
    case tx(initialCredit: UInt32, element: BindingSchema)
    case rx(initialCredit: UInt32, element: BindingSchema)
    case vec(element: BindingSchema)
    case option(inner: BindingSchema)
    case map(key: BindingSchema, value: BindingSchema)
    case tuple(elements: [BindingSchema])
    case `struct`(fields: [(String, BindingSchema)])
    case `enum`(variants: [(String, [BindingSchema])])

    public static func tx(element: BindingSchema) -> BindingSchema {
        .tx(initialCredit: 16, element: element)
    }

    public static func rx(element: BindingSchema) -> BindingSchema {
        .rx(initialCredit: 16, element: element)
    }
}

public struct MethodBindingSchema: Sendable {
    public let args: [BindingSchema]

    public init(args: [BindingSchema]) {
        self.args = args
    }
}

public protocol BindingSerializers: Sendable {
    func txSerializer(for schema: BindingSchema) -> @Sendable (Any) -> [UInt8]
    func rxDeserializer(for schema: BindingSchema) -> @Sendable ([UInt8]) throws -> Any
}
