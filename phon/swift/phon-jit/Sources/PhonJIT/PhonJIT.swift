import CPhonJITStencils
import Darwin
import PhonEngine
import PhonIR
import PhonSchema

// r[impl crates.concern-separation]
// r[impl crates.engine-is-binding-free]
// r[impl ir.stencils]
public enum PhonJITError: Error, CustomStringConvertible, Sendable {
    case emptyStencil
    case executableMemoryUnavailable(errno: Int32)
    case branchOutOfRange

    // r[impl ir.stencils]
    public var description: String {
        switch self {
        case .emptyStencil:
            return "empty JIT stencil"
        case .executableMemoryUnavailable(let errno):
            return "could not allocate executable JIT memory (errno \(errno))"
        case .branchOutOfRange:
            return "JIT branch target is out of range"
        }
    }
}

// r[impl exec.jit-optional]
// r[impl ir.stencils]
public struct JITEngine: TypedEngine {
    public let name = "jit"

    // r[impl exec.jit-optional]
    // r[impl ir.stencils]
    public init() {}

    // r[impl exec.jit-optional]
    // r[impl ir.stencils]
    public func compileEncode(_ lowered: Lowered) throws -> TypedEncodeFn {
        if let native = try NativeEncode.compile(lowered) {
            return { base in native.run(base) }
        }
        return { base in encodeWith(lowered, base) }
    }

    // r[impl exec.jit-optional]
    // r[impl ir.stencils]
    public func compileDecode(_ lowered: Lowered) throws -> TypedDecodeFn {
        if let native = try NativeDecode.compile(lowered) {
            return { bytes, out in try native.run(bytes, out) }
        }
        return { bytes, out in try decodeInto(lowered, bytes, out) }
    }
}

// r[impl crates.jit-opt-in]
// r[impl ir.stencils]
// r[impl exec.jit-optional]
public enum PhonJIT {
    // r[impl exec.jit-optional]
    // r[impl ir.stencils]
    public static func smoke(_ x: Int64) throws -> Int64 {
        let stencil = try smokeStencil()
        let buffer = try ExecutableBuffer(copying: stencil)
        typealias SmokeFn = @convention(c) (Int64) -> Int64
        let fn = unsafeBitCast(buffer.entry, to: SmokeFn.self)
        return fn(x)
    }

    // r[impl exec.strict-recording]
    public static func nativeFallbackReport(_ lowered: Lowered) -> JitFallbackReport {
        var decode: [JitFallbackRecord] = []
        var encode: [JitFallbackRecord] = []

        recordNativeFallbacks(lowered.program, path: "$", mode: .decode, into: &decode)
        recordNativeFallbacks(lowered.program, path: "$", mode: .encode, into: &encode)

        for (schema, block) in lowered.blocks {
            let path = "$block[\(schema)]"
            recordNativeFallbacks(block, path: path, mode: .decode, into: &decode)
            recordNativeFallbacks(block, path: path, mode: .encode, into: &encode)
        }

        return JitFallbackReport(decode: decode, encode: encode)
    }

    // r[impl ir.stencils]
    private static func smokeStencil() throws -> UnsafeRawBufferPointer {
        let start = UnsafeRawPointer(phon_jit_smoke_bytes())
        let count = phon_jit_smoke_len()
        guard count > 0 else {
            throw PhonJITError.emptyStencil
        }
        return UnsafeRawBufferPointer(start: start, count: count)
    }
}

// r[impl ir.stencils]
private struct NativeStencil {
    var bytes: UnsafeRawBufferPointer
    var branchOffset: Int?
}

// r[impl ir.stencils]
private enum NativeMode {
    case decode
    case encode
}

// r[impl exec.strict-recording]
private enum NativeFallbackMode {
    case decode
    case encode
}

// r[impl exec.strict-recording]
private func recordNativeFallbacks(
    _ program: MemProgram,
    path: String,
    mode: NativeFallbackMode,
    into records: inout [JitFallbackRecord]
) {
    for (index, op) in program.enumerated() {
        let opPath = "\(path).\(index)"
        switch op {
        case .scalar(let offset, let size, let align):
            if offset < 0 || size < 0 || align <= 0 || (align & (align - 1)) != 0 {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native JIT rejects invalid scalar layout"
                ))
            }
        case .option(let option):
            if option.offset < 0 {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native JIT rejects invalid option offset"
                ))
            }
            recordNativeFallbacks(option.some, path: "\(opPath).some", mode: mode, into: &records)
        case .dynamic:
            break
        case .bytes(let bytes):
            if bytes.offset < 0 || bytes.stride <= 0 || bytes.elemAlign <= 0
                || (bytes.elemAlign & (bytes.elemAlign - 1)) != 0
            {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native JIT rejects invalid bytes layout"
                ))
            }
        case .enumeration(let enumeration):
            // r[impl compat.enum]
            if mode == .encode && !enumeration.writerOnly.isEmpty {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native encode JIT cannot emit decode-only writer-only enum variants"
                ))
            }
            for variant in enumeration.variants {
                recordNativeFallbacks(
                    variant.payload,
                    path: "\(opPath).variant[\(variant.wireIndex)]",
                    mode: mode,
                    into: &records
                )
            }
        case .sequence(let sequence):
            if sequence.offset < 0 || sequence.stride <= 0 || sequence.elemAlign <= 0
                || (sequence.elemAlign & (sequence.elemAlign - 1)) != 0
            {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native JIT rejects invalid sequence layout"
                ))
            }
            recordNativeFallbacks(sequence.element, path: "\(opPath).element", mode: mode, into: &records)
        case .map(let map):
            if map.offset < 0
                || map.keyStride <= 0 || map.keyAlign <= 0 || (map.keyAlign & (map.keyAlign - 1)) != 0
                || map.valueStride <= 0 || map.valueAlign <= 0 || (map.valueAlign & (map.valueAlign - 1)) != 0
            {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native JIT rejects invalid map layout"
                ))
            }
            recordNativeFallbacks(map.key, path: "\(opPath).key", mode: mode, into: &records)
            recordNativeFallbacks(map.value, path: "\(opPath).value", mode: mode, into: &records)
        case .skipWire:
            // r[impl compat.skip-writer-only]
            let reason: String? = switch mode {
            case .decode:
                nil
            case .encode:
                "Swift native encode JIT cannot emit decode-only skip-wire ops"
            }
            if let reason {
                records.append(JitFallbackRecord(path: opPath, reason: reason))
            }
        case .writeDefault:
            // r[impl compat.reader-only-fields]
            // r[impl compat.defaults-are-reader-side]
            let reason: String? = switch mode {
            case .decode:
                nil
            case .encode:
                "Swift native encode JIT cannot emit decode-only default ops"
            }
            if let reason {
                records.append(JitFallbackRecord(path: opPath, reason: reason))
            }
        case .callBlock(_, let offset):
            if offset < 0 {
                records.append(JitFallbackRecord(
                    path: opPath,
                    reason: "Swift native JIT rejects invalid recursive call offset"
                ))
            }
        }
    }
}

// r[impl ir.stencils]
private final class WordBuffer {
    let pointer: UnsafePointer<UInt64>

    // r[impl ir.stencils]
    init(_ words: [UInt64]) {
        let count = max(words.count, 1)
        let storage = UnsafeMutablePointer<UInt64>.allocate(capacity: count)
        storage.initialize(repeating: 0, count: count)
        for (index, word) in words.enumerated() {
            storage.advanced(by: index).pointee = word
        }
        self.storage = storage
        pointer = UnsafePointer(storage)
        allocatedCount = count
    }

    private let storage: UnsafeMutablePointer<UInt64>
    private let allocatedCount: Int

    // r[impl ir.stencils]
    deinit {
        storage.deinitialize(count: allocatedCount)
        storage.deallocate()
    }
}

// r[impl ir.stencils]
private final class OptionWitnessBox {
    let witness: OptionWitness

    // r[impl ir.stencils]
    init(_ witness: OptionWitness) {
        self.witness = witness
    }
}

// r[impl ir.stencils]
private final class BytesWitnessBox {
    let witness: BytesWitness

    // r[impl ir.stencils]
    init(_ witness: BytesWitness) {
        self.witness = witness
    }
}

// r[impl ir.stencils]
private final class BytesInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITBytesInfo>
    let fieldOffset: Int
    let stride: Int
    let witnessBox: BytesWitnessBox

    // r[impl ir.stencils]
    init(
        fieldOffset: Int,
        stride: Int,
        elemAlign: Int,
        witnessBox: BytesWitnessBox
    ) {
        self.fieldOffset = fieldOffset
        self.stride = stride
        self.witnessBox = witnessBox
        pointer = UnsafeMutablePointer<PhonJITBytesInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITBytesInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.stride = UInt64(stride)
        pointer.pointee.elem_align = UInt64(elemAlign)
        pointer.pointee.witness_ctx = pointerUInt(Unmanaged.passUnretained(witnessBox).toOpaque())
        pointer.pointee.count = phon_jit_bytes_count_ptr()
        pointer.pointee.copy_into = phon_jit_bytes_copy_into_ptr()
        pointer.pointee.construct = phon_jit_bytes_construct_ptr()
        pointer.pointee.decode = phon_jit_bytes_decode_ptr()
        pointer.pointee.encode = phon_jit_bytes_encode_ptr()
    }

    // r[impl ir.stencils]
    func dynamicByteCount(base: UnsafeRawPointer) -> Int {
        let field = base.advanced(by: fieldOffset)
        return witnessBox.witness.count(field) * stride
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class SeqWitnessBox {
    let witness: SeqWitness

    // r[impl ir.stencils]
    init(_ witness: SeqWitness) {
        self.witness = witness
    }
}

// r[impl ir.stencils]
private final class SeqInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITSeqInfo>
    let fieldOffset: Int
    let elementMaxWireSize: Int
    private let witnessBox: SeqWitnessBox

    // r[impl ir.stencils]
    init(
        fieldOffset: Int,
        stride: Int,
        elemAlign: Int,
        minWire: Int,
        unique: Bool,
        witnessBox: SeqWitnessBox,
        element: NativeProgram
    ) {
        self.fieldOffset = fieldOffset
        self.elementMaxWireSize = element.maxWireSize
        self.witnessBox = witnessBox
        pointer = UnsafeMutablePointer<PhonJITSeqInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITSeqInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.stride = UInt64(stride)
        pointer.pointee.elem_align = UInt64(elemAlign)
        pointer.pointee.min_wire = UInt64(minWire)
        pointer.pointee.unique = unique ? 1 : 0
        pointer.pointee.witness_ctx = pointerUInt(Unmanaged.passUnretained(witnessBox).toOpaque())
        pointer.pointee.count = phon_jit_seq_count_ptr()
        pointer.pointee.copy_elements = phon_jit_seq_copy_elements_ptr()
        pointer.pointee.destroy_elements = phon_jit_seq_destroy_elements_ptr()
        pointer.pointee.construct = phon_jit_seq_construct_ptr()
        pointer.pointee.element_entry = pointerUInt(element.code.entry)
        pointer.pointee.element_prog = pointerUInt(element.words.pointer)
        pointer.pointee.decode = phon_jit_seq_decode_ptr()
        pointer.pointee.encode = phon_jit_seq_encode_ptr()
    }

    // r[impl ir.stencils]
    func dynamicWireCount(base: UnsafeRawPointer) -> Int {
        let field = base.advanced(by: fieldOffset)
        return witnessBox.witness.count(field) * elementMaxWireSize
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class MapWitnessBox {
    let witness: MapWitness

    // r[impl ir.stencils]
    init(_ witness: MapWitness) {
        self.witness = witness
    }
}

// r[impl ir.stencils]
private final class DynamicInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITDynamicInfo>
    let fieldOffset: Int

    // r[impl ir.stencils]
    init(fieldOffset: Int) {
        self.fieldOffset = fieldOffset
        pointer = UnsafeMutablePointer<PhonJITDynamicInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITDynamicInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.decode = phon_jit_dynamic_decode_ptr()
        pointer.pointee.encode = phon_jit_dynamic_encode_ptr()
    }

    // r[impl ir.stencils]
    func dynamicByteCount(base: UnsafeRawPointer) -> Int {
        let field = base.advanced(by: fieldOffset)
        let value = field.assumingMemoryBound(to: Value.self).pointee
        return valueToBytes(value).count
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class SkipWireBox {
    let op: SkipOp

    // r[impl ir.stencils]
    init(_ op: SkipOp) {
        self.op = op
    }
}

// r[impl ir.stencils]
private final class SkipWireInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITSkipWireInfo>
    let box: SkipWireBox

    // r[impl ir.stencils]
    init(_ op: SkipOp) {
        box = SkipWireBox(op)
        pointer = UnsafeMutablePointer<PhonJITSkipWireInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITSkipWireInfo())
        pointer.pointee.ctx = pointerUInt(Unmanaged.passUnretained(box).toOpaque())
        pointer.pointee.decode = phon_jit_skipwire_decode_ptr()
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class DefaultBox {
    let op: DefaultOp

    // r[impl ir.stencils]
    init(_ op: DefaultOp) {
        self.op = op
    }
}

// r[impl ir.stencils]
private final class DefaultInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITDefaultInfo>
    let box: DefaultBox

    // r[impl ir.stencils]
    init(_ op: DefaultOp) {
        box = DefaultBox(op)
        pointer = UnsafeMutablePointer<PhonJITDefaultInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITDefaultInfo())
        pointer.pointee.ctx = pointerUInt(Unmanaged.passUnretained(box).toOpaque())
        pointer.pointee.decode = phon_jit_default_decode_ptr()
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class MapInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITMapInfo>
    let fieldOffset: Int
    let keyMaxWireSize: Int
    let valueMaxWireSize: Int
    private let witnessBox: MapWitnessBox

    // r[impl ir.stencils]
    init(
        fieldOffset: Int,
        keyStride: Int,
        keyAlign: Int,
        valueStride: Int,
        valueAlign: Int,
        witnessBox: MapWitnessBox,
        key: NativeProgram,
        value: NativeProgram
    ) {
        self.fieldOffset = fieldOffset
        self.keyMaxWireSize = key.maxWireSize
        self.valueMaxWireSize = value.maxWireSize
        self.witnessBox = witnessBox
        pointer = UnsafeMutablePointer<PhonJITMapInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITMapInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.key_stride = UInt64(keyStride)
        pointer.pointee.key_align = UInt64(keyAlign)
        pointer.pointee.value_stride = UInt64(valueStride)
        pointer.pointee.value_align = UInt64(valueAlign)
        pointer.pointee.witness_ctx = pointerUInt(Unmanaged.passUnretained(witnessBox).toOpaque())
        pointer.pointee.count = phon_jit_map_count_ptr()
        pointer.pointee.project_entries = phon_jit_map_project_entries_ptr()
        pointer.pointee.destroy_entries = phon_jit_map_destroy_entries_ptr()
        pointer.pointee.init_with_capacity = phon_jit_map_init_with_capacity_ptr()
        pointer.pointee.insert = phon_jit_map_insert_ptr()
        pointer.pointee.key_entry = pointerUInt(key.code.entry)
        pointer.pointee.key_prog = pointerUInt(key.words.pointer)
        pointer.pointee.value_entry = pointerUInt(value.code.entry)
        pointer.pointee.value_prog = pointerUInt(value.words.pointer)
        pointer.pointee.decode = phon_jit_map_decode_ptr()
        pointer.pointee.encode = phon_jit_map_encode_ptr()
    }

    // r[impl ir.stencils]
    func dynamicWireCount(base: UnsafeRawPointer) -> Int {
        let field = base.advanced(by: fieldOffset)
        return witnessBox.witness.count(field) * (keyMaxWireSize + valueMaxWireSize)
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class BlockCallInfoAllocation {
    let schema: SchemaId
    let pointer: UnsafeMutablePointer<PhonJITBlockInfo>

    // r[impl ir.stencils]
    init(schema: SchemaId, fieldOffset: Int) {
        self.schema = schema
        pointer = UnsafeMutablePointer<PhonJITBlockInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITBlockInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.entry = 0
        pointer.pointee.prog = 0
        pointer.pointee.scratch_size = 0
        pointer.pointee.scratch_align = 1
        pointer.pointee.decode = phon_jit_block_decode_ptr()
        pointer.pointee.encode = phon_jit_block_encode_ptr()
    }

    // r[impl ir.stencils]
    func bind(to program: NativeProgram, scratch: ScratchAllocator) {
        pointer.pointee.entry = pointerUInt(program.code.entry)
        pointer.pointee.prog = pointerUInt(program.words.pointer)
        pointer.pointee.scratch_size = UInt64(max(scratch.size, 1))
        pointer.pointee.scratch_align = UInt64(max(scratch.alignment, 1))
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class BlockCallTable {
    private var infosBySchema: [SchemaId: [BlockCallInfoAllocation]] = [:]

    // r[impl ir.stencils]
    func add(schema: SchemaId, fieldOffset: Int) -> BlockCallInfoAllocation {
        let info = BlockCallInfoAllocation(schema: schema, fieldOffset: fieldOffset)
        infosBySchema[schema, default: []].append(info)
        return info
    }

    // r[impl ir.stencils]
    func bind(programs: [SchemaId: NativeProgram], scratch: ScratchAllocator) -> Bool {
        for (schema, infos) in infosBySchema {
            guard let program = programs[schema] else {
                return false
            }
            for info in infos {
                info.bind(to: program, scratch: scratch)
            }
        }
        return true
    }

    // r[impl ir.stencils]
    var allInfos: [BlockCallInfoAllocation] {
        infosBySchema.values.flatMap { $0 }
    }
}

// r[impl ir.stencils]
private final class EnumWitnessBox {
    let op: EnumOp

    // r[impl ir.stencils]
    init(_ op: EnumOp) {
        self.op = op
    }

    // r[impl ir.stencils]
    func tagWireIndex(_ field: UnsafeRawPointer) -> UInt32? {
        let localIndex = op.tag(field)
        guard op.variants.indices.contains(localIndex) else {
            return nil
        }
        return UInt32(localIndex)
    }

    // r[impl ir.stencils]
    func project(_ field: UnsafeRawPointer, localIndex: UInt32, scratch: UnsafeMutableRawPointer) {
        op.projectPayload(field, Int(localIndex), scratch)
    }

    // r[impl ir.stencils]
    func destroy(_ scratch: UnsafeMutableRawPointer, localIndex: UInt32) {
        op.destroyPayload(scratch, Int(localIndex))
    }

    // r[impl ir.stencils]
    func inject(_ field: UnsafeMutableRawPointer, localIndex: UInt32, scratch: UnsafeMutableRawPointer) {
        op.inject(field, Int(localIndex), scratch)
    }
}

// r[impl ir.stencils]
private final class EnumInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITEnumInfo>
    private let variantsPointer: UnsafeMutablePointer<PhonJITEnumVariantInfo>
    private let writerOnlyPointer: UnsafeMutablePointer<UInt32>
    private let variantCount: Int
    private let writerOnlyCount: Int

    // r[impl ir.stencils]
    init(
        fieldOffset: Int,
        witnessBox: EnumWitnessBox,
        variants: [PhonJITEnumVariantInfo],
        writerOnly: [UInt32]
    ) {
        variantCount = variants.count
        writerOnlyCount = writerOnly.count
        variantsPointer = UnsafeMutablePointer<PhonJITEnumVariantInfo>.allocate(
            capacity: max(variantCount, 1)
        )
        variantsPointer.initialize(repeating: PhonJITEnumVariantInfo(), count: max(variantCount, 1))
        for (index, variant) in variants.enumerated() {
            variantsPointer.advanced(by: index).pointee = variant
        }

        writerOnlyPointer = UnsafeMutablePointer<UInt32>.allocate(capacity: max(writerOnlyCount, 1))
        writerOnlyPointer.initialize(repeating: 0, count: max(writerOnlyCount, 1))
        for (index, wireIndex) in writerOnly.enumerated() {
            writerOnlyPointer.advanced(by: index).pointee = wireIndex
        }

        pointer = UnsafeMutablePointer<PhonJITEnumInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITEnumInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.variants = UnsafeRawPointer(variantsPointer)
        pointer.pointee.variant_count = UInt64(variantCount)
        pointer.pointee.writer_only = UnsafePointer(writerOnlyPointer)
        pointer.pointee.writer_only_count = UInt64(writerOnlyCount)
        pointer.pointee.witness_ctx = pointerUInt(Unmanaged.passUnretained(witnessBox).toOpaque())
        pointer.pointee.tag = phon_jit_enum_tag_ptr()
        pointer.pointee.project = phon_jit_enum_project_ptr()
        pointer.pointee.destroy = phon_jit_enum_destroy_ptr()
        pointer.pointee.inject = phon_jit_enum_inject_ptr()
        pointer.pointee.decode = phon_jit_enum_decode_ptr()
        pointer.pointee.encode = phon_jit_enum_encode_ptr()
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
        variantsPointer.deinitialize(count: max(variantCount, 1))
        variantsPointer.deallocate()
        writerOnlyPointer.deinitialize(count: max(writerOnlyCount, 1))
        writerOnlyPointer.deallocate()
    }
}

// r[impl ir.stencils]
private final class OptionInfoAllocation {
    let pointer: UnsafeMutablePointer<PhonJITOptionInfo>

    // r[impl ir.stencils]
    init(
        fieldOffset: Int,
        scratchOffset: Int,
        child: NativeProgram,
        witnessBox: OptionWitnessBox
    ) {
        pointer = UnsafeMutablePointer<PhonJITOptionInfo>.allocate(capacity: 1)
        pointer.initialize(to: PhonJITOptionInfo())
        pointer.pointee.field_offset = UInt64(fieldOffset)
        pointer.pointee.scratch_offset = UInt64(scratchOffset)
        pointer.pointee.some_entry = pointerUInt(child.code.entry)
        pointer.pointee.some_prog = pointerUInt(child.words.pointer)
        pointer.pointee.witness_ctx = pointerUInt(Unmanaged.passUnretained(witnessBox).toOpaque())
        pointer.pointee.project_some = phon_jit_option_project_some_ptr()
        pointer.pointee.init_some = phon_jit_option_init_some_ptr()
        pointer.pointee.init_none = phon_jit_option_init_none_ptr()
    }

    // r[impl ir.stencils]
    deinit {
        pointer.deinitialize(count: 1)
        pointer.deallocate()
    }
}

// r[impl ir.stencils]
private struct ScratchAllocator {
    var size = 0
    var alignment = 1

    // r[impl ir.stencils]
    mutating func allocate(byteCount: Int, alignment requestedAlignment: Int) -> Int {
        let align = max(requestedAlignment, 1)
        alignment = max(alignment, align)
        size = alignUp(size, align)
        let offset = size
        size += max(byteCount, 1)
        return offset
    }
}

// r[impl ir.stencils]
private final class NativeProgram {
    let code: ExecutableBuffer
    let words: WordBuffer
    let maxWireSize: Int

    private let children: [NativeProgram]
    private let infos: [OptionInfoAllocation]
    private let witnessBoxes: [OptionWitnessBox]
    private let bytesInfos: [BytesInfoAllocation]
    private let bytesWitnessBoxes: [BytesWitnessBox]
    private let seqInfos: [SeqInfoAllocation]
    private let seqWitnessBoxes: [SeqWitnessBox]
    private let dynamicInfos: [DynamicInfoAllocation]
    private let skipWireInfos: [SkipWireInfoAllocation]
    private let defaultInfos: [DefaultInfoAllocation]
    private let mapInfos: [MapInfoAllocation]
    private let mapWitnessBoxes: [MapWitnessBox]
    private let blockCallInfos: [BlockCallInfoAllocation]
    private let enumInfos: [EnumInfoAllocation]
    private let enumWitnessBoxes: [EnumWitnessBox]

    // r[impl ir.stencils]
    static func compile(
        _ program: MemProgram,
        mode: NativeMode,
        scratch: inout ScratchAllocator,
        blockCalls: BlockCallTable? = nil
    ) throws -> NativeProgram? {
        var words: [UInt64] = []
        var stencils: [NativeStencil] = []
        var children: [NativeProgram] = []
        var infos: [OptionInfoAllocation] = []
        var witnessBoxes: [OptionWitnessBox] = []
        var bytesInfos: [BytesInfoAllocation] = []
        var bytesWitnessBoxes: [BytesWitnessBox] = []
        var seqInfos: [SeqInfoAllocation] = []
        var seqWitnessBoxes: [SeqWitnessBox] = []
        var dynamicInfos: [DynamicInfoAllocation] = []
        var skipWireInfos: [SkipWireInfoAllocation] = []
        var defaultInfos: [DefaultInfoAllocation] = []
        var mapInfos: [MapInfoAllocation] = []
        var mapWitnessBoxes: [MapWitnessBox] = []
        var blockCallInfos: [BlockCallInfoAllocation] = []
        var enumInfos: [EnumInfoAllocation] = []
        var enumWitnessBoxes: [EnumWitnessBox] = []
        var maxWireSize = 0

        for op in program {
            switch op {
            case .scalar(let offset, let size, let align):
                guard appendScalar(offset: offset, size: size, align: align, to: &words) else {
                    return nil
                }
                stencils.append(try mode == .decode ? scalarDecodeStencil() : scalarEncodeStencil())
                maxWireSize += max(align - 1, 0) + size
            case .option(let option):
                guard option.offset >= 0 else {
                    return nil
                }
                guard let child = try NativeProgram.compile(option.some, mode: mode, scratch: &scratch, blockCalls: blockCalls) else {
                    return nil
                }
                let scratchOffset = scratch.allocate(
                    byteCount: option.innerSize,
                    alignment: option.innerAlign
                )
                let witnessBox = OptionWitnessBox(option.witness)
                let info = OptionInfoAllocation(
                    fieldOffset: option.offset,
                    scratchOffset: scratchOffset,
                    child: child,
                    witnessBox: witnessBox
                )
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? optionDecodeStencil() : optionEncodeStencil())
                maxWireSize += 1 + child.maxWireSize
                children.append(child)
                infos.append(info)
                witnessBoxes.append(witnessBox)
            case .bytes(let bytes):
                guard bytes.offset >= 0, bytes.stride > 0, bytes.elemAlign > 0,
                      bytes.elemAlign & (bytes.elemAlign - 1) == 0
                else {
                    return nil
                }
                let witnessBox = BytesWitnessBox(bytes.witness)
                let info = BytesInfoAllocation(
                    fieldOffset: bytes.offset,
                    stride: bytes.stride,
                    elemAlign: bytes.elemAlign,
                    witnessBox: witnessBox
                )
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? bytesDecodeStencil() : bytesEncodeStencil())
                maxWireSize += 4 + max(bytes.elemAlign - 1, 0)
                bytesInfos.append(info)
                bytesWitnessBoxes.append(witnessBox)
            case .dynamic(let offset):
                guard offset >= 0 else {
                    return nil
                }
                let info = DynamicInfoAllocation(fieldOffset: offset)
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? dynamicDecodeStencil() : dynamicEncodeStencil())
                dynamicInfos.append(info)
            case .skipWire(let op):
                // r[impl compat.skip-writer-only]
                guard mode == .decode else {
                    return nil
                }
                let info = SkipWireInfoAllocation(op)
                words.append(pointerWord(info.pointer))
                stencils.append(try dynamicDecodeStencil())
                skipWireInfos.append(info)
            case .writeDefault(let op):
                // r[impl compat.reader-only-fields]
                // r[impl compat.defaults-are-reader-side]
                guard mode == .decode, op.offset >= 0 else {
                    return nil
                }
                let info = DefaultInfoAllocation(op)
                words.append(pointerWord(info.pointer))
                stencils.append(try dynamicDecodeStencil())
                defaultInfos.append(info)
            case .sequence(let sequence):
                guard sequence.offset >= 0,
                      sequence.stride > 0,
                      sequence.elemAlign > 0,
                      sequence.elemAlign & (sequence.elemAlign - 1) == 0,
                      sequence.minWire >= 0
                else {
                    return nil
                }
                guard let child = try NativeProgram.compile(sequence.element, mode: mode, scratch: &scratch, blockCalls: blockCalls) else {
                    return nil
                }
                let witnessBox = SeqWitnessBox(sequence.witness)
                let info = SeqInfoAllocation(
                    fieldOffset: sequence.offset,
                    stride: sequence.stride,
                    elemAlign: sequence.elemAlign,
                    minWire: sequence.minWire,
                    unique: sequence.unique,
                    witnessBox: witnessBox,
                    element: child
                )
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? seqDecodeStencil() : seqEncodeStencil())
                maxWireSize += 4 + child.maxWireSize
                children.append(child)
                seqInfos.append(info)
                seqWitnessBoxes.append(witnessBox)
            case .map(let map):
                guard map.offset >= 0,
                      map.keyStride > 0,
                      map.keyAlign > 0,
                      map.keyAlign & (map.keyAlign - 1) == 0,
                      map.valueStride > 0,
                      map.valueAlign > 0,
                      map.valueAlign & (map.valueAlign - 1) == 0
                else {
                    return nil
                }
                guard let key = try NativeProgram.compile(map.key, mode: mode, scratch: &scratch, blockCalls: blockCalls),
                      let value = try NativeProgram.compile(map.value, mode: mode, scratch: &scratch, blockCalls: blockCalls)
                else {
                    return nil
                }
                let witnessBox = MapWitnessBox(map.witness)
                let info = MapInfoAllocation(
                    fieldOffset: map.offset,
                    keyStride: map.keyStride,
                    keyAlign: map.keyAlign,
                    valueStride: map.valueStride,
                    valueAlign: map.valueAlign,
                    witnessBox: witnessBox,
                    key: key,
                    value: value
                )
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? mapDecodeStencil() : mapEncodeStencil())
                maxWireSize += 4 + key.maxWireSize + value.maxWireSize
                children.append(key)
                children.append(value)
                mapInfos.append(info)
                mapWitnessBoxes.append(witnessBox)
            case .enumeration(let enumeration):
                guard enumeration.offset >= 0 else {
                    return nil
                }
                guard mode == .decode || enumeration.writerOnly.isEmpty else {
                    return nil
                }
                var variantInfos: [PhonJITEnumVariantInfo] = []
                var maxPayloadWireSize = 0
                for variant in enumeration.variants {
                    guard variant.readerLocalIndex >= 0,
                          let readerLocalIndex = UInt32(exactly: variant.readerLocalIndex)
                    else {
                        return nil
                    }
                    let scratchOffset = scratch.allocate(
                        byteCount: variant.payloadSize,
                        alignment: variant.payloadAlign
                    )
                    guard let child = try NativeProgram.compile(variant.payload, mode: mode, scratch: &scratch, blockCalls: blockCalls) else {
                        return nil
                    }
                    var variantInfo = PhonJITEnumVariantInfo()
                    variantInfo.wire_index = variant.wireIndex
                    variantInfo.reader_local_index = readerLocalIndex
                    variantInfo.scratch_offset = UInt64(scratchOffset)
                    variantInfo.payload_entry = pointerUInt(child.code.entry)
                    variantInfo.payload_prog = pointerUInt(child.words.pointer)
                    variantInfos.append(variantInfo)
                    maxPayloadWireSize = max(maxPayloadWireSize, child.maxWireSize)
                    children.append(child)
                }
                let witnessBox = EnumWitnessBox(enumeration)
                let info = EnumInfoAllocation(
                    fieldOffset: enumeration.offset,
                    witnessBox: witnessBox,
                    variants: variantInfos,
                    writerOnly: enumeration.writerOnly
                )
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? enumDecodeStencil() : enumEncodeStencil())
                maxWireSize += 4 + maxPayloadWireSize
                enumInfos.append(info)
                enumWitnessBoxes.append(witnessBox)
            case .callBlock(let schema, let offset):
                guard offset >= 0, let blockCalls else {
                    return nil
                }
                let info = blockCalls.add(schema: schema, fieldOffset: offset)
                words.append(pointerWord(info.pointer))
                stencils.append(try mode == .decode ? blockDecodeStencil() : blockEncodeStencil())
                blockCallInfos.append(info)
            }
        }

        return try NativeProgram(
            code: ExecutableBuffer.compileChain(stencils),
            words: WordBuffer(words),
            maxWireSize: maxWireSize,
            children: children,
            infos: infos,
            witnessBoxes: witnessBoxes,
            bytesInfos: bytesInfos,
            bytesWitnessBoxes: bytesWitnessBoxes,
            seqInfos: seqInfos,
            seqWitnessBoxes: seqWitnessBoxes,
            dynamicInfos: dynamicInfos,
            skipWireInfos: skipWireInfos,
            defaultInfos: defaultInfos,
            mapInfos: mapInfos,
            mapWitnessBoxes: mapWitnessBoxes,
            blockCallInfos: blockCallInfos,
            enumInfos: enumInfos,
            enumWitnessBoxes: enumWitnessBoxes
        )
    }

    // r[impl ir.stencils]
    private init(
        code: ExecutableBuffer,
        words: WordBuffer,
        maxWireSize: Int,
        children: [NativeProgram],
        infos: [OptionInfoAllocation],
        witnessBoxes: [OptionWitnessBox],
        bytesInfos: [BytesInfoAllocation],
        bytesWitnessBoxes: [BytesWitnessBox],
        seqInfos: [SeqInfoAllocation],
        seqWitnessBoxes: [SeqWitnessBox],
        dynamicInfos: [DynamicInfoAllocation],
        skipWireInfos: [SkipWireInfoAllocation],
        defaultInfos: [DefaultInfoAllocation],
        mapInfos: [MapInfoAllocation],
        mapWitnessBoxes: [MapWitnessBox],
        blockCallInfos: [BlockCallInfoAllocation],
        enumInfos: [EnumInfoAllocation],
        enumWitnessBoxes: [EnumWitnessBox]
    ) {
        self.code = code
        self.words = words
        self.maxWireSize = maxWireSize
        self.children = children
        self.infos = infos
        self.witnessBoxes = witnessBoxes
        self.bytesInfos = bytesInfos
        self.bytesWitnessBoxes = bytesWitnessBoxes
        self.seqInfos = seqInfos
        self.seqWitnessBoxes = seqWitnessBoxes
        self.dynamicInfos = dynamicInfos
        self.skipWireInfos = skipWireInfos
        self.defaultInfos = defaultInfos
        self.mapInfos = mapInfos
        self.mapWitnessBoxes = mapWitnessBoxes
        self.blockCallInfos = blockCallInfos
        self.enumInfos = enumInfos
        self.enumWitnessBoxes = enumWitnessBoxes
    }

    // r[impl ir.stencils]
    func initialEncodeCapacity(base: UnsafeRawPointer) -> Int {
        let dynamicBytes = bytesInfos.reduce(0) { $0 + $1.dynamicByteCount(base: base) }
        let dynamicValueBytes = dynamicInfos.reduce(0) { $0 + $1.dynamicByteCount(base: base) }
        let dynamicSequenceBytes = seqInfos.reduce(0) { $0 + $1.dynamicWireCount(base: base) }
        let dynamicMapBytes = mapInfos.reduce(0) { $0 + $1.dynamicWireCount(base: base) }
        return max(maxWireSize + dynamicBytes + dynamicValueBytes + dynamicSequenceBytes + dynamicMapBytes, 64)
    }
}

// r[impl ir.stencils]
private struct CompiledNativeLowered {
    var root: NativeProgram
    var blocks: [NativeProgram]
    var scratch: ScratchAllocator
}

// r[impl ir.stencils]
private func compileNativeLowered(_ lowered: Lowered, mode: NativeMode) throws -> CompiledNativeLowered? {
    var scratch = ScratchAllocator()
    let blockCalls = BlockCallTable()
    var blockProgramsBySchema: [SchemaId: NativeProgram] = [:]

    for (schema, block) in lowered.blocks {
        guard let program = try NativeProgram.compile(block, mode: mode, scratch: &scratch, blockCalls: blockCalls) else {
            return nil
        }
        blockProgramsBySchema[schema] = program
    }
    guard let root = try NativeProgram.compile(lowered.program, mode: mode, scratch: &scratch, blockCalls: blockCalls) else {
        return nil
    }
    guard blockCalls.bind(programs: blockProgramsBySchema, scratch: scratch) else {
        return nil
    }

    return CompiledNativeLowered(root: root, blocks: Array(blockProgramsBySchema.values), scratch: scratch)
}

// r[impl ir.stencils]
final class NativeDecode {
    private let program: NativeProgram
    private let blockPrograms: [NativeProgram]
    private let scratchSize: Int
    private let scratchAlignment: Int

    // r[impl ir.stencils]
    static func compile(_ lowered: Lowered) throws -> NativeDecode? {
        guard let compiled = try compileNativeLowered(lowered, mode: .decode) else {
            return nil
        }
        return NativeDecode(program: compiled.root, blockPrograms: compiled.blocks, scratch: compiled.scratch)
    }

    // r[impl ir.stencils]
    private init(program: NativeProgram, blockPrograms: [NativeProgram], scratch: ScratchAllocator) {
        self.program = program
        self.blockPrograms = blockPrograms
        scratchSize = scratch.size
        scratchAlignment = scratch.alignment
    }

    // r[impl ir.stencils]
    func run(_ bytes: [UInt8], _ out: UnsafeMutableRawPointer) throws {
        typealias DecodeFn = @convention(c) (UnsafeMutablePointer<PhonJITDecodeCtx>) -> Void

        var dummyWire: UInt8 = 0
        var status: UInt64 = 0
        var aux: UInt64 = 0
        var remaining = 0
        let fn = unsafeBitCast(program.code.entry, to: DecodeFn.self)

        withScratch(byteCount: scratchSize, alignment: scratchAlignment) { scratch in
            bytes.withUnsafeBytes { raw in
                withUnsafePointer(to: &dummyWire) { dummy in
                    let wire = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) ?? dummy
                    var ctx = PhonJITDecodeCtx(
                        wire: wire,
                        wire_start: wire,
                        wire_end: wire.advanced(by: bytes.count),
                        base: out.assumingMemoryBound(to: UInt8.self),
                        prog: program.words.pointer,
                        status: 0,
                        aux: 0,
                        scratch: scratch.assumingMemoryBound(to: UInt8.self)
                    )
                    withUnsafeMutablePointer(to: &ctx) { fn($0) }
                    status = ctx.status
                    aux = ctx.aux
                    remaining = ctx.wire.distance(to: ctx.wire_end)
                }
            }
        }

        switch status {
        case 0:
            break
        case 1:
            throw CompactError.decode(.unexpectedEof(needed: 1, remaining: 0))
        case 2:
            throw CompactError.decode(.invalidBool(UInt8(truncatingIfNeeded: aux)))
        case 3:
            throw CompactError.decode(.invalidUtf8)
        case 4:
            throw CompactError.badVariantIndex(UInt32(truncatingIfNeeded: aux))
        case 5:
            throw CompactError.decode(.malformed("jit allocation failed"))
        case 6:
            throw CompactError.decode(.duplicateKey)
        case 7:
            throw CompactError.decode(.duplicateElement)
        case 8:
            throw CompactError.writerOnlyVariant(UInt32(truncatingIfNeeded: aux))
        case 9:
            throw CompactError.decode(.malformed("jit skip-wire failed"))
        default:
            throw CompactError.decode(.malformed("jit status \(status)"))
        }
        if remaining != 0 {
            throw CompactError.decode(.trailingBytes(remaining))
        }
    }
}

// r[impl ir.stencils]
final class NativeEncode {
    private let program: NativeProgram
    private let blockPrograms: [NativeProgram]
    private let scratchSize: Int
    private let scratchAlignment: Int

    // r[impl ir.stencils]
    static func compile(_ lowered: Lowered) throws -> NativeEncode? {
        guard let compiled = try compileNativeLowered(lowered, mode: .encode) else {
            return nil
        }
        return NativeEncode(program: compiled.root, blockPrograms: compiled.blocks, scratch: compiled.scratch)
    }

    // r[impl ir.stencils]
    private init(program: NativeProgram, blockPrograms: [NativeProgram], scratch: ScratchAllocator) {
        self.program = program
        self.blockPrograms = blockPrograms
        scratchSize = scratch.size
        scratchAlignment = scratch.alignment
    }

    // r[impl ir.stencils]
    func run(_ base: UnsafeRawPointer) -> [UInt8] {
        typealias EncodeFn = @convention(c) (UnsafeMutablePointer<PhonJITEncodeCtx>) -> Void

        let fn = unsafeBitCast(program.code.entry, to: EncodeFn.self)
        var byteCount = program.initialEncodeCapacity(base: base)

        while true {
            var bytes = [UInt8](repeating: 0, count: byteCount)
            var dummyOut: UInt8 = 0
            var status: UInt64 = 0
            var written = 0

            withScratch(byteCount: scratchSize, alignment: scratchAlignment) { scratch in
                bytes.withUnsafeMutableBytes { raw in
                    withUnsafeMutablePointer(to: &dummyOut) { dummy in
                        let out = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) ?? dummy
                        var ctx = PhonJITEncodeCtx(
                            base: base.assumingMemoryBound(to: UInt8.self),
                            prog: program.words.pointer,
                            out: out,
                            out_start: out,
                            out_end: out.advanced(by: byteCount),
                            status: 0,
                            scratch: scratch.assumingMemoryBound(to: UInt8.self)
                        )
                        withUnsafeMutablePointer(to: &ctx) { fn($0) }
                        status = ctx.status
                        written = ctx.out_start.distance(to: ctx.out)
                    }
                }
            }

            if status == 0 {
                return Array(bytes.prefix(written))
            }
            precondition(status == 1, "phon JIT encode failed with status \(status)")
            let grown = byteCount > Int.max / 2 ? Int.max : max(byteCount * 2, byteCount + 64)
            precondition(grown > byteCount, "phon JIT encode buffer cannot grow")
            byteCount = grown
        }
    }
}

// r[impl ir.stencils]
private final class ExecutableBuffer {
    let entry: UnsafeRawPointer

    private let mapping: UnsafeMutableRawPointer
    private let size: Int

    // r[impl ir.stencils]
    convenience init(copying stencil: UnsafeRawBufferPointer) throws {
        try self.init(byteCount: stencil.count) { dst in
            memcpy(dst, stencil.baseAddress!, stencil.count)
        }
    }

    // r[impl ir.stencils]
    init(byteCount: Int, build: (UnsafeMutableRawPointer) throws -> Void) throws {
        guard byteCount > 0 else {
            throw PhonJITError.emptyStencil
        }

        let mapped = mmap(
            nil,
            byteCount,
            PROT_READ | PROT_WRITE | PROT_EXEC,
            MAP_PRIVATE | MAP_ANON | MAP_JIT,
            -1,
            0
        )
        guard mapped != MAP_FAILED else {
            throw PhonJITError.executableMemoryUnavailable(errno: errno)
        }

        mapping = mapped!
        size = byteCount
        entry = UnsafeRawPointer(mapping)

        pthread_jit_write_protect_np(0)
        do {
            try build(mapping)
            phon_jit_flush_instruction_cache(mapping, byteCount)
            pthread_jit_write_protect_np(1)
        } catch {
            pthread_jit_write_protect_np(1)
            munmap(mapping, size)
            throw error
        }
    }

    // r[impl ir.stencils]
    static func compileChain(_ stencils: [NativeStencil]) throws -> ExecutableBuffer {
        let done = try doneStencil()
        let totalSize = stencils.reduce(done.bytes.count) { $0 + $1.bytes.count }
        return try ExecutableBuffer(byteCount: totalSize) { dst in
            var opStarts: [UnsafeMutableRawPointer] = []
            var cursor = dst
            for stencil in stencils {
                opStarts.append(cursor)
                memcpy(cursor, stencil.bytes.baseAddress!, stencil.bytes.count)
                cursor = cursor.advanced(by: stencil.bytes.count)
            }

            let doneStart = cursor
            memcpy(doneStart, done.bytes.baseAddress!, done.bytes.count)

            for (index, opStart) in opStarts.enumerated() {
                guard let branchOffset = stencils[index].branchOffset else {
                    continue
                }
                let target = index + 1 < opStarts.count ? opStarts[index + 1] : doneStart
                try patchBranch26(at: opStart.advanced(by: branchOffset), to: target)
            }
        }
    }

    // r[impl ir.stencils]
    deinit {
        munmap(mapping, size)
    }
}

// r[impl ir.stencils]
private func appendScalar(offset: Int, size: Int, align: Int, to words: inout [UInt64]) -> Bool {
    guard offset >= 0, size >= 0, align > 0, align & (align - 1) == 0 else {
        return false
    }
    guard let offsetWord = UInt64(exactly: offset),
          let sizeWord = UInt64(exactly: size),
          let alignWord = UInt64(exactly: align)
    else {
        return false
    }
    words.append(offsetWord)
    words.append(sizeWord)
    words.append(alignWord)
    return true
}

// r[impl ir.stencils]
private func staticStencil(
    _ bytes: UnsafePointer<UInt8>?,
    _ count: Int,
    branchOffset: Int? = nil
) throws -> NativeStencil {
    guard let bytes, count > 0 else {
        throw PhonJITError.emptyStencil
    }
    return NativeStencil(
        bytes: UnsafeRawBufferPointer(start: bytes, count: count),
        branchOffset: branchOffset
    )
}

// r[impl ir.stencils]
private func smokeStencil() throws -> NativeStencil {
    try staticStencil(phon_jit_smoke_bytes(), phon_jit_smoke_len())
}

// r[impl ir.stencils]
private func scalarDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_scalar_decode_bytes(),
        phon_jit_scalar_decode_len(),
        branchOffset: phon_jit_scalar_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func scalarEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_scalar_encode_bytes(),
        phon_jit_scalar_encode_len(),
        branchOffset: phon_jit_scalar_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func optionDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_option_decode_bytes(),
        phon_jit_option_decode_len(),
        branchOffset: phon_jit_option_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func optionEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_option_encode_bytes(),
        phon_jit_option_encode_len(),
        branchOffset: phon_jit_option_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func bytesDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_bytes_decode_bytes(),
        phon_jit_bytes_decode_len(),
        branchOffset: phon_jit_bytes_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func bytesEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_bytes_encode_bytes(),
        phon_jit_bytes_encode_len(),
        branchOffset: phon_jit_bytes_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func dynamicDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_dynamic_decode_bytes(),
        phon_jit_dynamic_decode_len(),
        branchOffset: phon_jit_dynamic_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func dynamicEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_dynamic_encode_bytes(),
        phon_jit_dynamic_encode_len(),
        branchOffset: phon_jit_dynamic_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func enumDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_enum_decode_bytes(),
        phon_jit_enum_decode_len(),
        branchOffset: phon_jit_enum_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func enumEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_enum_encode_bytes(),
        phon_jit_enum_encode_len(),
        branchOffset: phon_jit_enum_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func seqDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_seq_decode_bytes(),
        phon_jit_seq_decode_len(),
        branchOffset: phon_jit_seq_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func seqEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_seq_encode_bytes(),
        phon_jit_seq_encode_len(),
        branchOffset: phon_jit_seq_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func mapDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_map_decode_bytes(),
        phon_jit_map_decode_len(),
        branchOffset: phon_jit_map_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func mapEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_map_encode_bytes(),
        phon_jit_map_encode_len(),
        branchOffset: phon_jit_map_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func blockDecodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_block_decode_bytes(),
        phon_jit_block_decode_len(),
        branchOffset: phon_jit_block_decode_branch_offset()
    )
}

// r[impl ir.stencils]
private func blockEncodeStencil() throws -> NativeStencil {
    try staticStencil(
        phon_jit_block_encode_bytes(),
        phon_jit_block_encode_len(),
        branchOffset: phon_jit_block_encode_branch_offset()
    )
}

// r[impl ir.stencils]
private func doneStencil() throws -> NativeStencil {
    try staticStencil(phon_jit_done_bytes(), phon_jit_done_len())
}

// r[impl ir.stencils]
private func patchBranch26(at site: UnsafeMutableRawPointer, to target: UnsafeMutableRawPointer) throws {
    let fromAddress = Int(bitPattern: site)
    let targetAddress = Int(bitPattern: target)
    let delta = targetAddress - fromAddress
    guard delta % 4 == 0 else {
        throw PhonJITError.branchOutOfRange
    }
    let wordOffset = delta / 4
    guard wordOffset >= -(1 << 25), wordOffset < (1 << 25) else {
        throw PhonJITError.branchOutOfRange
    }

    let slot = site.assumingMemoryBound(to: UInt32.self)
    let original = UInt32(littleEndian: slot.pointee)
    let imm26 = UInt32(bitPattern: Int32(wordOffset)) & 0x03ff_ffff
    slot.pointee = ((original & 0xfc00_0000) | imm26).littleEndian
}

// r[impl ir.stencils]
private func alignUp(_ value: Int, _ alignment: Int) -> Int {
    let mask = alignment - 1
    return (value + mask) & ~mask
}

// r[impl ir.stencils]
private func pointerUInt(_ pointer: UnsafeRawPointer) -> UInt {
    UInt(bitPattern: pointer)
}

// r[impl ir.stencils]
private func pointerUInt<T>(_ pointer: UnsafePointer<T>) -> UInt {
    UInt(bitPattern: pointer)
}

// r[impl ir.stencils]
private func pointerWord<T>(_ pointer: UnsafePointer<T>) -> UInt64 {
    UInt64(pointerUInt(pointer))
}

// r[impl ir.stencils]
private func withScratch<T>(
    byteCount: Int,
    alignment: Int,
    _ body: (UnsafeMutableRawPointer) throws -> T
) rethrows -> T {
    let scratch = UnsafeMutableRawPointer.allocate(
        byteCount: max(byteCount, 1),
        alignment: max(alignment, 1)
    )
    defer { scratch.deallocate() }
    return try body(scratch)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_option_project_some")
func phonJitOptionProjectSome(
    _ ctx: UnsafeRawPointer?,
    _ option: UnsafeRawPointer?,
    _ scratch: UnsafeMutableRawPointer?
) -> Bool {
    guard let ctx, let option, let scratch else {
        return false
    }
    let box = Unmanaged<OptionWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    return box.witness.projectSome(option, scratch)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_option_init_some")
func phonJitOptionInitSome(
    _ ctx: UnsafeRawPointer?,
    _ option: UnsafeMutableRawPointer?,
    _ scratch: UnsafeMutableRawPointer?
) {
    guard let ctx, let option, let scratch else {
        return
    }
    let box = Unmanaged<OptionWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.initSome(option, scratch)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_option_init_none")
func phonJitOptionInitNone(
    _ ctx: UnsafeRawPointer?,
    _ option: UnsafeMutableRawPointer?
) {
    guard let ctx, let option else {
        return
    }
    let box = Unmanaged<OptionWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.initNone(option)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_bytes_count")
func phonJitBytesCount(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?
) -> UInt64 {
    guard let ctx, let field else {
        return 0
    }
    let box = Unmanaged<BytesWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    return UInt64(box.witness.count(field))
}

// r[impl ir.stencils]
@_cdecl("phon_jit_bytes_copy_into")
func phonJitBytesCopyInto(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?,
    _ dst: UnsafeMutableRawPointer?
) {
    guard let ctx, let field, let dst else {
        return
    }
    let box = Unmanaged<BytesWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.copyInto(field, dst)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_bytes_construct")
func phonJitBytesConstruct(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeMutableRawPointer?,
    _ src: UnsafeRawPointer?,
    _ count: UInt64
) -> Bool {
    guard let ctx, let field, let src, let n = Int(exactly: count) else {
        return false
    }
    let box = Unmanaged<BytesWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    return box.witness.construct(field, src, n)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_enum_tag")
func phonJitEnumTag(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?
) -> UInt32 {
    guard let ctx, let field else {
        return UInt32.max
    }
    let box = Unmanaged<EnumWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    return box.tagWireIndex(field) ?? UInt32.max
}

// r[impl ir.stencils]
@_cdecl("phon_jit_enum_project")
func phonJitEnumProject(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?,
    _ localIndex: UInt32,
    _ scratch: UnsafeMutableRawPointer?
) {
    guard let ctx, let field, let scratch else {
        return
    }
    let box = Unmanaged<EnumWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.project(field, localIndex: localIndex, scratch: scratch)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_enum_destroy")
func phonJitEnumDestroy(
    _ ctx: UnsafeRawPointer?,
    _ scratch: UnsafeMutableRawPointer?,
    _ localIndex: UInt32
) {
    guard let ctx, let scratch else {
        return
    }
    let box = Unmanaged<EnumWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.destroy(scratch, localIndex: localIndex)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_enum_inject")
func phonJitEnumInject(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeMutableRawPointer?,
    _ localIndex: UInt32,
    _ scratch: UnsafeMutableRawPointer?
) {
    guard let ctx, let field, let scratch else {
        return
    }
    let box = Unmanaged<EnumWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.inject(field, localIndex: localIndex, scratch: scratch)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_seq_count")
func phonJitSeqCount(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?
) -> UInt64 {
    guard let ctx, let field else {
        return 0
    }
    let box = Unmanaged<SeqWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    return UInt64(box.witness.count(field))
}

// r[impl ir.stencils]
@_cdecl("phon_jit_seq_copy_elements")
func phonJitSeqCopyElements(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?,
    _ dst: UnsafeMutableRawPointer?
) {
    guard let ctx, let field, let dst else {
        return
    }
    let box = Unmanaged<SeqWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.copyElements(field, dst)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_seq_destroy_elements")
func phonJitSeqDestroyElements(
    _ ctx: UnsafeRawPointer?,
    _ elements: UnsafeMutableRawPointer?,
    _ count: UInt64
) {
    guard let ctx, let elements, let n = Int(exactly: count) else {
        return
    }
    let box = Unmanaged<SeqWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.destroyElements?(elements, n)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_seq_construct")
func phonJitSeqConstruct(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeMutableRawPointer?,
    _ src: UnsafeMutableRawPointer?,
    _ count: UInt64
) {
    guard let ctx, let field, let src, let n = Int(exactly: count) else {
        return
    }
    let box = Unmanaged<SeqWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.construct(field, src, n)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_dynamic_decode")
func phonJitDynamicDecode(
    _ ctx: UnsafeMutablePointer<PhonJITDecodeCtx>?,
    _ info: UnsafePointer<PhonJITDynamicInfo>?
) {
    guard let ctx, let info, let wire = ctx.pointee.wire, let wireEnd = ctx.pointee.wire_end,
          let base = ctx.pointee.base, let offset = Int(exactly: info.pointee.field_offset)
    else {
        return
    }

    let remaining = wire.distance(to: wireEnd)
    guard remaining >= 0 else {
        ctx.pointee.status = 1
        return
    }

    let bytes = Array(UnsafeBufferPointer(start: wire, count: remaining))
    var reader = Reader(bytes)
    do {
        let value = try readValue(&reader)
        UnsafeMutableRawPointer(base.advanced(by: offset))
            .assumingMemoryBound(to: Value.self)
            .initialize(to: value)
        ctx.pointee.wire = wire.advanced(by: reader.position)
    } catch {
        ctx.pointee.status = 1
    }
}

// r[impl ir.stencils]
@_cdecl("phon_jit_dynamic_encode")
func phonJitDynamicEncode(
    _ ctx: UnsafeMutablePointer<PhonJITEncodeCtx>?,
    _ info: UnsafePointer<PhonJITDynamicInfo>?
) {
    guard let ctx, let info, let out = ctx.pointee.out, let outEnd = ctx.pointee.out_end,
          let base = ctx.pointee.base, let offset = Int(exactly: info.pointee.field_offset)
    else {
        return
    }

    let value = UnsafeRawPointer(base.advanced(by: offset))
        .assumingMemoryBound(to: Value.self)
        .pointee
    let bytes = valueToBytes(value)
    guard UnsafePointer(out).distance(to: outEnd) >= bytes.count else {
        ctx.pointee.status = 1
        return
    }

    bytes.withUnsafeBytes { raw in
        if let source = raw.baseAddress {
            out.update(from: source.assumingMemoryBound(to: UInt8.self), count: raw.count)
        }
    }
    ctx.pointee.out = out.advanced(by: bytes.count)
}

// r[impl compat.skip-writer-only]
// r[impl ir.stencils]
@_cdecl("phon_jit_skipwire_decode")
func phonJitSkipWireDecode(
    _ ctx: UnsafeMutablePointer<PhonJITDecodeCtx>?,
    _ info: UnsafePointer<PhonJITSkipWireInfo>?
) {
    guard let ctx, let info, let wire = ctx.pointee.wire, let wireEnd = ctx.pointee.wire_end,
          let rawBox = UnsafeRawPointer(bitPattern: UInt(info.pointee.ctx))
    else {
        return
    }

    let remaining = wire.distance(to: wireEnd)
    guard remaining >= 0 else {
        ctx.pointee.status = 1
        return
    }

    let box = Unmanaged<SkipWireBox>.fromOpaque(rawBox).takeUnretainedValue()
    let bytes = Array(UnsafeBufferPointer(start: wire, count: remaining))
    var reader = Reader(bytes)
    do {
        try skipWire(&reader, box.op)
        ctx.pointee.wire = wire.advanced(by: reader.position)
    } catch let error as CompactError {
        setSkipWireStatus(error, ctx)
    } catch {
        ctx.pointee.status = 9
    }
}

// r[impl compat.reader-only-fields]
// r[impl compat.defaults-are-reader-side]
// r[impl ir.stencils]
@_cdecl("phon_jit_default_decode")
func phonJitDefaultDecode(
    _ ctx: UnsafeMutablePointer<PhonJITDecodeCtx>?,
    _ info: UnsafePointer<PhonJITDefaultInfo>?
) {
    guard let ctx, let info, let base = ctx.pointee.base,
          let rawBox = UnsafeRawPointer(bitPattern: UInt(info.pointee.ctx))
    else {
        return
    }

    let box = Unmanaged<DefaultBox>.fromOpaque(rawBox).takeUnretainedValue()
    box.op.initFn(UnsafeMutableRawPointer(base.advanced(by: box.op.offset)))
}

// r[impl ir.stencils]
private func setSkipWireStatus(
    _ error: CompactError,
    _ ctx: UnsafeMutablePointer<PhonJITDecodeCtx>
) {
    switch error {
    case .decode(.unexpectedEof):
        ctx.pointee.status = 1
    case .decode(.invalidBool(let byte)):
        ctx.pointee.status = 2
        ctx.pointee.aux = UInt64(byte)
    case .badVariantIndex(let index):
        ctx.pointee.status = 4
        ctx.pointee.aux = UInt64(index)
    case .writerOnlyVariant(let index):
        ctx.pointee.status = 8
        ctx.pointee.aux = UInt64(index)
    default:
        ctx.pointee.status = 9
    }
}

// r[impl ir.stencils]
@_cdecl("phon_jit_map_count")
func phonJitMapCount(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?
) -> UInt64 {
    guard let ctx, let field else {
        return 0
    }
    let box = Unmanaged<MapWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    return UInt64(box.witness.count(field))
}

// r[impl ir.stencils]
@_cdecl("phon_jit_map_project_entries")
func phonJitMapProjectEntries(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeRawPointer?,
    _ keys: UnsafeMutableRawPointer?,
    _ values: UnsafeMutableRawPointer?
) {
    guard let ctx, let field, let keys, let values else {
        return
    }
    let box = Unmanaged<MapWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.projectEntries(field, keys, values)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_map_destroy_entries")
func phonJitMapDestroyEntries(
    _ ctx: UnsafeRawPointer?,
    _ keys: UnsafeMutableRawPointer?,
    _ values: UnsafeMutableRawPointer?,
    _ count: UInt64
) {
    guard let ctx, let keys, let values, let n = Int(exactly: count) else {
        return
    }
    let box = Unmanaged<MapWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.destroyEntries(keys, values, n)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_map_init_with_capacity")
func phonJitMapInitWithCapacity(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeMutableRawPointer?,
    _ capacity: UInt64
) {
    guard let ctx, let field, let cap = Int(exactly: capacity) else {
        return
    }
    let box = Unmanaged<MapWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.initWithCapacity(field, cap)
}

// r[impl ir.stencils]
@_cdecl("phon_jit_map_insert")
func phonJitMapInsert(
    _ ctx: UnsafeRawPointer?,
    _ field: UnsafeMutableRawPointer?,
    _ key: UnsafeMutableRawPointer?,
    _ value: UnsafeMutableRawPointer?
) {
    guard let ctx, let field, let key, let value else {
        return
    }
    let box = Unmanaged<MapWitnessBox>.fromOpaque(ctx).takeUnretainedValue()
    box.witness.insert(field, key, value)
}
