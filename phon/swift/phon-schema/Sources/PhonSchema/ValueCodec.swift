// The coarse self-describing `Value` codec.
//
// The rich tag set folds onto `Value`'s small model: `list`/`tuple`/`set`/`array`/
// `tensor` all become an array; `map`/`struct`/`enum` become an object; `unit` and
// `option-none` become null. Each `Value` case has a fixed tag, so the bytes a
// `Value` re-encodes to are canonical. This is the codec the `Dynamic` kind and
// the metadata map ride.
//
// Mirrors the value half of `rust/phon-schema/src/selfdescribing.rs` byte-for-byte.

// MARK: - Public API

/// Encode a `Value` to self-describing bytes.
// r[impl value]
// r[impl self-describing.tag-led]
// r[impl self-describing.no-extra-kinds]
public func valueToBytes(_ v: Value) -> [UInt8] {
    var out = ByteSink()
    writeValue(&out, v)
    return out.bytes
}

/// Decode a `Value` from self-describing bytes, rejecting trailing bytes.
// r[impl value]
// r[impl self-describing.tag-led]
// r[impl self-describing.no-extra-kinds]
// r[impl decode.whole-message]
public func valueFromBytes(_ buf: [UInt8]) throws -> Value {
    var r = Reader(buf)
    let v = try decValue(&r, 0)
    if r.remaining != 0 {
        throw DecodeError.trailingBytes(r.remaining)
    }
    return v
}

/// Read a `Value` from a reader (for embedding, e.g. a `Dynamic` field).
// r[impl value]
// r[impl self-describing.tag-led]
public func readValue(_ r: inout Reader) throws -> Value {
    try decValue(&r, 0)
}

// MARK: - Encoding

public func writeValue<S: Sink>(_ out: inout S, _ value: Value) {
    switch value {
    case .null:
        out.writeU8(Tag.optionNone)
    case .bool(let b):
        out.writeU8(Tag.bool)
        out.writeBool(b)
    case .number(let n):
        encNumber(&out, n)
    case .string(let s):
        out.writeU8(Tag.string)
        out.writeStr(s)
    case .bytes(let b):
        out.writeU8(Tag.bytes)
        b.withUnsafeBytes { out.writeBytes($0) }
    case .char(let c):
        out.writeU8(Tag.char)
        out.writeU32(c.value)
    case .array(let a):
        out.writeU8(Tag.list)
        out.writeU32(UInt32(a.count))
        for e in a { writeValue(&out, e) }
    case .object(let entries):
        out.writeU8(Tag.map)
        out.writeU32(UInt32(entries.count))
        for e in entries {
            out.writeU8(Tag.string)
            out.writeStr(e.key)
            writeValue(&out, e.value)
        }
    // r[impl value.extended-kinds]
    case .datetime(let d):
        out.writeU8(Tag.datetime)
        out.writeStr(datetimeString(d))
    case .uuid(let n):
        out.writeU8(Tag.uuid)
        out.writeStr(uuidString(n))
    case .qname(let ns, let local):
        out.writeU8(Tag.qname)
        out.writeStr(qnameString(ns, local))
    }
}

/// A number's wire tag follows its canonical width — the same choice the
/// `Number` decoder made, so encode is the exact inverse.
private func encNumber<S: Sink>(_ out: inout S, _ n: Number) {
    switch n {
    case .f64(let d):
        out.writeU8(Tag.f64)
        out.writeF64(d)
    case .i64(let i):
        out.writeU8(Tag.i64)
        out.writeI64(i)
    case .u64(let u):
        out.writeU8(Tag.u64)
        out.writeU64(u)
    case .i128(let i):
        out.writeU8(Tag.i128)
        out.writeI128(i)
    case .u128(let u):
        out.writeU8(Tag.u128)
        out.writeU128(u)
    }
}

// MARK: - Decoding

private func decValue(_ r: inout Reader, _ depth: Int) throws -> Value {
    try checkDepth(depth)
    let t = try r.readU8()
    switch t {
    case Tag.unit, Tag.optionNone:
        return .null
    case Tag.bool:
        return .bool(try r.readBool())
    case Tag.u8:
        return .number(.canonical(unsigned: UInt128(try r.readU8())))
    case Tag.u16:
        return .number(.canonical(unsigned: UInt128(try r.readU16())))
    case Tag.u32:
        return .number(.canonical(unsigned: UInt128(try r.readU32())))
    case Tag.u64:
        return .number(.canonical(unsigned: UInt128(try r.readU64())))
    case Tag.u128:
        return .number(.canonical(unsigned: try r.readU128()))
    case Tag.i8:
        return .number(.canonical(signed: Int128(try r.readI8())))
    case Tag.i16:
        return .number(.canonical(signed: Int128(try r.readI16())))
    case Tag.i32:
        return .number(.canonical(signed: Int128(try r.readI32())))
    case Tag.i64:
        return .number(.canonical(signed: Int128(try r.readI64())))
    case Tag.i128:
        return .number(.canonical(signed: try r.readI128()))
    case Tag.f32:
        // A 32-bit float widens to f64 on decode (lossless), matching Rust's
        // `Value::from(f32)` -> `enc_number` re-emitting as f64.
        return .number(.f64(Double(try r.readF32())))
    case Tag.f64:
        return .number(.f64(try r.readF64()))
    case Tag.char:
        return .char(try r.readChar())
    case Tag.string:
        return .string(try r.readStr())
    case Tag.bytes:
        return .bytes(Array(try r.readBytes()))
    // list and tuple both fold to a flat array.
    case Tag.list, Tag.tuple:
        let n = try r.readLen(minElemSize: 1)
        var a: [Value] = []
        a.reserveCapacity(n)
        for _ in 0..<n { a.append(try decValue(&r, depth + 1)) }
        return .array(a)
    case Tag.set:
        let n = try r.readLen(minElemSize: 1)
        var a: [Value] = []
        a.reserveCapacity(n)
        var seen: Set<Value> = []
        for _ in 0..<n {
            let elem = try decValue(&r, depth + 1)
            guard seen.insert(elem).inserted else { throw DecodeError.duplicateElement }
            a.append(elem)
        }
        return .array(a)
    case Tag.map:
        return try decMap(&r, depth)
    case Tag.array, Tag.tensor:
        return try decDimensioned(&r, depth)
    case Tag.structure:
        return try decStructValue(&r, depth)
    case Tag.enumeration:
        return try decEnumValue(&r, depth)
    case Tag.optionSome:
        return try decValue(&r, depth + 1)
    // r[impl value.extended-kinds]
    case Tag.datetime:
        return .datetime(try parseDatetime(r.readStr()))
    case Tag.uuid:
        return .uuid(try parseUuid(r.readStr()))
    case Tag.qname:
        let (ns, local) = try parseQName(r.readStr())
        return .qname(namespace: ns, local: local)
    // r[impl validate.tags]
    default:
        throw DecodeError.unknownTag(t)
    }
}

/// A `map` folds to an object when its keys are all strings, else to an array of
/// `[key, value]` pairs. Keys must be unique either way.
private func decMap(_ r: inout Reader, _ depth: Int) throws -> Value {
    let n = try r.readLen(minElemSize: 2)
    var entries: [(Value, Value)] = []
    var seen: Set<Value> = []
    var allString = true
    for _ in 0..<n {
        let key = try decValue(&r, depth + 1)
        let val = try decValue(&r, depth + 1)
        guard seen.insert(key).inserted else { throw DecodeError.duplicateKey }
        if case .string = key {} else { allString = false }
        entries.append((key, val))
    }
    if allString {
        var obj: [Value.Entry] = []
        obj.reserveCapacity(entries.count)
        for (key, val) in entries {
            guard case .string(let s) = key else { continue }
            obj.append(Value.Entry(key: s, value: val))
        }
        return .object(obj)
    } else {
        return .array(entries.map { .array([$0.0, $0.1]) })
    }
}

/// `array` and `tensor` fold to a flat array. The dimensions are validated: rank
/// and the element product are bounded by the buffer, computed with checked
/// arithmetic.
// r[impl validate.dimensions]
private func decDimensioned(_ r: inout Reader, _ depth: Int) throws -> Value {
    let rank = Int(try r.readU32())
    let (rankBytes, rankOv) = rank.multipliedReportingOverflow(by: 8)
    if rankOv || rankBytes > r.remaining {
        throw DecodeError.lengthTooLarge(count: UInt64(rank), remaining: r.remaining)
    }
    var product: UInt64 = 1
    for _ in 0..<rank {
        let dim = try r.readU64()
        let (p, ov) = product.multipliedReportingOverflow(by: dim)
        if ov { throw DecodeError.malformed("array/tensor dimension overflow") }
        product = p
    }
    if product > UInt64(r.remaining) {
        throw DecodeError.lengthTooLarge(count: product, remaining: r.remaining)
    }
    var a: [Value] = []
    for _ in 0..<product { a.append(try decValue(&r, depth + 1)) }
    return .array(a)
}

/// A `struct` folds to an object keyed by field name (names must be unique).
private func decStructValue(_ r: inout Reader, _ depth: Int) throws -> Value {
    _ = try r.readStr() // struct name, folded away
    let n = try r.readLen(minElemSize: 1)
    var obj: [Value.Entry] = []
    var seen: Set<String> = []
    for _ in 0..<n {
        let field = try r.readStr()
        guard seen.insert(field).inserted else { throw DecodeError.duplicateKey }
        let val = try decValue(&r, depth + 1)
        obj.append(Value.Entry(key: field, value: val))
    }
    return .object(obj)
}

/// An `enum` folds to a one-entry object mapping the variant name to its single
/// payload value.
// r[impl self-describing.enum-payload]
private func decEnumValue(_ r: inout Reader, _ depth: Int) throws -> Value {
    let variant = try r.readStr()
    let payload = try decValue(&r, depth + 1)
    return .object([Value.Entry(key: variant, value: payload)])
}
