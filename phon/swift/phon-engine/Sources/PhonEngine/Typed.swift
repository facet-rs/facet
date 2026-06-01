// The typed path: lower a `Descriptor` (which carries its schema) into a flat
// `MemProgram`, then run it to encode or decode a value living in this process's
// memory.
//
// This is the memory counterpart to the dynamic `Value` path. The schema
// (resolved through the registry) decides the wire bytes and their order; the
// descriptor says where each field lives in memory. Because the wire is
// schema-driven, a typed value produces byte-identical output to the dynamic
// codec for the same logical value — that equivalence is the oracle the tests
// check.
//
// Mirrors `rust/phon-engine/src/typed.rs`. First cut: fixed-width scalars and
// in-place records (struct/tuple). A nested fixed struct dissolves into a flat
// run of scalar copies — folded, base-relative offsets, no per-decode descriptor
// walk. Owned sequences, options, enums, and maps come next.

import PhonIR
import PhonSchema

/// The in-memory (and wire) size of a fixed-width scalar, or `nil` for the
/// variable-length and uninhabited primitives.
func fixedSize(_ p: Primitive) -> Int? {
    switch p {
    case .unit: return 0
    case .bool, .u8, .i8: return 1
    case .u16, .i16: return 2
    case .u32, .i32, .f32, .char: return 4
    case .u64, .i64, .f64: return 8
    case .u128, .i128: return 16
    case .string, .bytes, .never, .datetime, .uuid, .qname: return nil
    }
}

/// The minimum wire bytes one owned-sequence element occupies, for the
/// length-vs-remaining guard. `0` when the element is zero-sized (all-ZST), else
/// `1`. An empty program is vacuously zero-sized.
func elemMinWire(_ element: MemProgram) -> Int {
    let zeroSized = element.allSatisfy {
        if case .scalar(_, 0, _) = $0 { return true }
        return false
    }
    return zeroSized ? 0 : 1
}

// MARK: - Lowering

/// Lower a descriptor into a flat `MemProgram`: base-relative memory copies, in
/// wire order. Build it once, run it many times. The program is wire-ordered with
/// memory offsets, so the same program drives both encode and decode in the
/// no-drift case.
public func lowerTyped(_ descriptor: Descriptor, _ reg: Registry) throws -> MemProgram {
    var out: MemProgram = []
    try lowerTypedNode(descriptor, reg, 0, &out)
    return out
}

private func lowerTypedNode(_ d: Descriptor, _ reg: Registry, _ base: Int, _ out: inout MemProgram) throws {
    let resolved = try resolve(reg, d.schema)
    switch (d.access, resolved) {
    case (.scalar, .primitive(let p)):
        guard let size = fixedSize(p) else {
            throw CompactError.unsupported("typed: variable-length scalar field")
        }
        out.append(.scalar(offset: base, size: size, align: alignment(p)))
    case (.record(let ra), .composite(let kind)):
        let arity: Int
        switch kind {
        case .structure(_, let fields): arity = fields.count
        case .tuple(let elements): arity = elements.count
        default:
            throw CompactError.typeMismatch(expected: "struct or tuple for a record descriptor")
        }
        guard arity == ra.fields.count else {
            throw CompactError.malformed("descriptor/schema field count mismatch")
        }
        switch ra.construct {
        case .inPlace: break
        case .thunk: throw CompactError.unsupported("typed: thunk construction")
        }
        // Splice each field in wire order, folding its memory offset into the base.
        for fa in ra.fields {
            try lowerTypedNode(fa.descriptor, reg, base + fa.offset, &out)
        }
    case (.option(let oa), .composite(.option)):
        // The inner runs at its own value (base 0).
        var some: MemProgram = []
        try lowerTypedNode(oa.some, reg, 0, &some)
        out.append(.option(OptionOp(
            offset: base,
            some: some,
            innerSize: oa.some.layout.size,
            innerAlign: oa.some.layout.align,
            witness: oa.witness
        )))
    case (.dynamic, .composite(.dynamic)):
        out.append(.dynamic(offset: base))
    case (.bytes(let ba), let resolved):
        switch resolved {
        case .primitive(.string), .primitive(.bytes),
             .composite(.list), .composite(.set):
            break
        default:
            throw CompactError.unsupported("typed: bulk-bytes descriptor over a non-bulk schema")
        }
        out.append(.bytes(BytesOp(
            offset: base, stride: ba.stride, elemAlign: ba.elemAlign, witness: ba.witness)))
    case (.enumeration(let ea), .composite(.enumeration(_, let schemaVariants))):
        var variantOps: [EnumVariantOp] = []
        for va in ea.variants {
            guard schemaVariants.contains(where: { $0.index == va.wireIndex }) else {
                throw CompactError.malformed("descriptor variant has no schema counterpart")
            }
            // The variant's payload fields lay out a record in the variant scratch.
            var payload: MemProgram = []
            for fa in va.payloadFields {
                try lowerTypedNode(fa.descriptor, reg, fa.offset, &payload)
            }
            variantOps.append(EnumVariantOp(
                wireIndex: va.wireIndex,
                payload: payload,
                payloadSize: va.payloadLayout.size,
                payloadAlign: va.payloadLayout.align
            ))
        }
        out.append(.enumeration(EnumOp(
            offset: base,
            tag: ea.tag,
            projectPayload: ea.projectPayload,
            inject: ea.inject,
            variants: variantOps
        )))
    case (.sequence(let sa), .composite(.list)), (.sequence(let sa), .composite(.set)):
        // The element runs at its own slot (base 0).
        var element: MemProgram = []
        try lowerTypedNode(sa.element, reg, 0, &element)
        out.append(.sequence(SeqOp(
            offset: base,
            element: element,
            stride: sa.stride,
            elemAlign: sa.elemAlign,
            minWire: elemMinWire(element),
            witness: sa.witness
        )))
    default:
        throw CompactError.unsupported("typed: unhandled descriptor/schema combination")
    }
}

// MARK: - Encode

/// Encode the value at `base` (described by `program`) to compact bytes.
public func encodeWith(_ program: MemProgram, _ base: UnsafeRawPointer) -> [UInt8] {
    var out = ByteSink()
    encodeTypedProgram(program, base, &out)
    return out.bytes
}

private func encodeTypedProgram(_ program: MemProgram, _ base: UnsafeRawPointer, _ out: inout ByteSink) {
    for op in program {
        switch op {
        case .scalar(let offset, let size, let align):
            out.padTo(align)
            guard size > 0 else { continue }
            out.put(UnsafeRawBufferPointer(start: base.advanced(by: offset), count: size))
        case .option(let o):
            let option = base.advanced(by: o.offset)
            let scratch = UnsafeMutableRawPointer.allocate(
                byteCount: max(o.innerSize, 1), alignment: o.innerAlign)
            defer { scratch.deallocate() }
            if o.witness.projectSome(option, scratch) {
                out.writeU8(1)
                encodeTypedProgram(o.some, UnsafeRawPointer(scratch), &out)
            } else {
                out.writeU8(0)
            }
        case .dynamic(let offset):
            let v = base.advanced(by: offset).assumingMemoryBound(to: Value.self).pointee
            writeValue(&out, v)
        case .bytes(let b):
            let field = base.advanced(by: b.offset)
            let n = b.witness.count(field)
            out.writeU32(UInt32(n))
            guard n > 0 else { continue }
            // Alignment pads BEFORE the run; an empty run writes no padding.
            out.padTo(b.elemAlign)
            let byteCount = n * b.stride
            let scratch = UnsafeMutableRawPointer.allocate(byteCount: byteCount, alignment: max(b.elemAlign, 1))
            defer { scratch.deallocate() }
            b.witness.copyInto(field, scratch)
            out.put(UnsafeRawBufferPointer(start: scratch, count: byteCount))
        case .enumeration(let e):
            let value = base.advanced(by: e.offset)
            let localIndex = e.tag(value)
            let variant = e.variants[localIndex]
            out.writeU32(variant.wireIndex)
            let scratch = UnsafeMutableRawPointer.allocate(
                byteCount: max(variant.payloadSize, 1), alignment: max(variant.payloadAlign, 1))
            defer { scratch.deallocate() }
            e.projectPayload(value, localIndex, scratch)
            encodeTypedProgram(variant.payload, UnsafeRawPointer(scratch), &out)
        case .sequence(let s):
            let handle = base.advanced(by: s.offset)
            let n = s.witness.count(handle)
            out.writeU32(UInt32(n))
            guard n > 0 else { continue }
            let buf = UnsafeMutableRawPointer.allocate(
                byteCount: n * s.stride, alignment: max(s.elemAlign, 1))
            defer { buf.deallocate() }
            s.witness.copyElements(handle, buf)
            for i in 0..<n {
                encodeTypedProgram(s.element, UnsafeRawPointer(buf).advanced(by: i * s.stride), &out)
            }
        }
    }
}

// MARK: - Decode

/// Decode compact `bytes` (described by `program`) into the value-shaped storage
/// at `base`, rejecting trailing bytes. `base` must point at uninitialized
/// storage of the value's layout; on success every field has been written.
public func decodeInto(_ program: MemProgram, _ bytes: [UInt8], _ base: UnsafeMutableRawPointer) throws {
    var r = Reader(bytes)
    try decodeTypedProgram(program, &r, base)
    if r.remaining != 0 {
        throw CompactError.decode(.trailingBytes(r.remaining))
    }
}

private func decodeTypedProgram(_ program: MemProgram, _ r: inout Reader, _ base: UnsafeMutableRawPointer) throws {
    for op in program {
        switch op {
        case .scalar(let offset, let size, let align):
            try r.skipPad(align)
            guard size > 0 else { continue }
            let slice = try r.readSlice(size)
            let dst = base.advanced(by: offset)
            slice.withUnsafeBytes { buf in
                dst.copyMemory(from: buf.baseAddress!, byteCount: size)
            }
        case .option(let o):
            let option = base.advanced(by: o.offset)
            switch try r.readU8() {
            case 0:
                o.witness.initNone(option)
            case 1:
                let scratch = UnsafeMutableRawPointer.allocate(
                    byteCount: max(o.innerSize, 1), alignment: o.innerAlign)
                defer { scratch.deallocate() }
                try decodeTypedProgram(o.some, &r, scratch)
                o.witness.initSome(option, UnsafeRawPointer(scratch))
            case let b:
                throw CompactError.decode(.invalidBool(b))
            }
        case .dynamic(let offset):
            let v = try readValue(&r)
            base.advanced(by: offset).assumingMemoryBound(to: Value.self).initialize(to: v)
        case .bytes(let b):
            let field = base.advanced(by: b.offset)
            let n = try r.readLen(minElemSize: max(b.stride, 1))
            if n > 0 {
                try r.skipPad(b.elemAlign)
                let slice = try r.readSlice(n * b.stride)
                let ok = slice.withUnsafeBytes { buf in
                    b.witness.construct(field, buf.baseAddress!, n)
                }
                guard ok else { throw CompactError.decode(.invalidUtf8) }
            } else {
                var dummy: UInt8 = 0
                let ok = withUnsafePointer(to: &dummy) {
                    b.witness.construct(field, UnsafeRawPointer($0), 0)
                }
                guard ok else { throw CompactError.decode(.invalidUtf8) }
            }
        case .enumeration(let e):
            let slot = base.advanced(by: e.offset)
            let wireIndex = try r.readU32()
            guard let localIndex = e.variants.firstIndex(where: { $0.wireIndex == wireIndex }) else {
                throw CompactError.badVariantIndex(wireIndex)
            }
            let variant = e.variants[localIndex]
            let scratch = UnsafeMutableRawPointer.allocate(
                byteCount: max(variant.payloadSize, 1), alignment: max(variant.payloadAlign, 1))
            defer { scratch.deallocate() }
            try decodeTypedProgram(variant.payload, &r, scratch)
            e.inject(slot, localIndex, UnsafeRawPointer(scratch))
        case .sequence(let s):
            let handle = base.advanced(by: s.offset)
            let n = try r.readLen(minElemSize: s.minWire)
            let buf = UnsafeMutableRawPointer.allocate(
                byteCount: max(n * s.stride, 1), alignment: max(s.elemAlign, 1))
            defer { buf.deallocate() }
            for i in 0..<n {
                try decodeTypedProgram(s.element, &r, buf.advanced(by: i * s.stride))
            }
            s.witness.construct(handle, buf, n)
        }
    }
}
