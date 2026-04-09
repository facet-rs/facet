import Foundation
@preconcurrency import NIOCore

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

    // MARK: - Postcard encoding (for internal use)

    func encode(into buffer: inout ByteBuffer) {
        switch self {
        case .concrete(let typeId, let args):
            encodeVarint(0, into: &buffer)
            encodeVarint(typeId, into: &buffer)
            encodeVec(args, into: &buffer, encoder: { $0.encode(into: &$1) })
        case .var(let name):
            encodeVarint(1, into: &buffer)
            encodeString(name, into: &buffer)
        }
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        let disc = try decodeVarint(from: &buffer)
        switch disc {
        case 0:
            let typeId = try decodeVarint(from: &buffer)
            let args = try decodeVec(from: &buffer, decoder: { try TypeRef.decode(from: &$0) })
            return .concrete(typeId: typeId, args: args)
        case 1:
            return .var(name: try decodeString(from: &buffer))
        default:
            throw PostcardError.unknownVariant
        }
    }

    // MARK: - CBOR encoding (for wire schema exchange)

    func encodeCbor() -> [UInt8] {
        switch self {
        case .concrete(let typeId, let args):
            // Internally tagged: {"tag": "concrete", "type_id": ..., "args": [...]}
            var out: [UInt8] = []
            out += cborEncodeMapHeader(3)
            out += cborEncodeText("tag")
            out += cborEncodeText("concrete")
            out += cborEncodeText("type_id")
            out += cborEncodeUnsigned(typeId)
            out += cborEncodeText("args")
            out += cborEncodeArrayHeader(args.count)
            for arg in args {
                out += arg.encodeCbor()
            }
            return out
        case .var(let name):
            // Internally tagged: {"tag": "var", "name": ...}
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("var")
            out += cborEncodeText("name")
            out += cborEncodeText(name)
            return out
        }
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var tag: String?
        var typeId: UInt64?
        var args: [TypeRef]?
        var name: String?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "tag":
                tag = try cborReadText(bytes, offset: &offset)
            case "type_id":
                typeId = try cborReadUnsigned(bytes, offset: &offset)
            case "args":
                let argCount = try cborReadArrayHeader(bytes, offset: &offset)
                var argList: [TypeRef] = []
                for _ in 0..<argCount {
                    argList.append(try TypeRef.decodeCbor(bytes, offset: &offset))
                }
                args = argList
            case "name":
                name = try cborReadText(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let tagValue = tag else {
            throw CborError.invalidType("missing tag in TypeRef")
        }

        switch tagValue {
        case "concrete":
            guard let tid = typeId, let a = args else {
                throw CborError.invalidType("missing type_id or args in TypeRef::Concrete")
            }
            return .concrete(typeId: tid, args: a)
        case "var":
            guard let n = name else {
                throw CborError.invalidType("missing name in TypeRef::Var")
            }
            return .var(name: n)
        default:
            throw CborError.invalidType("unknown TypeRef tag: \(tagValue)")
        }
    }

    /// Collect all type IDs referenced by this TypeRef (for dependency tracking)
    public func collectTypeIds(_ out: inout Set<SchemaHash>) {
        switch self {
        case .concrete(let typeId, let args):
            out.insert(typeId)
            for arg in args {
                arg.collectTypeIds(&out)
            }
        case .var:
            break
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

    // MARK: - Postcard encoding

    func encode(into buffer: inout ByteBuffer) {
        encodeVarint(id, into: &buffer)
        encodeVec(typeParams, into: &buffer, encoder: { encodeString($0, into: &$1) })
        kind.encode(into: &buffer)
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        let id = try decodeVarint(from: &buffer)
        let typeParams = try decodeVec(from: &buffer, decoder: { try decodeString(from: &$0) })
        let kind = try SchemaKind.decode(from: &buffer)
        return .init(id: id, typeParams: typeParams, kind: kind)
    }

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        // Schema is a struct: {"id": ..., "type_params": [...], "kind": {...}}
        var out: [UInt8] = []
        out += cborEncodeMapHeader(3)
        out += cborEncodeText("id")
        out += cborEncodeUnsigned(id)
        out += cborEncodeText("type_params")
        out += cborEncodeArrayHeader(typeParams.count)
        for tp in typeParams {
            out += cborEncodeText(tp)
        }
        out += cborEncodeText("kind")
        out += kind.encodeCbor()
        return out
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var id: UInt64?
        var typeParams: [String]?
        var kind: SchemaKind?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "id":
                id = try cborReadUnsigned(bytes, offset: &offset)
            case "type_params":
                let tpCount = try cborReadArrayHeader(bytes, offset: &offset)
                var tps: [String] = []
                for _ in 0..<tpCount {
                    tps.append(try cborReadText(bytes, offset: &offset))
                }
                typeParams = tps
            case "kind":
                kind = try SchemaKind.decodeCbor(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let i = id, let tp = typeParams, let k = kind else {
            throw CborError.invalidType("missing fields in Schema")
        }
        return .init(id: i, typeParams: tp, kind: k)
    }

    public var name: String? {
        switch kind {
        case .struct(let name, _), .enum(let name, _):
            return name
        default:
            return nil
        }
    }

    /// Collect all type IDs referenced by this schema (for dependency tracking)
    public func collectTypeIds(_ out: inout Set<SchemaHash>) {
        out.insert(id)
        kind.collectTypeIds(&out)
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

    // MARK: - Postcard encoding

    func encode(into buffer: inout ByteBuffer) {
        switch self {
        case .struct(let name, let fields):
            encodeVarint(0, into: &buffer)
            encodeString(name, into: &buffer)
            encodeVec(fields, into: &buffer, encoder: { $0.encode(into: &$1) })
        case .enum(let name, let variants):
            encodeVarint(1, into: &buffer)
            encodeString(name, into: &buffer)
            encodeVec(variants, into: &buffer, encoder: { $0.encode(into: &$1) })
        case .tuple(let elements):
            encodeVarint(2, into: &buffer)
            encodeVec(elements, into: &buffer, encoder: { $0.encode(into: &$1) })
        case .list(let element):
            encodeVarint(3, into: &buffer)
            element.encode(into: &buffer)
        case .map(let key, let value):
            encodeVarint(4, into: &buffer)
            key.encode(into: &buffer)
            value.encode(into: &buffer)
        case .array(let element, let length):
            encodeVarint(5, into: &buffer)
            element.encode(into: &buffer)
            encodeVarint(length, into: &buffer)
        case .option(let element):
            encodeVarint(6, into: &buffer)
            element.encode(into: &buffer)
        case .channel(let direction, let element):
            encodeVarint(7, into: &buffer)
            direction.encode(into: &buffer)
            element.encode(into: &buffer)
        case .primitive(let primitiveType):
            encodeVarint(8, into: &buffer)
            primitiveType.encode(into: &buffer)
        }
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        let disc = try decodeVarint(from: &buffer)
        switch disc {
        case 0:
            return .struct(
                name: try decodeString(from: &buffer),
                fields: try decodeVec(from: &buffer, decoder: { try FieldSchema.decode(from: &$0) })
            )
        case 1:
            return .enum(
                name: try decodeString(from: &buffer),
                variants: try decodeVec(
                    from: &buffer, decoder: { try VariantSchema.decode(from: &$0) })
            )
        case 2:
            return .tuple(
                elements: try decodeVec(from: &buffer, decoder: { try TypeRef.decode(from: &$0) })
            )
        case 3:
            return .list(element: try TypeRef.decode(from: &buffer))
        case 4:
            return .map(
                key: try TypeRef.decode(from: &buffer),
                value: try TypeRef.decode(from: &buffer)
            )
        case 5:
            return .array(
                element: try TypeRef.decode(from: &buffer),
                length: try decodeVarint(from: &buffer)
            )
        case 6:
            return .option(element: try TypeRef.decode(from: &buffer))
        case 7:
            return .channel(
                direction: try ChannelDirection.decode(from: &buffer),
                element: try TypeRef.decode(from: &buffer)
            )
        case 8:
            return .primitive(try PrimitiveType.decode(from: &buffer))
        default:
            throw PostcardError.unknownVariant
        }
    }

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        // Internally tagged enum: {"tag": "variant_name", ...fields}
        switch self {
        case .struct(let name, let fields):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(3)
            out += cborEncodeText("tag")
            out += cborEncodeText("struct")
            out += cborEncodeText("name")
            out += cborEncodeText(name)
            out += cborEncodeText("fields")
            out += cborEncodeArrayHeader(fields.count)
            for field in fields {
                out += field.encodeCbor()
            }
            return out
        case .enum(let name, let variants):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(3)
            out += cborEncodeText("tag")
            out += cborEncodeText("enum")
            out += cborEncodeText("name")
            out += cborEncodeText(name)
            out += cborEncodeText("variants")
            out += cborEncodeArrayHeader(variants.count)
            for variant in variants {
                out += variant.encodeCbor()
            }
            return out
        case .tuple(let elements):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("tuple")
            out += cborEncodeText("elements")
            out += cborEncodeArrayHeader(elements.count)
            for elem in elements {
                out += elem.encodeCbor()
            }
            return out
        case .list(let element):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("list")
            out += cborEncodeText("element")
            out += element.encodeCbor()
            return out
        case .map(let key, let value):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(3)
            out += cborEncodeText("tag")
            out += cborEncodeText("map")
            out += cborEncodeText("key")
            out += key.encodeCbor()
            out += cborEncodeText("value")
            out += value.encodeCbor()
            return out
        case .array(let element, let length):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(3)
            out += cborEncodeText("tag")
            out += cborEncodeText("array")
            out += cborEncodeText("element")
            out += element.encodeCbor()
            out += cborEncodeText("length")
            out += cborEncodeUnsigned(length)
            return out
        case .option(let element):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("option")
            out += cborEncodeText("element")
            out += element.encodeCbor()
            return out
        case .channel(let direction, let element):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(3)
            out += cborEncodeText("tag")
            out += cborEncodeText("channel")
            out += cborEncodeText("direction")
            out += direction.encodeCbor()
            out += cborEncodeText("element")
            out += element.encodeCbor()
            return out
        case .primitive(let primitiveType):
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("primitive")
            out += cborEncodeText("primitive_type")
            out += primitiveType.encodeCbor()
            return out
        }
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var tag: String?
        var name: String?
        var fields: [FieldSchema]?
        var variants: [VariantSchema]?
        var elements: [TypeRef]?
        var element: TypeRef?
        var key: TypeRef?
        var value: TypeRef?
        var length: UInt64?
        var direction: ChannelDirection?
        var primitiveType: PrimitiveType?

        for _ in 0..<count {
            let fieldKey = try cborReadText(bytes, offset: &offset)
            switch fieldKey {
            case "tag":
                tag = try cborReadText(bytes, offset: &offset)
            case "name":
                name = try cborReadText(bytes, offset: &offset)
            case "fields":
                let fc = try cborReadArrayHeader(bytes, offset: &offset)
                var fs: [FieldSchema] = []
                for _ in 0..<fc {
                    fs.append(try FieldSchema.decodeCbor(bytes, offset: &offset))
                }
                fields = fs
            case "variants":
                let vc = try cborReadArrayHeader(bytes, offset: &offset)
                var vs: [VariantSchema] = []
                for _ in 0..<vc {
                    vs.append(try VariantSchema.decodeCbor(bytes, offset: &offset))
                }
                variants = vs
            case "elements":
                let ec = try cborReadArrayHeader(bytes, offset: &offset)
                var es: [TypeRef] = []
                for _ in 0..<ec {
                    es.append(try TypeRef.decodeCbor(bytes, offset: &offset))
                }
                elements = es
            case "element":
                element = try TypeRef.decodeCbor(bytes, offset: &offset)
            case "key":
                key = try TypeRef.decodeCbor(bytes, offset: &offset)
            case "value":
                value = try TypeRef.decodeCbor(bytes, offset: &offset)
            case "length":
                length = try cborReadUnsigned(bytes, offset: &offset)
            case "direction":
                direction = try ChannelDirection.decodeCbor(bytes, offset: &offset)
            case "primitive_type":
                primitiveType = try PrimitiveType.decodeCbor(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let tagValue = tag else {
            throw CborError.invalidType("missing tag in SchemaKind")
        }

        switch tagValue {
        case "struct":
            guard let n = name, let f = fields else {
                throw CborError.invalidType("missing name or fields in SchemaKind::Struct")
            }
            return .struct(name: n, fields: f)
        case "enum":
            guard let n = name, let v = variants else {
                throw CborError.invalidType("missing name or variants in SchemaKind::Enum")
            }
            return .enum(name: n, variants: v)
        case "tuple":
            guard let e = elements else {
                throw CborError.invalidType("missing elements in SchemaKind::Tuple")
            }
            return .tuple(elements: e)
        case "list":
            guard let e = element else {
                throw CborError.invalidType("missing element in SchemaKind::List")
            }
            return .list(element: e)
        case "map":
            guard let k = key, let v = value else {
                throw CborError.invalidType("missing key or value in SchemaKind::Map")
            }
            return .map(key: k, value: v)
        case "array":
            guard let e = element, let l = length else {
                throw CborError.invalidType("missing element or length in SchemaKind::Array")
            }
            return .array(element: e, length: l)
        case "option":
            guard let e = element else {
                throw CborError.invalidType("missing element in SchemaKind::Option")
            }
            return .option(element: e)
        case "channel":
            guard let d = direction, let e = element else {
                throw CborError.invalidType("missing direction or element in SchemaKind::Channel")
            }
            return .channel(direction: d, element: e)
        case "primitive":
            guard let p = primitiveType else {
                throw CborError.invalidType("missing primitive_type in SchemaKind::Primitive")
            }
            return .primitive(p)
        default:
            throw CborError.invalidType("unknown SchemaKind tag: \(tagValue)")
        }
    }

    /// Collect all type IDs referenced by this schema kind
    public func collectTypeIds(_ out: inout Set<SchemaHash>) {
        switch self {
        case .struct(_, let fields):
            for field in fields {
                field.typeRef.collectTypeIds(&out)
            }
        case .enum(_, let variants):
            for variant in variants {
                variant.payload.collectTypeIds(&out)
            }
        case .tuple(let elements):
            for elem in elements {
                elem.collectTypeIds(&out)
            }
        case .list(let element), .option(let element), .array(let element, _),
            .channel(_, let element):
            element.collectTypeIds(&out)
        case .map(let key, let value):
            key.collectTypeIds(&out)
            value.collectTypeIds(&out)
        case .primitive:
            break
        }
    }
}

public enum ChannelDirection: Sendable, Equatable {
    case tx
    case rx

    func encode(into buffer: inout ByteBuffer) {
        switch self {
        case .tx:
            encodeVarint(0, into: &buffer)
        case .rx:
            encodeVarint(1, into: &buffer)
        }
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        switch try decodeVarint(from: &buffer) {
        case 0:
            return .tx
        case 1:
            return .rx
        default:
            throw PostcardError.unknownVariant
        }
    }

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        // Unit variants as simple strings
        switch self {
        case .tx:
            return cborEncodeText("tx")
        case .rx:
            return cborEncodeText("rx")
        }
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let value = try cborReadText(bytes, offset: &offset)
        switch value {
        case "tx":
            return .tx
        case "rx":
            return .rx
        default:
            throw CborError.invalidType("unknown ChannelDirection: \(value)")
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

    func encode(into buffer: inout ByteBuffer) {
        encodeString(name, into: &buffer)
        typeRef.encode(into: &buffer)
        encodeBool(required, into: &buffer)
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        .init(
            name: try decodeString(from: &buffer),
            typeRef: try TypeRef.decode(from: &buffer),
            required: try decodeBool(from: &buffer)
        )
    }

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        var out: [UInt8] = []
        out += cborEncodeMapHeader(3)
        out += cborEncodeText("name")
        out += cborEncodeText(name)
        out += cborEncodeText("type_ref")
        out += typeRef.encodeCbor()
        out += cborEncodeText("required")
        out += cborEncodeBool(required)
        return out
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var name: String?
        var typeRef: TypeRef?
        var required: Bool?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "name":
                name = try cborReadText(bytes, offset: &offset)
            case "type_ref":
                typeRef = try TypeRef.decodeCbor(bytes, offset: &offset)
            case "required":
                required = try cborReadBool(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let n = name, let tr = typeRef, let r = required else {
            throw CborError.invalidType("missing fields in FieldSchema")
        }
        return .init(name: n, typeRef: tr, required: r)
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

    func encode(into buffer: inout ByteBuffer) {
        encodeString(name, into: &buffer)
        encodeVarint(UInt64(index), into: &buffer)
        payload.encode(into: &buffer)
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        let name = try decodeString(from: &buffer)
        let rawIndex = try decodeVarint(from: &buffer)
        guard rawIndex <= UInt64(UInt32.max) else {
            throw PostcardError.overflow
        }
        return .init(
            name: name,
            index: UInt32(rawIndex),
            payload: try VariantPayload.decode(from: &buffer)
        )
    }

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        var out: [UInt8] = []
        out += cborEncodeMapHeader(3)
        out += cborEncodeText("name")
        out += cborEncodeText(name)
        out += cborEncodeText("index")
        out += cborEncodeUnsigned(UInt64(index))
        out += cborEncodeText("payload")
        out += payload.encodeCbor()
        return out
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var name: String?
        var index: UInt64?
        var payload: VariantPayload?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "name":
                name = try cborReadText(bytes, offset: &offset)
            case "index":
                index = try cborReadUnsigned(bytes, offset: &offset)
            case "payload":
                payload = try VariantPayload.decodeCbor(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let n = name, let i = index, let p = payload else {
            throw CborError.invalidType("missing fields in VariantSchema")
        }
        guard i <= UInt64(UInt32.max) else {
            throw CborError.invalidType("variant index overflow")
        }
        return .init(name: n, index: UInt32(i), payload: p)
    }
}

public indirect enum VariantPayload: Sendable, Equatable {
    case unit
    case newtype(typeRef: TypeRef)
    case tuple(types: [TypeRef])
    case `struct`(fields: [FieldSchema])

    func encode(into buffer: inout ByteBuffer) {
        switch self {
        case .unit:
            encodeVarint(0, into: &buffer)
        case .newtype(let typeRef):
            encodeVarint(1, into: &buffer)
            typeRef.encode(into: &buffer)
        case .tuple(let types):
            encodeVarint(2, into: &buffer)
            encodeVec(types, into: &buffer, encoder: { $0.encode(into: &$1) })
        case .struct(let fields):
            encodeVarint(3, into: &buffer)
            encodeVec(fields, into: &buffer, encoder: { $0.encode(into: &$1) })
        }
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        let disc = try decodeVarint(from: &buffer)
        switch disc {
        case 0:
            return .unit
        case 1:
            return .newtype(typeRef: try TypeRef.decode(from: &buffer))
        case 2:
            return .tuple(
                types: try decodeVec(from: &buffer, decoder: { try TypeRef.decode(from: &$0) })
            )
        case 3:
            return .struct(
                fields: try decodeVec(from: &buffer, decoder: { try FieldSchema.decode(from: &$0) })
            )
        default:
            throw PostcardError.unknownVariant
        }
    }

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        // facet-cbor internally-tagged enum encoding:
        // - Unit variants → just a text string: "variant_name"
        // - Struct variants → map with tag merged: { "tag": "variant_name", ...fields }
        switch self {
        case .unit:
            // Unit variant: just the tag value as a string
            return cborEncodeText("unit")
        case .newtype(let typeRef):
            // Struct variant with one field
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("newtype")
            out += cborEncodeText("type_ref")
            out += typeRef.encodeCbor()
            return out
        case .tuple(let types):
            // Struct variant with one field
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("tuple")
            out += cborEncodeText("types")
            out += cborEncodeArrayHeader(types.count)
            for t in types {
                out += t.encodeCbor()
            }
            return out
        case .struct(let fields):
            // Struct variant with one field
            var out: [UInt8] = []
            out += cborEncodeMapHeader(2)
            out += cborEncodeText("tag")
            out += cborEncodeText("struct")
            out += cborEncodeText("fields")
            out += cborEncodeArrayHeader(fields.count)
            for f in fields {
                out += f.encodeCbor()
            }
            return out
        }
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        // facet-cbor internally-tagged enum decoding:
        // - Unit variants → just a text string: "variant_name"
        // - Struct variants → map with tag: { "tag": "variant_name", ...fields }
        guard offset < bytes.count else {
            throw CborError.truncated
        }
        let majorType = bytes[offset] >> 5

        if majorType == 3 {
            // Text string - unit variant
            let tag = try cborReadText(bytes, offset: &offset)
            if tag == "unit" {
                return .unit
            }
            throw CborError.invalidType("unknown VariantPayload unit variant: \(tag)")
        }

        // Map - struct variant
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var tag: String?
        var typeRef: TypeRef?
        var types: [TypeRef]?
        var fields: [FieldSchema]?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "tag":
                tag = try cborReadText(bytes, offset: &offset)
            case "type_ref":
                typeRef = try TypeRef.decodeCbor(bytes, offset: &offset)
            case "types":
                let tc = try cborReadArrayHeader(bytes, offset: &offset)
                var ts: [TypeRef] = []
                for _ in 0..<tc {
                    ts.append(try TypeRef.decodeCbor(bytes, offset: &offset))
                }
                types = ts
            case "fields":
                let fc = try cborReadArrayHeader(bytes, offset: &offset)
                var fs: [FieldSchema] = []
                for _ in 0..<fc {
                    fs.append(try FieldSchema.decodeCbor(bytes, offset: &offset))
                }
                fields = fs
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let tagValue = tag else {
            throw CborError.invalidType("missing tag in VariantPayload")
        }

        switch tagValue {
        case "unit":
            return .unit
        case "newtype":
            guard let tr = typeRef else {
                throw CborError.invalidType("missing type_ref in VariantPayload::Newtype")
            }
            return .newtype(typeRef: tr)
        case "tuple":
            guard let ts = types else {
                throw CborError.invalidType("missing types in VariantPayload::Tuple")
            }
            return .tuple(types: ts)
        case "struct":
            guard let fs = fields else {
                throw CborError.invalidType("missing fields in VariantPayload::Struct")
            }
            return .struct(fields: fs)
        default:
            throw CborError.invalidType("unknown VariantPayload tag: \(tagValue)")
        }
    }

    /// Collect all type IDs referenced by this payload
    public func collectTypeIds(_ out: inout Set<SchemaHash>) {
        switch self {
        case .unit:
            break
        case .newtype(let typeRef):
            typeRef.collectTypeIds(&out)
        case .tuple(let types):
            for t in types {
                t.collectTypeIds(&out)
            }
        case .struct(let fields):
            for f in fields {
                f.typeRef.collectTypeIds(&out)
            }
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

    func encode(into buffer: inout ByteBuffer) {
        let disc: UInt64 =
            switch self {
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
        encodeVarint(disc, into: &buffer)
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        switch try decodeVarint(from: &buffer) {
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

    // MARK: - CBOR encoding

    func encodeCbor() -> [UInt8] {
        // Unit variants as simple strings
        let name: String =
            switch self {
            case .bool: "bool"
            case .u8: "u8"
            case .u16: "u16"
            case .u32: "u32"
            case .u64: "u64"
            case .u128: "u128"
            case .i8: "i8"
            case .i16: "i16"
            case .i32: "i32"
            case .i64: "i64"
            case .i128: "i128"
            case .f32: "f32"
            case .f64: "f64"
            case .char: "char"
            case .string: "string"
            case .unit: "unit"
            case .never: "never"
            case .bytes: "bytes"
            case .payload: "payload"
            }
        return cborEncodeText(name)
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let value = try cborReadText(bytes, offset: &offset)
        switch value {
        case "bool": return .bool
        case "u8": return .u8
        case "u16": return .u16
        case "u32": return .u32
        case "u64": return .u64
        case "u128": return .u128
        case "i8": return .i8
        case "i16": return .i16
        case "i32": return .i32
        case "i64": return .i64
        case "i128": return .i128
        case "f32": return .f32
        case "f64": return .f64
        case "char": return .char
        case "string": return .string
        case "unit": return .unit
        case "never": return .never
        case "bytes": return .bytes
        case "payload": return .payload
        default:
            throw CborError.invalidType("unknown PrimitiveType: \(value)")
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

    // MARK: - CBOR encoding

    public func encodeCbor() -> [UInt8] {
        var out: [UInt8] = []
        out += cborEncodeMapHeader(2)
        out += cborEncodeText("schemas")
        out += cborEncodeArrayHeader(schemas.count)
        for schema in schemas {
            out += schema.encodeCbor()
        }
        out += cborEncodeText("root")
        out += root.encodeCbor()
        return out
    }

    public static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var schemas: [Schema]?
        var root: TypeRef?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "schemas":
                let sc = try cborReadArrayHeader(bytes, offset: &offset)
                var ss: [Schema] = []
                for _ in 0..<sc {
                    ss.append(try Schema.decodeCbor(bytes, offset: &offset))
                }
                schemas = ss
            case "root":
                root = try TypeRef.decodeCbor(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        guard let s = schemas, let r = root else {
            throw CborError.invalidType("missing fields in SchemaPayload")
        }
        return .init(schemas: s, root: r)
    }

    /// Collect all schema IDs in this payload
    public func collectSchemaIds() -> Set<SchemaHash> {
        var ids: Set<SchemaHash> = []
        for schema in schemas {
            ids.insert(schema.id)
        }
        return ids
    }
}

public enum BindingDirection: Sendable, Equatable {
    case args
    case response

    func encode(into buffer: inout ByteBuffer) {
        switch self {
        case .args:
            encodeVarint(0, into: &buffer)
        case .response:
            encodeVarint(1, into: &buffer)
        }
    }

    static func decode(from buffer: inout ByteBuffer) throws -> Self {
        let disc = try decodeVarint(from: &buffer)
        switch disc {
        case 0:
            return .args
        case 1:
            return .response
        default:
            throw WireError.unknownVariant(disc)
        }
    }
}

/// Pre-computed CBOR schema payloads for a method's args and response.
/// Generated by vox-codegen and used for wire protocol schema exchange.
/// @deprecated Use MethodSchemaInfo instead for proper per-schema tracking.
public struct MethodWireSchemas: Sendable {
    /// CBOR-encoded schema payload for method arguments
    public let argsSchemas: [UInt8]
    /// CBOR-encoded schema payload for method response
    public let responseSchemas: [UInt8]

    public init(argsSchemas: [UInt8], responseSchemas: [UInt8]) {
        self.argsSchemas = argsSchemas
        self.responseSchemas = responseSchemas
    }
}

/// Per-method schema information for wire protocol schema exchange.
/// Contains schema IDs and root TypeRefs - actual Schema objects are in the global registry.
public struct MethodSchemaInfo: Sendable {
    /// Schema IDs needed for method arguments
    public let argsSchemaIds: [SchemaHash]
    /// Root TypeRef for method arguments
    public let argsRoot: TypeRef
    /// Schema IDs needed for method response
    public let responseSchemaIds: [SchemaHash]
    /// Root TypeRef for method response
    public let responseRoot: TypeRef

    public init(
        argsSchemaIds: [SchemaHash],
        argsRoot: TypeRef,
        responseSchemaIds: [SchemaHash],
        responseRoot: TypeRef
    ) {
        self.argsSchemaIds = argsSchemaIds
        self.argsRoot = argsRoot
        self.responseSchemaIds = responseSchemaIds
        self.responseRoot = responseRoot
    }

    /// Build a SchemaPayload for sending, looking up schemas from the registry.
    public func buildPayload(
        direction: BindingDirection,
        registry: [SchemaHash: Schema]
    ) -> SchemaPayload {
        let schemaIds = direction == .args ? argsSchemaIds : responseSchemaIds
        let root = direction == .args ? argsRoot : responseRoot

        var schemas: [Schema] = []
        for id in schemaIds {
            if let schema = registry[id] {
                schemas.append(schema)
            }
        }

        return SchemaPayload(schemas: schemas, root: root)
    }
}

/// Protocol error when schema exchange violates protocol rules.
public enum SchemaProtocolError: Error, Equatable {
    /// Peer sent a schema ID that was already sent on this connection
    case duplicateSchema(SchemaHash)
    /// Root TypeRef references a schema ID that was never sent
    case unreferencedTypeId(SchemaHash)
    /// Expected root TypeRef does not match received root
    case rootMismatch(expected: String, received: String)
}

/// Tracks which schema IDs have been sent on a connection.
/// Per-connection, must be reset on resume.
public final class SchemaSendTracker: @unchecked Sendable {
    private var sentSchemaIds: Set<SchemaHash> = []
    private let lock = NSLock()

    public init() {}

    /// Filter a SchemaPayload to only include schemas not yet sent.
    /// Returns a new payload with only unsent schemas (root is always included).
    /// Marks the included schemas as sent.
    ///
    /// When `methodId` is provided, this tracker keeps a fast path for methods that
    /// have already sent all their schemas on this connection.
    public func filterForSending(_ payload: SchemaPayload, methodId: UInt64? = nil) -> SchemaPayload
    {
        lock.lock()
        defer { lock.unlock() }

        var unsent: [Schema] = []
        for schema in payload.schemas {
            if !sentSchemaIds.contains(schema.id) {
                sentSchemaIds.insert(schema.id)
                unsent.append(schema)
            }
        }

        return SchemaPayload(schemas: unsent, root: payload.root)
    }

    /// Check if a schema ID has been sent.
    public func hasSent(_ schemaId: SchemaHash) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return sentSchemaIds.contains(schemaId)
    }

    /// Reset tracking state (must be called on resume).
    public func reset() {
        lock.lock()
        defer { lock.unlock() }
        sentSchemaIds.removeAll()
    }

    // MARK: - Legacy API (for generated code compatibility)

    /// Legacy method for compatibility with generated code.
    /// Uses pre-computed CBOR blobs - will be replaced with proper per-schema tracking.
    @available(*, deprecated, message: "Use filterForSending with SchemaPayload instead")
    public func prepareSchemas(
        methodId: UInt64,
        direction: BindingDirection,
        wireSchemas: [UInt64: MethodWireSchemas]
    ) -> [UInt8] {
        // For now, use method+direction as key since generated code still uses pre-computed blobs
        let key = methodId &* 2 + (direction == .args ? 0 : 1)

        lock.lock()
        defer { lock.unlock() }

        if sentSchemaIds.contains(key) {
            return []
        }
        sentSchemaIds.insert(key)

        guard let methodSchemas = wireSchemas[methodId] else {
            return []
        }

        switch direction {
        case .args:
            return methodSchemas.argsSchemas
        case .response:
            return methodSchemas.responseSchemas
        }
    }
}

/// Tracks which schema IDs have been received on a connection.
/// Per-connection, must be reset on resume.
public final class SchemaReceiveTracker: @unchecked Sendable {
    private var receivedSchemaIds: Set<SchemaHash> = []
    private var receivedSchemas: [SchemaHash: Schema] = [:]
    private let lock = NSLock()

    public init() {}

    /// Process a received SchemaPayload.
    /// - Throws: SchemaProtocolError.duplicateSchema if any schema ID was already received
    /// - Throws: SchemaProtocolError.unreferencedTypeId if root references unknown type IDs
    public func receive(_ payload: SchemaPayload) throws {
        lock.lock()
        defer { lock.unlock() }

        // Check for duplicates
        for schema in payload.schemas {
            if receivedSchemaIds.contains(schema.id) {
                throw SchemaProtocolError.duplicateSchema(schema.id)
            }
        }

        // Register all schemas
        for schema in payload.schemas {
            receivedSchemaIds.insert(schema.id)
            receivedSchemas[schema.id] = schema
        }

        // Verify all type IDs referenced in root are known
        var referencedIds: Set<SchemaHash> = []
        payload.root.collectTypeIds(&referencedIds)
        for typeId in referencedIds {
            if !receivedSchemaIds.contains(typeId) {
                throw SchemaProtocolError.unreferencedTypeId(typeId)
            }
        }
    }

    /// Verify that a received root matches the expected root.
    /// Returns nil if they match, or a descriptive error if they don't.
    public func verifyRoot(expected: TypeRef, received: TypeRef) -> SchemaProtocolError? {
        if expected == received {
            return nil
        }
        return .rootMismatch(
            expected: describeTypeRef(expected),
            received: describeTypeRef(received)
        )
    }

    /// Check if we have a schema for the given type ID.
    public func hasSchema(_ typeId: SchemaHash) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return receivedSchemaIds.contains(typeId)
    }

    /// Get a received schema by ID.
    public func getSchema(_ typeId: SchemaHash) -> Schema? {
        lock.lock()
        defer { lock.unlock() }
        return receivedSchemas[typeId]
    }

    /// Reset tracking state (must be called on resume).
    public func reset() {
        lock.lock()
        defer { lock.unlock() }
        receivedSchemaIds.removeAll()
        receivedSchemas.removeAll()
    }

    /// Describe a TypeRef for error messages.
    private func describeTypeRef(_ ref: TypeRef) -> String {
        lock.lock()
        defer { lock.unlock() }
        return describeTypeRefLocked(ref)
    }

    private func describeTypeRefLocked(_ ref: TypeRef) -> String {
        switch ref {
        case .var(let name):
            return "var(\(name))"
        case .concrete(let typeId, let args):
            let name = receivedSchemas[typeId]?.name ?? "0x\(String(typeId, radix: 16))"
            if args.isEmpty {
                return name
            }
            let argDescs = args.map { describeTypeRefLocked($0) }
            return "\(name)<\(argDescs.joined(separator: ", "))>"
        }
    }
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
        return .concrete(
            typeId: typeId, args: args.map { substituteTypeRef($0, substitutions: substitutions) })
    }
}

private func substituteTypeRefs(in kind: SchemaKind, substitutions: [TypeParamName: TypeRef])
    -> SchemaKind
{
    let sub = { substituteTypeRef($0, substitutions: substitutions) }

    switch kind {
    case .struct(let name, let fields):
        return .struct(
            name: name,
            fields: fields.map {
                FieldSchema(name: $0.name, typeRef: sub($0.typeRef), required: $0.required)
            }
        )
    case .enum(let name, let variants):
        return .enum(
            name: name,
            variants: variants.map {
                VariantSchema(
                    name: $0.name, index: $0.index,
                    payload: substituteVariantPayload($0.payload, substitutions: substitutions))
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
            fields: fields.map {
                FieldSchema(name: $0.name, typeRef: sub($0.typeRef), required: $0.required)
            }
        )
    }
}
