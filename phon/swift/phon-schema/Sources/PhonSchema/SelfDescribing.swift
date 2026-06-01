// The self-describing (tag-led) schema codec.
//
// A `Schema` is encoded as an ordinary phon value: it rides the one mode that
// needs nothing agreed in advance, which is how two peers bootstrap schema
// exchange. The encoding is a hand-written, full-fidelity walk of the typed
// `Schema`, using the rich tag table directly.
//
// Decoding is the first untrusted-input path: every length, tag, depth, and
// UTF-8 check runs here, via `Reader`.
//
// Mirrors `rust/phon-schema/src/selfdescribing.rs` byte-for-byte (the schema
// half — the coarse `Value` codec lives in `Value.swift`).

// MARK: - Public API

/// Encode a schema to self-describing bytes.
public func schemaToBytes(_ schema: Schema) -> [UInt8] {
    var out = ByteSink()
    encSchema(&out, schema)
    return out.bytes
}

/// Decode a schema from self-describing bytes, rejecting trailing bytes.
public func schemaFromBytes(_ buf: [UInt8]) throws -> Schema {
    var r = Reader(buf)
    let schema = try decSchema(&r, 0)
    if r.remaining != 0 {
        throw DecodeError.trailingBytes(r.remaining)
    }
    return schema
}

// MARK: - Encoding — scalar/value helpers

private func vU32<S: Sink>(_ out: inout S, _ n: UInt32) {
    out.writeU8(Tag.u32)
    out.writeU32(n)
}

private func vU64<S: Sink>(_ out: inout S, _ n: UInt64) {
    out.writeU8(Tag.u64)
    out.writeU64(n)
}

private func vBool<S: Sink>(_ out: inout S, _ b: Bool) {
    out.writeU8(Tag.bool)
    out.writeBool(b)
}

private func vStr<S: Sink>(_ out: inout S, _ s: String) {
    out.writeU8(Tag.string)
    out.writeStr(s)
}

private func vUnit<S: Sink>(_ out: inout S) {
    out.writeU8(Tag.unit)
}

/// Begin a struct value: the tag, the struct name, and the field count.
private func st<S: Sink>(_ out: inout S, _ name: String, _ fields: UInt32) {
    out.writeU8(Tag.structure)
    out.writeStr(name)
    out.writeU32(fields)
}

/// Begin a list value of `len` elements.
private func listBegin<S: Sink>(_ out: inout S, _ len: Int) {
    out.writeU8(Tag.list)
    out.writeU32(UInt32(len))
}

// MARK: - Encoding — schema

private func encSchema<S: Sink>(_ out: inout S, _ s: Schema) {
    st(&out, "Schema", 3)
    out.writeStr("id")
    vU64(&out, s.id.raw)
    out.writeStr("type_params")
    listBegin(&out, s.typeParams.count)
    for p in s.typeParams {
        vStr(&out, p)
    }
    out.writeStr("kind")
    encKind(&out, s.kind)
}

private func encKind<S: Sink>(_ out: inout S, _ k: SchemaKind) {
    out.writeU8(Tag.enumeration)
    switch k {
    case .primitive(let p):
        out.writeStr("Primitive")
        encPrimitive(&out, p)
    case .structure(let name, let fields):
        out.writeStr("Struct")
        st(&out, "Struct", 2)
        out.writeStr("name")
        vStr(&out, name)
        out.writeStr("fields")
        encFieldList(&out, fields)
    case .enumeration(let name, let variants):
        out.writeStr("Enum")
        st(&out, "Enum", 2)
        out.writeStr("name")
        vStr(&out, name)
        out.writeStr("variants")
        listBegin(&out, variants.count)
        for v in variants {
            encVariant(&out, v)
        }
    case .tuple(let elements):
        out.writeStr("Tuple")
        st(&out, "Tuple", 1)
        out.writeStr("elements")
        encRefList(&out, elements)
    case .list(let element):
        out.writeStr("List")
        st(&out, "List", 1)
        out.writeStr("element")
        encRef(&out, element)
    case .set(let element):
        out.writeStr("Set")
        st(&out, "Set", 1)
        out.writeStr("element")
        encRef(&out, element)
    case .option(let element):
        out.writeStr("Option")
        st(&out, "Option", 1)
        out.writeStr("element")
        encRef(&out, element)
    case .map(let key, let value):
        out.writeStr("Map")
        st(&out, "Map", 2)
        out.writeStr("key")
        encRef(&out, key)
        out.writeStr("value")
        encRef(&out, value)
    case .array(let element, let dimensions):
        out.writeStr("Array")
        st(&out, "Array", 2)
        out.writeStr("element")
        encRef(&out, element)
        out.writeStr("dimensions")
        listBegin(&out, dimensions.count)
        for d in dimensions {
            vU64(&out, d)
        }
    case .tensor(let element, let rank):
        out.writeStr("Tensor")
        st(&out, "Tensor", 2)
        out.writeStr("element")
        encRef(&out, element)
        out.writeStr("rank")
        switch rank {
        case .none:
            out.writeU8(Tag.optionNone)
        case .some(let r):
            out.writeU8(Tag.optionSome)
            vU32(&out, r)
        }
    case .channel(let direction, let element):
        out.writeStr("Channel")
        st(&out, "Channel", 2)
        out.writeStr("direction")
        encDirection(&out, direction)
        out.writeStr("element")
        encRef(&out, element)
    case .dynamic:
        out.writeStr("Dynamic")
        vUnit(&out)
    case .external(let kind, let metadata):
        out.writeStr("External")
        st(&out, "External", 2)
        out.writeStr("kind")
        vStr(&out, kind)
        out.writeStr("metadata")
        switch metadata {
        case .none:
            out.writeU8(Tag.optionNone)
        case .some(let r):
            out.writeU8(Tag.optionSome)
            encRef(&out, r)
        }
    }
}

private func encPrimitive<S: Sink>(_ out: inout S, _ p: Primitive) {
    out.writeU8(Tag.enumeration)
    out.writeStr(p.tag)
    vUnit(&out)
}

private func encDirection<S: Sink>(_ out: inout S, _ d: ChannelDirection) {
    out.writeU8(Tag.enumeration)
    out.writeStr(d.rawValue)
    vUnit(&out)
}

private func encRef<S: Sink>(_ out: inout S, _ r: SchemaRef) {
    out.writeU8(Tag.enumeration)
    switch r {
    case .concrete(let id, let args):
        out.writeStr("Concrete")
        st(&out, "Concrete", 2)
        out.writeStr("id")
        vU64(&out, id.raw)
        out.writeStr("args")
        encRefList(&out, args)
    case .variable(let name):
        out.writeStr("Var")
        st(&out, "Var", 1)
        out.writeStr("name")
        vStr(&out, name)
    }
}

private func encField<S: Sink>(_ out: inout S, _ f: Field) {
    st(&out, "Field", 3)
    out.writeStr("name")
    vStr(&out, f.name)
    out.writeStr("schema")
    encRef(&out, f.schema)
    out.writeStr("required")
    vBool(&out, f.required)
}

private func encVariant<S: Sink>(_ out: inout S, _ v: Variant) {
    st(&out, "Variant", 3)
    out.writeStr("name")
    vStr(&out, v.name)
    out.writeStr("index")
    vU32(&out, v.index)
    out.writeStr("payload")
    encVariantPayload(&out, v.payload)
}

private func encVariantPayload<S: Sink>(_ out: inout S, _ p: VariantPayload) {
    out.writeU8(Tag.enumeration)
    switch p {
    case .unit:
        out.writeStr("Unit")
        vUnit(&out)
    case .newtype(let r):
        out.writeStr("Newtype")
        encRef(&out, r)
    case .tuple(let refs):
        out.writeStr("Tuple")
        encRefList(&out, refs)
    case .structure(let fields):
        out.writeStr("Struct")
        encFieldList(&out, fields)
    }
}

private func encRefList<S: Sink>(_ out: inout S, _ refs: [SchemaRef]) {
    listBegin(&out, refs.count)
    for r in refs {
        encRef(&out, r)
    }
}

private func encFieldList<S: Sink>(_ out: inout S, _ fields: [Field]) {
    listBegin(&out, fields.count)
    for f in fields {
        encField(&out, f)
    }
}

// MARK: - Decoding — helpers
// (`checkDepth` is shared with the value codec; see Tags.swift.)

private func expect(_ r: inout Reader, _ t: UInt8, _ what: String) throws {
    let got = try r.readU8()
    if got != t {
        throw DecodeError.unexpectedTag(expected: what, got: got)
    }
}

private func dU32(_ r: inout Reader) throws -> UInt32 {
    try expect(&r, Tag.u32, "u32")
    return try r.readU32()
}

private func dU64(_ r: inout Reader) throws -> UInt64 {
    try expect(&r, Tag.u64, "u64")
    return try r.readU64()
}

private func dBool(_ r: inout Reader) throws -> Bool {
    try expect(&r, Tag.bool, "bool")
    return try r.readBool()
}

private func dStr(_ r: inout Reader) throws -> String {
    try expect(&r, Tag.string, "string")
    return try r.readStr()
}

private func dUnit(_ r: inout Reader) throws {
    try expect(&r, Tag.unit, "unit")
}

/// Read a struct header (tag, name, field count), verifying the field count.
private func stBegin(_ r: inout Reader, _ fields: UInt32) throws {
    try expect(&r, Tag.structure, "struct")
    _ = try r.readStr() // struct name (informational)
    if try r.readU32() != fields {
        throw DecodeError.malformed("struct field count")
    }
}

/// Read and discard a struct field's name (fields are positional here).
private func fname(_ r: inout Reader) throws {
    _ = try r.readStr()
}

/// Read a list header, returning the element count (bounded by the buffer).
private func listLen(_ r: inout Reader) throws -> Int {
    try expect(&r, Tag.list, "list")
    return try r.readLen(minElemSize: 1)
}

// MARK: - Decoding — schema

private func decSchema(_ r: inout Reader, _ depth: Int) throws -> Schema {
    try checkDepth(depth)
    try stBegin(&r, 3)
    try fname(&r)
    let id = SchemaId(try dU64(&r))
    try fname(&r)
    let n = try listLen(&r)
    var typeParams: [String] = []
    typeParams.reserveCapacity(n)
    for _ in 0..<n {
        typeParams.append(try dStr(&r))
    }
    try fname(&r)
    let kind = try decKind(&r, depth + 1)
    return Schema(id: id, typeParams: typeParams, kind: kind)
}

private func decKind(_ r: inout Reader, _ depth: Int) throws -> SchemaKind {
    try checkDepth(depth)
    try expect(&r, Tag.enumeration, "enum")
    let variant = try r.readStr()
    switch variant {
    case "Primitive":
        return .primitive(try decPrimitive(&r))
    case "Struct":
        try stBegin(&r, 2)
        try fname(&r)
        let name = try dStr(&r)
        try fname(&r)
        let fields = try decFieldList(&r, depth + 1)
        return .structure(name: name, fields: fields)
    case "Enum":
        try stBegin(&r, 2)
        try fname(&r)
        let name = try dStr(&r)
        try fname(&r)
        let count = try listLen(&r)
        var variants: [Variant] = []
        variants.reserveCapacity(count)
        for _ in 0..<count {
            variants.append(try decVariant(&r, depth + 1))
        }
        return .enumeration(name: name, variants: variants)
    case "Tuple":
        try stBegin(&r, 1)
        try fname(&r)
        return .tuple(elements: try decRefList(&r, depth + 1))
    case "List":
        try stBegin(&r, 1)
        try fname(&r)
        return .list(element: try decRef(&r, depth + 1))
    case "Set":
        try stBegin(&r, 1)
        try fname(&r)
        return .set(element: try decRef(&r, depth + 1))
    case "Option":
        try stBegin(&r, 1)
        try fname(&r)
        return .option(element: try decRef(&r, depth + 1))
    case "Map":
        try stBegin(&r, 2)
        try fname(&r)
        let key = try decRef(&r, depth + 1)
        try fname(&r)
        let value = try decRef(&r, depth + 1)
        return .map(key: key, value: value)
    case "Array":
        try stBegin(&r, 2)
        try fname(&r)
        let element = try decRef(&r, depth + 1)
        try fname(&r)
        let count = try listLen(&r)
        var dimensions: [UInt64] = []
        dimensions.reserveCapacity(count)
        for _ in 0..<count {
            dimensions.append(try dU64(&r))
        }
        return .array(element: element, dimensions: dimensions)
    case "Tensor":
        try stBegin(&r, 2)
        try fname(&r)
        let element = try decRef(&r, depth + 1)
        try fname(&r)
        let rank: UInt32?
        switch try r.readU8() {
        case Tag.optionNone:
            rank = nil
        case Tag.optionSome:
            rank = try dU32(&r)
        case let got:
            throw DecodeError.unexpectedTag(expected: "option", got: got)
        }
        return .tensor(element: element, rank: rank)
    case "Channel":
        try stBegin(&r, 2)
        try fname(&r)
        let direction = try decDirection(&r)
        try fname(&r)
        let element = try decRef(&r, depth + 1)
        return .channel(direction: direction, element: element)
    case "Dynamic":
        try dUnit(&r)
        return .dynamic
    case "External":
        try stBegin(&r, 2)
        try fname(&r)
        let kind = try dStr(&r)
        try fname(&r)
        let metadata: SchemaRef?
        switch try r.readU8() {
        case Tag.optionNone:
            metadata = nil
        case Tag.optionSome:
            metadata = try decRef(&r, depth + 1)
        case let got:
            throw DecodeError.unexpectedTag(expected: "option", got: got)
        }
        return .external(kind: kind, metadata: metadata)
    default:
        throw DecodeError.unknownVariant(variant)
    }
}

private func decPrimitive(_ r: inout Reader) throws -> Primitive {
    try expect(&r, Tag.enumeration, "enum")
    let name = try r.readStr()
    try dUnit(&r)
    guard let p = Primitive(rawValue: name) else {
        throw DecodeError.unknownVariant(name)
    }
    return p
}

private func decDirection(_ r: inout Reader) throws -> ChannelDirection {
    try expect(&r, Tag.enumeration, "enum")
    let name = try r.readStr()
    try dUnit(&r)
    guard let d = ChannelDirection(rawValue: name) else {
        throw DecodeError.unknownVariant(name)
    }
    return d
}

private func decRef(_ r: inout Reader, _ depth: Int) throws -> SchemaRef {
    try checkDepth(depth)
    try expect(&r, Tag.enumeration, "enum")
    let variant = try r.readStr()
    switch variant {
    case "Concrete":
        try stBegin(&r, 2)
        try fname(&r)
        let id = SchemaId(try dU64(&r))
        try fname(&r)
        let args = try decRefList(&r, depth + 1)
        return .concrete(id: id, args: args)
    case "Var":
        try stBegin(&r, 1)
        try fname(&r)
        return .variable(name: try dStr(&r))
    default:
        throw DecodeError.unknownVariant(variant)
    }
}

private func decField(_ r: inout Reader, _ depth: Int) throws -> Field {
    try checkDepth(depth)
    try stBegin(&r, 3)
    try fname(&r)
    let name = try dStr(&r)
    try fname(&r)
    let schema = try decRef(&r, depth + 1)
    try fname(&r)
    let required = try dBool(&r)
    return Field(name: name, schema: schema, required: required)
}

private func decVariant(_ r: inout Reader, _ depth: Int) throws -> Variant {
    try checkDepth(depth)
    try stBegin(&r, 3)
    try fname(&r)
    let name = try dStr(&r)
    try fname(&r)
    let index = try dU32(&r)
    try fname(&r)
    let payload = try decVariantPayload(&r, depth + 1)
    return Variant(name: name, index: index, payload: payload)
}

private func decVariantPayload(_ r: inout Reader, _ depth: Int) throws -> VariantPayload {
    try checkDepth(depth)
    try expect(&r, Tag.enumeration, "enum")
    let variant = try r.readStr()
    switch variant {
    case "Unit":
        try dUnit(&r)
        return .unit
    case "Newtype":
        return .newtype(try decRef(&r, depth + 1))
    case "Tuple":
        return .tuple(try decRefList(&r, depth + 1))
    case "Struct":
        return .structure(try decFieldList(&r, depth + 1))
    default:
        throw DecodeError.unknownVariant(variant)
    }
}

private func decRefList(_ r: inout Reader, _ depth: Int) throws -> [SchemaRef] {
    let n = try listLen(&r)
    var v: [SchemaRef] = []
    v.reserveCapacity(n)
    for _ in 0..<n {
        v.append(try decRef(&r, depth + 1))
    }
    return v
}

private func decFieldList(_ r: inout Reader, _ depth: Int) throws -> [Field] {
    let n = try listLen(&r)
    var v: [Field] = []
    v.reserveCapacity(n)
    for _ in 0..<n {
        v.append(try decField(&r, depth + 1))
    }
    return v
}
