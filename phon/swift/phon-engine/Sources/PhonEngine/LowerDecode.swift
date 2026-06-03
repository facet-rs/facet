// The compat typed decode lowering.
//
// `lowerDecode` walks a *writer* schema against the *reader* descriptor and bakes
// the writer↔reader compatibility decision in ONCE, at lowering, producing a flat
// `MemProgram` of reader-memory ops in WIRE order. There is no fast/slow path:
// when the writer schema is the one the reader carries, the result has no skips or
// defaults and is equivalent to `lowerTyped` — the identity case. This is the
// ONLY typed decode lowering; `lowerTyped` is encode-only.
//
// Compatibility mirrors `Plan.swift` (the cross-engine oracle): struct fields and
// enum variants match by name, writer-only fields are skipped, reader-only fields
// default (or fail if required), and primitives match with no implicit widening.
//
// Mirrors `rust/phon-engine/src/typed.rs::lower_decode` + `skip_op`, adapted to
// the Swift descriptor's witness model (project/inject closures rather than a
// direct in-memory discriminant).

import PhonIR
import PhonSchema

// MARK: - Entry point

/// Lower `(writer schema ⋈ reader descriptor)` into the reader-memory decode
/// program, in wire order.
///
/// A recursive reader lowers each of its cyclic schemas to a callable block, just
/// as `lowerTyped` does — a `.recurse` reader node becomes a `.callBlock` into one
/// of these. For the same-schema path the writer's schema at every `.recurse`
/// position is that same schema, so a block translates
/// `concrete(R) ⋈ readerBlocks[R]` — the identity case. Compatibility across a
/// recursion boundary is the tracked follow-up; here the block's writer ref is
/// the reader schema id.
// r[impl compat.plan-first]
public func lowerDecode(
    _ writerRoot: SchemaId, _ reader: Descriptor, _ reg: Registry,
    _ readerBlocks: [SchemaId: Descriptor] = [:]
) throws -> Lowered {
    var out: MemProgram = []
    try lowerDecodeNode(.concrete(id: writerRoot, args: []), reader, reg, 0, &out)
    var blocks: [SchemaId: MemProgram] = [:]
    for (id, body) in readerBlocks {
        var ops: MemProgram = []
        try lowerDecodeNode(.concrete(id: id, args: []), body, reg, 0, &ops)
        blocks[id] = fuse(ops)
    }
    return Lowered(program: fuse(out), blocks: blocks)
}

/// The same-schema decode: the writer is the schema the reader carries. The
/// resulting program has no skips/defaults — the identity case. (There is
/// no separate same-schema decoder; this is `lowerDecode` with writer == reader.)
public func lowerDecode(
    _ reader: Descriptor, _ reg: Registry, _ readerBlocks: [SchemaId: Descriptor] = [:]
) throws -> Lowered {
    guard case .concrete(let id, _) = reader.schema else {
        throw CompactError.malformed("lowerDecode: root descriptor schema must be concrete")
    }
    return try lowerDecode(id, reader, reg, readerBlocks)
}

// r[impl compat.type-match]
private func lowerDecodeNode(_ writer: SchemaRef, _ reader: Descriptor, _ reg: Registry, _ base: Int, _ out: inout MemProgram) throws {
    // A recursive reader back-edge: emit a call into that schema's block, run at
    // `base + offset`. `lowerDecode` lowers the block itself from `readerBlocks`.
    if case .recurse = reader.access {
        guard case .concrete(let id, _) = reader.schema else {
            throw CompactError.unsupported("typed: recursion via type-var ref (decode)")
        }
        out.append(.callBlock(schema: id, offset: base))
        return
    }
    let w = try resolve(reg, writer)
    switch (reader.access, w) {
    case (.scalar, .primitive(let wp)):
        // No implicit numeric widening: writer and reader primitive must match.
        guard case .primitive(let rp) = try resolve(reg, reader.schema) else {
            throw CompactError.typeMismatch(expected: "scalar reader schema for a scalar descriptor")
        }
        guard wp == rp else { throw CompactError.incompatible("primitive \(wp) is not \(rp)") }
        guard let size = fixedSize(wp) else {
            throw CompactError.unsupported("typed: variable-length scalar field")
        }
        out.append(.scalar(offset: base, size: size, align: alignment(wp)))

    case (.record(let ra), .composite(.structure(_, let wf))):
        try lowerDecodeStruct(wf, ra, reader.schema, reg, base, &out)

    case (.enumeration(let ea), .composite(.enumeration(_, let wv))):
        try lowerDecodeEnum(wv, ea, reader.schema, reg, base, &out)

    case (.option(let oa), .composite(.option(let we))):
        try requireReaderOption(reader.schema, reg)
        var some: MemProgram = []
        try lowerDecodeNode(we, oa.some, reg, 0, &some)
        out.append(.option(OptionOp(
            offset: base, some: fuse(some),
            innerSize: oa.some.layout.size, innerAlign: oa.some.layout.align,
            witness: oa.witness)))

    case (.bytes(let ba), let resolved):
        try requireReaderBulk(reader.schema, matches: resolved, reg)
        out.append(.bytes(BytesOp(offset: base, stride: ba.stride, elemAlign: ba.elemAlign, witness: ba.witness)))

    case (.sequence(let sa), .composite(.list(let we))):
        try requireReaderList(reader.schema, reg)
        var element: MemProgram = []
        try lowerDecodeNode(we, sa.element, reg, 0, &element)
        out.append(.sequence(SeqOp(
            offset: base, element: fuse(element), stride: sa.stride, elemAlign: sa.elemAlign,
            minWire: elemMinWire(element), witness: sa.witness)))

    case (.sequence(let sa), .composite(.set(let we))):
        try requireReaderSet(reader.schema, reg)
        var element: MemProgram = []
        try lowerDecodeNode(we, sa.element, reg, 0, &element)
        out.append(.sequence(SeqOp(
            offset: base, element: fuse(element), stride: sa.stride, elemAlign: sa.elemAlign,
            minWire: elemMinWire(element), witness: sa.witness)))

    case (.map(let ma), .composite(.map(let wk, let wv))):
        try requireReaderMap(reader.schema, reg)
        var key: MemProgram = []
        try lowerDecodeNode(wk, ma.key, reg, 0, &key)
        var value: MemProgram = []
        try lowerDecodeNode(wv, ma.value, reg, 0, &value)
        out.append(.map(MapOp(
            offset: base, key: fuse(key), value: fuse(value),
            keyStride: ma.keyStride, keyAlign: ma.keyAlign,
            valueStride: ma.valueStride, valueAlign: ma.valueAlign, witness: ma.witness)))

    case (.dynamic, .composite(.dynamic)):
        try requireReaderDynamic(reader.schema, reg)
        out.append(.dynamic(offset: base))

    default:
        throw CompactError.incompatible("writer and reader schema kinds differ")
    }
}

private func requireReaderOption(_ reader: SchemaRef, _ reg: Registry) throws {
    guard case .composite(.option) = try resolve(reg, reader) else {
        throw CompactError.typeMismatch(expected: "option reader schema")
    }
}

private func requireReaderList(_ reader: SchemaRef, _ reg: Registry) throws {
    guard case .composite(.list) = try resolve(reg, reader) else {
        throw CompactError.typeMismatch(expected: "list reader schema")
    }
}

private func requireReaderSet(_ reader: SchemaRef, _ reg: Registry) throws {
    guard case .composite(.set) = try resolve(reg, reader) else {
        throw CompactError.typeMismatch(expected: "set reader schema")
    }
}

private func requireReaderMap(_ reader: SchemaRef, _ reg: Registry) throws {
    guard case .composite(.map) = try resolve(reg, reader) else {
        throw CompactError.typeMismatch(expected: "map reader schema")
    }
}

private func requireReaderDynamic(_ reader: SchemaRef, _ reg: Registry) throws {
    guard case .composite(.dynamic) = try resolve(reg, reader) else {
        throw CompactError.typeMismatch(expected: "dynamic reader schema")
    }
}

private func requireReaderBulk(_ reader: SchemaRef, matches writer: Resolved, _ reg: Registry) throws {
    let r = try resolve(reg, reader)
    switch (writer, r) {
    case (.primitive(.string), .primitive(.string)),
         (.primitive(.bytes), .primitive(.bytes)),
         (.composite(.list), .composite(.list)),
         (.composite(.set), .composite(.set)):
        return
    default:
        throw CompactError.incompatible("writer and reader bulk schema kinds differ")
    }
}

// MARK: - Struct compat

// r[impl compat.field-matching]
// r[impl compat.skip-writer-only]
// r[impl compat.reader-only-fields]
// r[impl compat.defaults-are-reader-side]
private func lowerDecodeStruct(_ wFields: [Field], _ ra: RecordAccess, _ readerSchema: SchemaRef, _ reg: Registry, _ base: Int, _ out: inout MemProgram) throws {
    switch ra.construct {
    case .inPlace: break
    case .thunk: throw CompactError.unsupported("typed: thunk construction")
    }
    // Reader field names come from the reader schema, aligned by index with the
    // descriptor's fields (the bridge builds them in the same order).
    let rNamed = try readerStructFields(readerSchema, reg)
    guard rNamed.count == ra.fields.count else {
        throw CompactError.malformed("descriptor/schema field count mismatch")
    }
    var readerByName: [String: Int] = [:]
    for (i, f) in rNamed.enumerated() { readerByName[f.name] = i }

    // One step per writer field, in wire order: take the matched reader field, or
    // skip the writer-only one.
    var matched = [Bool](repeating: false, count: ra.fields.count)
    for wf in wFields {
        if let ri = readerByName[wf.name] {
            let fa = ra.fields[ri]
            try lowerDecodeNode(wf.schema, fa.descriptor, reg, base + fa.offset, &out)
            matched[ri] = true
        } else {
            out.append(.skipWire(try skipOp(wf.schema, reg)))
        }
    }
    // Reader-only fields: default in place, or — if required — incompatible.
    for (ri, fa) in ra.fields.enumerated() where !matched[ri] {
        guard let initFn = fa.defaultInit else {
            throw CompactError.incompatible("required reader field '\(rNamed[ri].name)' is absent from the writer")
        }
        out.append(.writeDefault(DefaultOp(offset: base + fa.offset, initFn: initFn)))
    }
}

// MARK: - Enum compat

// r[impl compat.enum]
private func lowerDecodeEnum(_ wVariants: [Variant], _ ea: EnumAccess, _ readerSchema: SchemaRef, _ reg: Registry, _ base: Int, _ out: inout MemProgram) throws {
    let rNamed = try readerEnumVariants(readerSchema, reg)
    guard rNamed.count == ea.variants.count else {
        throw CompactError.malformed("descriptor/schema variant count mismatch")
    }
    var readerByName: [String: Int] = [:]
    for (i, v) in rNamed.enumerated() { readerByName[v.name] = i }

    var variantOps: [EnumVariantOp] = []
    var writerOnly: [UInt32] = []
    for wv in wVariants {
        guard let ri = readerByName[wv.name] else {
            // A writer variant the reader lacks: receiving it is a decode error.
            writerOnly.append(wv.index)
            continue
        }
        let va = ea.variants[ri]
        let payload = try lowerDecodePayload(wv.payload, va, rNamed[ri].payload, reg)
        variantOps.append(EnumVariantOp(
            wireIndex: wv.index,
            readerLocalIndex: ri,
            payload: payload,
            payloadSize: va.payloadLayout.size,
            payloadAlign: va.payloadLayout.align))
    }
    out.append(.enumeration(EnumOp(
        offset: base,
        tag: ea.tag,
        projectPayload: ea.projectPayload,
        destroyPayload: ea.destroyPayload,
        inject: ea.inject,
        variants: variantOps,
        writerOnly: writerOnly)))
}

private func lowerDecodePayload(_ w: VariantPayload, _ va: VariantAccess, _ rPayload: VariantPayload, _ reg: Registry) throws -> MemProgram {
    var payload: MemProgram = []
    switch (w, rPayload) {
    case (.unit, .unit):
        break
    case (.newtype(let wr), .newtype):
        guard let fa = va.payloadFields.first else {
            throw CompactError.malformed("newtype variant has no payload field")
        }
        try lowerDecodeNode(wr, fa.descriptor, reg, fa.offset, &payload)
    case (.tuple(let wrs), .tuple(let rrs)):
        guard wrs.count == rrs.count, wrs.count == va.payloadFields.count else {
            throw CompactError.incompatible("variant tuple arity differs")
        }
        for (wr, fa) in zip(wrs, va.payloadFields) {
            try lowerDecodeNode(wr, fa.descriptor, reg, fa.offset, &payload)
        }
    case (.structure(let wfs), .structure(let rfs)):
        try lowerDecodeVariantStruct(wfs, va, rfs, reg, &payload)
    default:
        throw CompactError.incompatible("variant payload shapes differ")
    }
    return fuse(payload)
}

private func lowerDecodeVariantStruct(_ wFields: [Field], _ va: VariantAccess, _ rFields: [Field], _ reg: Registry, _ out: inout MemProgram) throws {
    guard rFields.count == va.payloadFields.count else {
        throw CompactError.malformed("variant descriptor/schema field count mismatch")
    }
    var readerByName: [String: Int] = [:]
    for (i, f) in rFields.enumerated() { readerByName[f.name] = i }
    var matched = [Bool](repeating: false, count: va.payloadFields.count)
    for wf in wFields {
        if let ri = readerByName[wf.name] {
            let fa = va.payloadFields[ri]
            try lowerDecodeNode(wf.schema, fa.descriptor, reg, fa.offset, &out)
            matched[ri] = true
        } else {
            out.append(.skipWire(try skipOp(wf.schema, reg)))
        }
    }
    for (ri, fa) in va.payloadFields.enumerated() where !matched[ri] {
        guard let initFn = fa.defaultInit else {
            throw CompactError.incompatible("required reader variant field '\(rFields[ri].name)' is absent from the writer")
        }
        out.append(.writeDefault(DefaultOp(offset: fa.offset, initFn: initFn)))
    }
}

// MARK: - Reader schema field/variant names

private func readerStructFields(_ r: SchemaRef, _ reg: Registry) throws -> [Field] {
    switch try resolve(reg, r) {
    case .composite(.structure(_, let fields)):
        return fields
    case .composite(.tuple(let elements)):
        // Positional: synthesize index names matching the descriptor's order.
        return elements.enumerated().map { Field(name: String($0.offset), schema: $0.element, required: true) }
    default:
        throw CompactError.typeMismatch(expected: "struct/tuple reader schema for a record descriptor")
    }
}

private func readerEnumVariants(_ r: SchemaRef, _ reg: Registry) throws -> [Variant] {
    guard case .composite(.enumeration(_, let variants)) = try resolve(reg, r) else {
        throw CompactError.typeMismatch(expected: "enum reader schema for an enum descriptor")
    }
    return variants
}

// MARK: - Skip skeletons (advance past writer-only values)

/// Advance `r` past one writer value described by `op`, writing nothing to memory.
func skip(_ r: inout Reader, _ op: SkipOp) throws {
    switch op {
    case .scalar(let size, let align):
        try r.skipPad(align)
        _ = try r.readSlice(size)
    case .bytes(let stride, let elemAlign):
        let count = try r.readLen(minElemSize: max(stride, 1))
        if count > 0 {
            try r.skipPad(elemAlign)
            _ = try r.readSlice(count * stride)
        }
    case .seq(let element):
        let count = try r.readLen(minElemSize: 1)
        for _ in 0..<count { try skip(&r, element) }
    case .option(let inner):
        switch try r.readU8() {
        case 0: break
        case 1: try skip(&r, inner)
        case let b: throw CompactError.decode(.invalidBool(b))
        }
    case .enumeration(let arms):
        let wireIndex = try r.readU32()
        guard let arm = arms.first(where: { $0.wireIndex == wireIndex }) else {
            throw CompactError.decode(.malformed("enum variant index out of range"))
        }
        for f in arm.fields { try skip(&r, f) }
    case .map(let key, let value):
        let count = try r.readLen(minElemSize: 1)
        for _ in 0..<count { try skip(&r, key); try skip(&r, value) }
    case .structure(let fields):
        for f in fields { try skip(&r, f) }
    case .dynamic:
        _ = try readValue(&r)
    }
}

/// Build the skip skeleton for a writer schema reference.
func skipOp(_ writer: SchemaRef, _ reg: Registry) throws -> SkipOp {
    switch try resolve(reg, writer) {
    case .primitive(let p):
        switch p {
        case .string, .bytes:
            return .bytes(stride: 1, elemAlign: 1)
        default:
            guard let size = fixedSize(p) else {
                throw CompactError.unsupported("skip: variable-length scalar (datetime/uuid/qname)")
            }
            return .scalar(size: size, align: alignment(p))
        }
    case .composite(let kind):
        switch kind {
        case .structure(_, let fields):
            return .structure(try fields.map { try skipOp($0.schema, reg) })
        case .tuple(let elements):
            return .structure(try elements.map { try skipOp($0, reg) })
        case .enumeration(_, let variants):
            var arms: [(wireIndex: UInt32, fields: [SkipOp])] = []
            for v in variants {
                let fields: [SkipOp]
                switch v.payload {
                case .unit: fields = []
                case .newtype(let r): fields = [try skipOp(r, reg)]
                case .tuple(let rs): fields = try rs.map { try skipOp($0, reg) }
                case .structure(let fs): fields = try fs.map { try skipOp($0.schema, reg) }
                }
                arms.append((wireIndex: v.index, fields: fields))
            }
            return .enumeration(arms)
        case .list(let element), .set(let element):
            // Bulk byte run when the element is a fixed scalar covering its size.
            if case .primitive(let ep) = try resolve(reg, element),
               ep != .string, ep != .bytes,
               let size = fixedSize(ep), size % alignment(ep) == 0 {
                return .bytes(stride: size, elemAlign: alignment(ep))
            }
            return .seq(try skipOp(element, reg))
        case .option(let element):
            return .option(try skipOp(element, reg))
        case .map(let key, let value):
            return .map(try skipOp(key, reg), try skipOp(value, reg))
        case .dynamic:
            return .dynamic
        case .array, .tensor, .channel, .external:
            throw CompactError.unsupported("skip: array/tensor/channel/external")
        case .primitive:
            throw CompactError.malformed("skip: primitive in composite position")
        }
    }
}
