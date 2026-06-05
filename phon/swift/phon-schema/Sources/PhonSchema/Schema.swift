// The phon schema model. Mirrors `rust/phon-schema/src/schema.rs`:
// a `Schema` is an id + type parameters + a `SchemaKind`; references between
// schemas are `SchemaRef`s (a concrete id with type arguments, or a type
// variable). The same logical schema produces the same bytes and the same
// `SchemaId` in every implementation.

/// A complete schema: its content-hash id, type parameters, and structure.
// r[impl type-system.canonical-form]
// r[impl type-system.generics]
public struct Schema: Equatable, Hashable, Sendable {
    public var id: SchemaId
    public var typeParams: [String]
    public var kind: SchemaKind

    public init(id: SchemaId, typeParams: [String] = [], kind: SchemaKind) {
        self.id = id
        self.typeParams = typeParams
        self.kind = kind
    }
}

/// What a schema represents.
// r[impl type-system.canonical-form]
public indirect enum SchemaKind: Equatable, Hashable, Sendable {
    case primitive(Primitive)
    case structure(name: String, fields: [Field])
    case enumeration(name: String, variants: [Variant])
    case tuple(elements: [SchemaRef])
    case list(element: SchemaRef)
    case set(element: SchemaRef)
    case map(key: SchemaRef, value: SchemaRef)
    // r[impl type-system.array]
    case array(element: SchemaRef, dimensions: [UInt64])
    // r[impl type-system.tensor]
    case tensor(element: SchemaRef, rank: UInt32?)
    case option(element: SchemaRef)
    // r[impl type-system.channel]
    case channel(direction: ChannelDirection, element: SchemaRef)
    // r[impl type-system.dynamic]
    case dynamic
    // r[impl type-system.external]
    case external(kind: String, metadata: SchemaRef?)
}

/// The direction of a streaming channel.
public enum ChannelDirection: String, Equatable, Hashable, Sendable {
    case tx
    case rx
}

/// A reference to another schema: a concrete id (with type arguments, empty for a
/// non-generic reference) or a type variable from an enclosing schema's
/// `typeParams`.
// r[impl type-system.generics]
public indirect enum SchemaRef: Equatable, Hashable, Sendable {
    case concrete(id: SchemaId, args: [SchemaRef])
    case variable(name: String)

    public static func concrete(_ id: SchemaId) -> SchemaRef { .concrete(id: id, args: []) }
}

/// A struct (or struct-variant) field.
public struct Field: Equatable, Hashable, Sendable {
    public var name: String
    public var schema: SchemaRef
    public var required: Bool

    public init(name: String, schema: SchemaRef, required: Bool) {
        self.name = name
        self.schema = schema
        self.required = required
    }
}

/// An enum variant: a name, a stable wire index, and a payload shape.
public struct Variant: Equatable, Hashable, Sendable {
    public var name: String
    public var index: UInt32
    public var payload: VariantPayload

    public init(name: String, index: UInt32, payload: VariantPayload) {
        self.name = name
        self.index = index
        self.payload = payload
    }
}

/// The four payload shapes an enum variant can hold.
// r[impl type-system.variant-payloads]
public indirect enum VariantPayload: Equatable, Hashable, Sendable {
    case unit
    case newtype(SchemaRef)
    case tuple([SchemaRef])
    case structure([Field])
}
