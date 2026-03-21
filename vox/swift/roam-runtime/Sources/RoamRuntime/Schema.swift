import Foundation

public typealias SchemaHash = UInt64
public typealias TypeParamName = String
public typealias SchemaRegistry = [SchemaHash: Schema]

public indirect enum TypeRef: Sendable, Equatable {
    case concrete(typeId: SchemaHash, args: [TypeRef])
    case `var`(name: TypeParamName)

    public static func concrete(_ typeId: SchemaHash) -> Self {
        .concrete(typeId: typeId, args: [])
    }

    public static func generic(_ typeId: SchemaHash, args: [TypeRef]) -> Self {
        .concrete(typeId: typeId, args: args)
    }

    func encode() -> [UInt8] {
        switch self {
        case .concrete(let typeId, let args):
            return encodeVarint(0) + encodeVarint(typeId) + encodeVec(args, encoder: { $0.encode() })
        case .var(let name):
            return encodeVarint(1) + encodeString(name)
        }
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        let disc = try decodeVarint(from: data, offset: &offset)
        switch disc {
        case 0:
            let typeId = try decodeVarint(from: data, offset: &offset)
            let args = try decodeVec(
                from: data,
                offset: &offset,
                decoder: { data, off in try TypeRef.decode(from: data, offset: &off) }
            )
            return .concrete(typeId: typeId, args: args)
        case 1:
            return .var(name: try decodeString(from: data, offset: &offset))
        default:
            throw PostcardError.unknownVariant
        }
    }
}

public struct Schema: Sendable, Equatable {
    public var id: SchemaHash
    public var typeParams: [TypeParamName]
    public var kind: SchemaKind

    public init(id: SchemaHash, typeParams: [TypeParamName], kind: SchemaKind) {
        self.id = id
        self.typeParams = typeParams
        self.kind = kind
    }

    func encode() -> [UInt8] {
        encodeVarint(id)
            + encodeVec(typeParams, encoder: encodeString)
            + kind.encode()
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        let id = try decodeVarint(from: data, offset: &offset)
        let typeParams = try decodeVec(from: data, offset: &offset, decoder: decodeString)
        let kind = try SchemaKind.decode(from: data, offset: &offset)
        return .init(id: id, typeParams: typeParams, kind: kind)
    }

    public var name: String? {
        switch kind {
        case .struct(let name, _), .enum(let name, _):
            return name
        default:
            return nil
        }
    }
}

public indirect enum SchemaKind: Sendable, Equatable {
    case `struct`(name: String, fields: [FieldSchema])
    case `enum`(name: String, variants: [VariantSchema])
    case tuple(elements: [TypeRef])
    case list(element: TypeRef)
    case map(key: TypeRef, value: TypeRef)
    case array(element: TypeRef, length: UInt64)
    case option(element: TypeRef)
    case channel(direction: ChannelDirection, element: TypeRef)
    case primitive(PrimitiveType)

    func encode() -> [UInt8] {
        switch self {
        case .struct(let name, let fields):
            return encodeVarint(0) + encodeString(name) + encodeVec(fields, encoder: { $0.encode() })
        case .enum(let name, let variants):
            return encodeVarint(1) + encodeString(name) + encodeVec(variants, encoder: { $0.encode() })
        case .tuple(let elements):
            return encodeVarint(2) + encodeVec(elements, encoder: { $0.encode() })
        case .list(let element):
            return encodeVarint(3) + element.encode()
        case .map(let key, let value):
            return encodeVarint(4) + key.encode() + value.encode()
        case .array(let element, let length):
            return encodeVarint(5) + element.encode() + encodeVarint(length)
        case .option(let element):
            return encodeVarint(6) + element.encode()
        case .channel(let direction, let element):
            return encodeVarint(7) + direction.encode() + element.encode()
        case .primitive(let primitiveType):
            return encodeVarint(8) + primitiveType.encode()
        }
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        let disc = try decodeVarint(from: data, offset: &offset)
        switch disc {
        case 0:
            return .struct(
                name: try decodeString(from: data, offset: &offset),
                fields: try decodeVec(
                    from: data,
                    offset: &offset,
                    decoder: { data, off in try FieldSchema.decode(from: data, offset: &off) }
                )
            )
        case 1:
            return .enum(
                name: try decodeString(from: data, offset: &offset),
                variants: try decodeVec(
                    from: data,
                    offset: &offset,
                    decoder: { data, off in try VariantSchema.decode(from: data, offset: &off) }
                )
            )
        case 2:
            return .tuple(
                elements: try decodeVec(
                    from: data,
                    offset: &offset,
                    decoder: { data, off in try TypeRef.decode(from: data, offset: &off) }
                )
            )
        case 3:
            return .list(element: try TypeRef.decode(from: data, offset: &offset))
        case 4:
            return .map(
                key: try TypeRef.decode(from: data, offset: &offset),
                value: try TypeRef.decode(from: data, offset: &offset)
            )
        case 5:
            return .array(
                element: try TypeRef.decode(from: data, offset: &offset),
                length: try decodeVarint(from: data, offset: &offset)
            )
        case 6:
            return .option(element: try TypeRef.decode(from: data, offset: &offset))
        case 7:
            return .channel(
                direction: try ChannelDirection.decode(from: data, offset: &offset),
                element: try TypeRef.decode(from: data, offset: &offset)
            )
        case 8:
            return .primitive(try PrimitiveType.decode(from: data, offset: &offset))
        default:
            throw PostcardError.unknownVariant
        }
    }
}

public enum ChannelDirection: Sendable, Equatable {
    case tx
    case rx

    func encode() -> [UInt8] {
        switch self {
        case .tx:
            return encodeVarint(0)
        case .rx:
            return encodeVarint(1)
        }
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        switch try decodeVarint(from: data, offset: &offset) {
        case 0:
            return .tx
        case 1:
            return .rx
        default:
            throw PostcardError.unknownVariant
        }
    }
}

public struct FieldSchema: Sendable, Equatable {
    public var name: String
    public var typeRef: TypeRef
    public var required: Bool

    public init(name: String, typeRef: TypeRef, required: Bool) {
        self.name = name
        self.typeRef = typeRef
        self.required = required
    }

    func encode() -> [UInt8] {
        encodeString(name) + typeRef.encode() + encodeBool(required)
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        .init(
            name: try decodeString(from: data, offset: &offset),
            typeRef: try TypeRef.decode(from: data, offset: &offset),
            required: try decodeBool(from: data, offset: &offset)
        )
    }
}

public struct VariantSchema: Sendable, Equatable {
    public var name: String
    public var index: UInt32
    public var payload: VariantPayload

    public init(name: String, index: UInt32, payload: VariantPayload) {
        self.name = name
        self.index = index
        self.payload = payload
    }

    func encode() -> [UInt8] {
        encodeString(name) + encodeVarint(UInt64(index)) + payload.encode()
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        let name = try decodeString(from: data, offset: &offset)
        let rawIndex = try decodeVarint(from: data, offset: &offset)
        guard rawIndex <= UInt64(UInt32.max) else {
            throw PostcardError.overflow
        }
        return .init(
            name: name,
            index: UInt32(rawIndex),
            payload: try VariantPayload.decode(from: data, offset: &offset)
        )
    }
}

public indirect enum VariantPayload: Sendable, Equatable {
    case unit
    case newtype(typeRef: TypeRef)
    case tuple(types: [TypeRef])
    case `struct`(fields: [FieldSchema])

    func encode() -> [UInt8] {
        switch self {
        case .unit:
            return encodeVarint(0)
        case .newtype(let typeRef):
            return encodeVarint(1) + typeRef.encode()
        case .tuple(let types):
            return encodeVarint(2) + encodeVec(types, encoder: { $0.encode() })
        case .struct(let fields):
            return encodeVarint(3) + encodeVec(fields, encoder: { $0.encode() })
        }
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        let disc = try decodeVarint(from: data, offset: &offset)
        switch disc {
        case 0:
            return .unit
        case 1:
            return .newtype(typeRef: try TypeRef.decode(from: data, offset: &offset))
        case 2:
            return .tuple(
                types: try decodeVec(
                    from: data,
                    offset: &offset,
                    decoder: { data, off in try TypeRef.decode(from: data, offset: &off) }
                )
            )
        case 3:
            return .struct(
                fields: try decodeVec(
                    from: data,
                    offset: &offset,
                    decoder: { data, off in try FieldSchema.decode(from: data, offset: &off) }
                )
            )
        default:
            throw PostcardError.unknownVariant
        }
    }
}

public enum PrimitiveType: Sendable, Equatable {
    case bool
    case u8
    case u16
    case u32
    case u64
    case u128
    case i8
    case i16
    case i32
    case i64
    case i128
    case f32
    case f64
    case char
    case string
    case unit
    case never
    case bytes
    case payload

    func encode() -> [UInt8] {
        let disc: UInt64 = switch self {
        case .bool: 0
        case .u8: 1
        case .u16: 2
        case .u32: 3
        case .u64: 4
        case .u128: 5
        case .i8: 6
        case .i16: 7
        case .i32: 8
        case .i64: 9
        case .i128: 10
        case .f32: 11
        case .f64: 12
        case .char: 13
        case .string: 14
        case .unit: 15
        case .never: 16
        case .bytes: 17
        case .payload: 18
        }
        return encodeVarint(disc)
    }

    static func decode(from data: Data, offset: inout Int) throws -> Self {
        switch try decodeVarint(from: data, offset: &offset) {
        case 0: return .bool
        case 1: return .u8
        case 2: return .u16
        case 3: return .u32
        case 4: return .u64
        case 5: return .u128
        case 6: return .i8
        case 7: return .i16
        case 8: return .i32
        case 9: return .i64
        case 10: return .i128
        case 11: return .f32
        case 12: return .f64
        case 13: return .char
        case 14: return .string
        case 15: return .unit
        case 16: return .never
        case 17: return .bytes
        case 18: return .payload
        default:
            throw PostcardError.unknownVariant
        }
    }
}

public struct SchemaPayload: Sendable, Equatable {
    public var schemas: [Schema]
    public var root: TypeRef

    public init(schemas: [Schema], root: TypeRef) {
        self.schemas = schemas
        self.root = root
    }
}

public enum BindingDirection: Sendable, Equatable {
    case args
    case response
}

public struct SchemaSet: Sendable, Equatable {
    public var root: Schema
    public var registry: SchemaRegistry

    public init(root: Schema, registry: SchemaRegistry) {
        self.root = root
        self.registry = registry
    }
}

public func buildSchemaRegistry(_ schemas: [Schema]) -> SchemaRegistry {
    var registry: SchemaRegistry = [:]
    for schema in schemas {
        registry[schema.id] = schema
    }
    return registry
}

public func resolveTypeRef(_ ref: TypeRef, in registry: SchemaRegistry) -> SchemaKind? {
    switch ref {
    case .var:
        return nil
    case .concrete(let typeId, let args):
        guard let schema = registry[typeId] else {
            return nil
        }
        guard !args.isEmpty else {
            return schema.kind
        }
        var substitutions: [TypeParamName: TypeRef] = [:]
        for (index, typeParam) in schema.typeParams.enumerated() where index < args.count {
            substitutions[typeParam] = args[index]
        }
        return substituteTypeRefs(in: schema.kind, substitutions: substitutions)
    }
}

private func substituteTypeRef(_ ref: TypeRef, substitutions: [TypeParamName: TypeRef]) -> TypeRef {
    switch ref {
    case .var(let name):
        return substitutions[name] ?? ref
    case .concrete(let typeId, let args):
        return .concrete(typeId: typeId, args: args.map { substituteTypeRef($0, substitutions: substitutions) })
    }
}

private func substituteTypeRefs(in kind: SchemaKind, substitutions: [TypeParamName: TypeRef]) -> SchemaKind {
    let sub = { substituteTypeRef($0, substitutions: substitutions) }

    switch kind {
    case .struct(let name, let fields):
        return .struct(
            name: name,
            fields: fields.map { FieldSchema(name: $0.name, typeRef: sub($0.typeRef), required: $0.required) }
        )
    case .enum(let name, let variants):
        return .enum(
            name: name,
            variants: variants.map {
                VariantSchema(name: $0.name, index: $0.index, payload: substituteVariantPayload($0.payload, substitutions: substitutions))
            }
        )
    case .tuple(let elements):
        return .tuple(elements: elements.map(sub))
    case .list(let element):
        return .list(element: sub(element))
    case .map(let key, let value):
        return .map(key: sub(key), value: sub(value))
    case .array(let element, let length):
        return .array(element: sub(element), length: length)
    case .option(let element):
        return .option(element: sub(element))
    case .channel(let direction, let element):
        return .channel(direction: direction, element: sub(element))
    case .primitive:
        return kind
    }
}

private func substituteVariantPayload(
    _ payload: VariantPayload,
    substitutions: [TypeParamName: TypeRef]
) -> VariantPayload {
    let sub = { substituteTypeRef($0, substitutions: substitutions) }

    switch payload {
    case .unit:
        return .unit
    case .newtype(let typeRef):
        return .newtype(typeRef: sub(typeRef))
    case .tuple(let types):
        return .tuple(types: types.map(sub))
    case .struct(let fields):
        return .struct(
            fields: fields.map { FieldSchema(name: $0.name, typeRef: sub($0.typeRef), required: $0.required) }
        )
    }
}
