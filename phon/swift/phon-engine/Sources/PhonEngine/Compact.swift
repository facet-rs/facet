// The compact (schema-driven) codec for the dynamic `Value`.
//
// Compact mode carries no tags and no names: the schema says what comes next, so
// the bytes are just the values, back to back, with alignment padding before
// aligned scalars. This encodes/decodes a `Value` against a schema — the
// schema-less counterpart to the self-describing `Value` codec.
//
// Mirrors `rust/phon-engine/src/compact.rs` byte-for-byte.

import PhonSchema

// MARK: - Alignment

func alignment(_ p: Primitive) -> Int {
    switch p {
    case .u16, .i16: return 2
    case .u32, .i32, .f32, .char: return 4
    case .u64, .i64, .f64: return 8
    case .u128, .i128: return 16
    default: return 1
    }
}

// MARK: - Minimum wire size

private let minWireDepth = 64

/// The minimum wire size to pass the length guard for a sequence element of
/// schema `rf`: `0` only when the element provably encodes to zero bytes, else `1`.
func minWireSizeRef(_ reg: Registry, _ rf: SchemaRef) -> Int {
    isZeroSizedRef(reg, rf, 0) ? 0 : 1
}

private func isZeroSizedRef(_ reg: Registry, _ rf: SchemaRef, _ depth: Int) -> Bool {
    if depth > minWireDepth { return false }
    guard let resolved = try? resolve(reg, rf) else { return false }
    switch resolved {
    case .primitive(let p): return isZeroSizedPrimitive(p)
    case .composite(let kind): return isZeroSizedKind(reg, kind, depth)
    }
}

private func isZeroSizedPrimitive(_ p: Primitive) -> Bool {
    // `unit` writes nothing; everything else writes at least one byte (`never` is
    // uninhabited, reported nonzero as the safe default).
    p == .unit
}

private func isZeroSizedKind(_ reg: Registry, _ kind: SchemaKind, _ depth: Int) -> Bool {
    switch kind {
    case .primitive(let p):
        return isZeroSizedPrimitive(p)
    case .structure(_, let fields):
        return fields.allSatisfy { isZeroSizedRef(reg, $0.schema, depth + 1) }
    case .tuple(let elements):
        return elements.allSatisfy { isZeroSizedRef(reg, $0, depth + 1) }
    case .array(let element, _):
        return isZeroSizedRef(reg, element, depth + 1)
    default:
        return false
    }
}

// MARK: - Dimensions

func product(_ dimensions: [UInt64]) throws -> UInt64 {
    var p: UInt64 = 1
    for d in dimensions {
        let (r, ov) = p.multipliedReportingOverflow(by: d)
        if ov { throw CompactError.decode(.malformed("array dimensions overflow")) }
        p = r
    }
    return p
}

/// Bound a fixed-array element `count` before the construction loop. With each
/// element costing at least `minWire` bytes the count may not exceed
/// `remaining / minWire`; for a zero-sized element a fixed cap applies.
func checkFixedCount(_ count: UInt64, _ minWire: Int, _ remaining: Int) throws {
    let max = minWire == 0 ? UInt64(zstCountCap) : UInt64(remaining / minWire)
    if count > max {
        throw CompactError.decode(.lengthTooLarge(count: count, remaining: remaining))
    }
}

// MARK: - Public API

/// Encode `value` against the schema named by `root` in `registry`.
public func encode(_ value: Value, _ root: SchemaId, _ registry: Registry) throws -> [UInt8] {
    var out = ByteSink()
    try encodeRef(value, .concrete(id: root, args: []), registry, &out)
    return out.bytes
}

/// Decode a value of schema `root` from `bytes`, rejecting trailing bytes.
public func decode(_ bytes: [UInt8], _ root: SchemaId, _ registry: Registry) throws -> Value {
    var r = Reader(bytes)
    let v = try decodeRef(&r, .concrete(id: root, args: []), registry, 0)
    if r.remaining != 0 {
        throw CompactError.decode(.trailingBytes(r.remaining))
    }
    return v
}

// MARK: - Encoding

func encodeRef(_ value: Value, _ r: SchemaRef, _ reg: Registry, _ out: inout ByteSink) throws {
    switch r {
    case .variable:
        throw CompactError.malformed("unbound type variable")
    case .concrete(let id, let args):
        if let p = reg.primitive(id) {
            if !args.isEmpty { throw CompactError.malformed("primitive carrying type arguments") }
            try encodePrimitive(value, p, &out)
        } else if let schema = reg.composite(id) {
            if schema.typeParams.count != args.count {
                throw CompactError.genericArity(params: schema.typeParams.count, args: args.count)
            }
            let kind = args.isEmpty ? schema.kind : substituteKind(schema.kind, schema.typeParams, args)
            try encodeKind(value, kind, reg, &out)
        } else {
            throw CompactError.unknownSchema(id)
        }
    }
}

private func number(_ value: Value) throws -> Number {
    guard let n = value.asNumber else { throw CompactError.typeMismatch(expected: "number") }
    return n
}

private func encodePrimitive(_ value: Value, _ p: Primitive, _ out: inout ByteSink) throws {
    out.padTo(alignment(p))
    switch p {
    case .bool:
        guard let b = value.asBool else { throw CompactError.typeMismatch(expected: "bool") }
        out.writeBool(b)
    case .u8:
        out.writeU8(UInt8(truncatingIfNeeded: try number(value).toU64 ?? 0))
    case .u16:
        out.writeU16(UInt16(truncatingIfNeeded: try number(value).toU64 ?? 0))
    case .u32:
        out.writeU32(UInt32(truncatingIfNeeded: try number(value).toU64 ?? 0))
    case .u64:
        out.writeU64(try number(value).toU64 ?? 0)
    case .u128:
        out.writeU128(try number(value).toU128 ?? 0)
    case .i8:
        out.writeI8(Int8(truncatingIfNeeded: try number(value).toI64 ?? 0))
    case .i16:
        out.writeI16(Int16(truncatingIfNeeded: try number(value).toI64 ?? 0))
    case .i32:
        out.writeI32(Int32(truncatingIfNeeded: try number(value).toI64 ?? 0))
    case .i64:
        out.writeI64(try number(value).toI64 ?? 0)
    case .i128:
        out.writeI128(try number(value).toI128 ?? 0)
    case .f32:
        out.writeF32(Float(try number(value).toF64Lossy))
    case .f64:
        out.writeF64(try number(value).toF64Lossy)
    case .char:
        guard let c = value.asChar else { throw CompactError.typeMismatch(expected: "char") }
        out.writeU32(c.value)
    case .string:
        guard let s = value.asString else { throw CompactError.typeMismatch(expected: "string") }
        out.writeStr(s)
    case .bytes:
        guard let b = value.asBytes else { throw CompactError.typeMismatch(expected: "bytes") }
        b.withUnsafeBytes { out.writeBytes($0) }
    case .unit:
        if !value.isNull { throw CompactError.typeMismatch(expected: "unit") }
    case .never:
        throw CompactError.typeMismatch(expected: "never")
    case .datetime, .uuid, .qname:
        guard let s = extendedToString(value, p) else {
            throw CompactError.encode("no self-describing encoding for value kind")
        }
        out.writeStr(s)
    }
}

private func encodeKind(_ value: Value, _ kind: SchemaKind, _ reg: Registry, _ out: inout ByteSink) throws {
    switch kind {
    case .primitive(let p):
        try encodePrimitive(value, p, &out)
    case .structure(_, let fields):
        guard value.asObject != nil else { throw CompactError.typeMismatch(expected: "object") }
        for field in fields {
            guard let fv = value.get(field.name) else {
                throw CompactError.typeMismatch(expected: "struct field")
            }
            try encodeRef(fv, field.schema, reg, &out)
        }
    case .tuple(let elements):
        guard let arr = value.asArray else { throw CompactError.typeMismatch(expected: "tuple") }
        if arr.count != elements.count { throw CompactError.typeMismatch(expected: "tuple arity") }
        for (i, e) in elements.enumerated() {
            try encodeRef(arr[i], e, reg, &out)
        }
    case .list(let element), .set(let element):
        guard let arr = value.asArray else { throw CompactError.typeMismatch(expected: "list") }
        out.writeU32(UInt32(arr.count))
        for e in arr { try encodeRef(e, element, reg, &out) }
    case .array(let element, let dimensions):
        let count = try product(dimensions)
        guard let arr = value.asArray else { throw CompactError.typeMismatch(expected: "array") }
        if UInt64(arr.count) != count { throw CompactError.typeMismatch(expected: "array shape") }
        for e in arr { try encodeRef(e, element, reg, &out) }
    case .map(let key, let val):
        guard let obj = value.asObject else { throw CompactError.typeMismatch(expected: "map") }
        out.writeU32(UInt32(obj.count))
        for entry in obj {
            try encodeRef(.string(entry.key), key, reg, &out)
            try encodeRef(entry.value, val, reg, &out)
        }
    case .option(let element):
        if value.isNull {
            out.writeU8(0)
        } else {
            out.writeU8(1)
            try encodeRef(value, element, reg, &out)
        }
    case .enumeration(_, let variants):
        guard let obj = value.asObject else { throw CompactError.typeMismatch(expected: "enum object") }
        if obj.count != 1 { throw CompactError.typeMismatch(expected: "single-variant enum object") }
        let entry = obj[0]
        guard let variant = variants.first(where: { $0.name == entry.key }) else {
            throw CompactError.unknownVariant(entry.key)
        }
        out.writeU32(variant.index)
        try encodePayload(entry.value, variant.payload, reg, &out)
    case .dynamic:
        writeValue(&out, value)
    case .tensor:
        throw CompactError.unsupported("tensor")
    case .channel:
        throw CompactError.unsupported("channel")
    case .external:
        throw CompactError.unsupported("external")
    }
}

private func encodePayload(_ value: Value, _ payload: VariantPayload, _ reg: Registry, _ out: inout ByteSink) throws {
    switch payload {
    case .unit:
        break
    case .newtype(let r):
        try encodeRef(value, r, reg, &out)
    case .tuple(let refs):
        guard let arr = value.asArray else { throw CompactError.typeMismatch(expected: "tuple variant") }
        if arr.count != refs.count { throw CompactError.typeMismatch(expected: "tuple variant arity") }
        for (i, r) in refs.enumerated() { try encodeRef(arr[i], r, reg, &out) }
    case .structure(let fields):
        guard value.asObject != nil else { throw CompactError.typeMismatch(expected: "struct variant") }
        for field in fields {
            guard let fv = value.get(field.name) else {
                throw CompactError.typeMismatch(expected: "struct variant field")
            }
            try encodeRef(fv, field.schema, reg, &out)
        }
    }
}

// MARK: - Decoding

func decodeRef(_ r: inout Reader, _ rf: SchemaRef, _ reg: Registry, _ depth: Int) throws -> Value {
    if depth > compactMaxDepth { throw CompactError.decode(.depthExceeded) }
    switch rf {
    case .variable:
        throw CompactError.malformed("unbound type variable")
    case .concrete(let id, let args):
        if let p = reg.primitive(id) {
            if !args.isEmpty { throw CompactError.malformed("primitive carrying type arguments") }
            return try decodePrimitive(&r, p)
        } else if let schema = reg.composite(id) {
            if schema.typeParams.count != args.count {
                throw CompactError.genericArity(params: schema.typeParams.count, args: args.count)
            }
            let kind = args.isEmpty ? schema.kind : substituteKind(schema.kind, schema.typeParams, args)
            return try decodeKind(&r, kind, reg, depth + 1)
        } else {
            throw CompactError.unknownSchema(id)
        }
    }
}

func decodePrimitive(_ r: inout Reader, _ p: Primitive) throws -> Value {
    try r.skipPad(alignment(p))
    switch p {
    case .bool: return .bool(try r.readBool())
    case .u8: return .number(.canonical(unsigned: UInt128(try r.readU8())))
    case .u16: return .number(.canonical(unsigned: UInt128(try r.readU16())))
    case .u32: return .number(.canonical(unsigned: UInt128(try r.readU32())))
    case .u64: return .number(.canonical(unsigned: UInt128(try r.readU64())))
    case .u128: return .number(.canonical(unsigned: try r.readU128()))
    case .i8: return .number(.canonical(signed: Int128(try r.readI8())))
    case .i16: return .number(.canonical(signed: Int128(try r.readI16())))
    case .i32: return .number(.canonical(signed: Int128(try r.readI32())))
    case .i64: return .number(.canonical(signed: Int128(try r.readI64())))
    case .i128: return .number(.canonical(signed: try r.readI128()))
    case .f32: return .number(.f64(Double(try r.readF32())))
    case .f64: return .number(.f64(try r.readF64()))
    case .char: return .char(try r.readChar())
    case .string: return .string(try r.readStr())
    case .bytes: return .bytes(Array(try r.readBytes()))
    case .unit: return .null
    case .never:
        throw CompactError.decode(.malformed("never is uninhabited"))
    case .datetime, .uuid, .qname:
        return try extendedFromString(r.readStr(), p)
    }
}

private func decodeKind(_ r: inout Reader, _ kind: SchemaKind, _ reg: Registry, _ depth: Int) throws -> Value {
    switch kind {
    case .primitive(let p):
        return try decodePrimitive(&r, p)
    case .structure(_, let fields):
        var obj: [Value.Entry] = []
        for field in fields {
            obj.append(Value.Entry(key: field.name, value: try decodeRef(&r, field.schema, reg, depth)))
        }
        return .object(obj)
    case .tuple(let elements):
        var arr: [Value] = []
        for e in elements { arr.append(try decodeRef(&r, e, reg, depth)) }
        return .array(arr)
    case .list(let element):
        let n = try r.readLen(minElemSize: minWireSizeRef(reg, element))
        var arr: [Value] = []
        for _ in 0..<n { arr.append(try decodeRef(&r, element, reg, depth)) }
        return .array(arr)
    case .set(let element):
        let n = try r.readLen(minElemSize: minWireSizeRef(reg, element))
        var arr: [Value] = []
        var seen: Set<Value> = []
        for _ in 0..<n {
            let v = try decodeRef(&r, element, reg, depth)
            guard seen.insert(v).inserted else { throw CompactError.decode(.duplicateElement) }
            arr.append(v)
        }
        return .array(arr)
    case .array(let element, let dimensions):
        let count = try product(dimensions)
        try checkFixedCount(count, minWireSizeRef(reg, element), r.remaining)
        var arr: [Value] = []
        for _ in 0..<count { arr.append(try decodeRef(&r, element, reg, depth)) }
        return .array(arr)
    case .map(let key, let value):
        let n = try r.readLen(minElemSize: 1)
        var obj: [Value.Entry] = []
        var seen: Set<String> = []
        for _ in 0..<n {
            let k = try decodeRef(&r, key, reg, depth)
            let v = try decodeRef(&r, value, reg, depth)
            guard let ks = k.asString else { throw CompactError.unsupported("map with non-string keys") }
            guard seen.insert(ks).inserted else { throw CompactError.decode(.duplicateKey) }
            obj.append(Value.Entry(key: ks, value: v))
        }
        return .object(obj)
    case .option(let element):
        switch try r.readU8() {
        case 0: return .null
        case 1: return try decodeRef(&r, element, reg, depth)
        case let b: throw CompactError.decode(.invalidBool(b))
        }
    case .enumeration(_, let variants):
        let index = try r.readU32()
        guard let variant = variants.first(where: { $0.index == index }) else {
            throw CompactError.badVariantIndex(index)
        }
        let payload = try decodePayload(&r, variant.payload, reg, depth)
        return .object([Value.Entry(key: variant.name, value: payload)])
    case .dynamic:
        return try readValue(&r)
    case .tensor:
        throw CompactError.unsupported("tensor")
    case .channel:
        throw CompactError.unsupported("channel")
    case .external:
        throw CompactError.unsupported("external")
    }
}

private func decodePayload(_ r: inout Reader, _ payload: VariantPayload, _ reg: Registry, _ depth: Int) throws -> Value {
    switch payload {
    case .unit:
        return .null
    case .newtype(let rf):
        return try decodeRef(&r, rf, reg, depth)
    case .tuple(let refs):
        var arr: [Value] = []
        for rf in refs { arr.append(try decodeRef(&r, rf, reg, depth)) }
        return .array(arr)
    case .structure(let fields):
        var obj: [Value.Entry] = []
        for field in fields {
            obj.append(Value.Entry(key: field.name, value: try decodeRef(&r, field.schema, reg, depth)))
        }
        return .object(obj)
    }
}
