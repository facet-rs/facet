import CPhonJITStencils
import Darwin
import PhonEngine
import PhonIR
import PhonSchema

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
    public func compileEncode(_ descriptor: Descriptor, _ reg: Registry) throws -> TypedEncodeFn {
        let lowered = try lowerTyped(descriptor, reg)
        if let native = try NativeEncode.compile(lowered) {
            return { base in native.run(base) }
        }
        return { base in encodeWith(lowered, base) }
    }

    // r[impl exec.jit-optional]
    // r[impl ir.stencils]
    public func compileDecode(_ writerRoot: SchemaId, _ reader: Descriptor, _ reg: Registry) throws -> TypedDecodeFn {
        let lowered = try lowerDecode(writerRoot, reader, reg)
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

    // r[impl ir.stencils]
    static func compile(
        _ program: MemProgram,
        mode: NativeMode,
        scratch: inout ScratchAllocator
    ) throws -> NativeProgram? {
        var words: [UInt64] = []
        var stencils: [NativeStencil] = []
        var children: [NativeProgram] = []
        var infos: [OptionInfoAllocation] = []
        var witnessBoxes: [OptionWitnessBox] = []
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
                guard let child = try NativeProgram.compile(option.some, mode: mode, scratch: &scratch) else {
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
            default:
                return nil
            }
        }

        return try NativeProgram(
            code: ExecutableBuffer.compileChain(stencils),
            words: WordBuffer(words),
            maxWireSize: maxWireSize,
            children: children,
            infos: infos,
            witnessBoxes: witnessBoxes
        )
    }

    // r[impl ir.stencils]
    private init(
        code: ExecutableBuffer,
        words: WordBuffer,
        maxWireSize: Int,
        children: [NativeProgram],
        infos: [OptionInfoAllocation],
        witnessBoxes: [OptionWitnessBox]
    ) {
        self.code = code
        self.words = words
        self.maxWireSize = maxWireSize
        self.children = children
        self.infos = infos
        self.witnessBoxes = witnessBoxes
    }
}

// r[impl ir.stencils]
final class NativeDecode {
    private let program: NativeProgram
    private let scratchSize: Int
    private let scratchAlignment: Int

    // r[impl ir.stencils]
    static func compile(_ lowered: Lowered) throws -> NativeDecode? {
        guard lowered.blocks.isEmpty else {
            return nil
        }
        var scratch = ScratchAllocator()
        guard let program = try NativeProgram.compile(lowered.program, mode: .decode, scratch: &scratch) else {
            return nil
        }
        return NativeDecode(program: program, scratch: scratch)
    }

    // r[impl ir.stencils]
    private init(program: NativeProgram, scratch: ScratchAllocator) {
        self.program = program
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
    private let scratchSize: Int
    private let scratchAlignment: Int

    // r[impl ir.stencils]
    static func compile(_ lowered: Lowered) throws -> NativeEncode? {
        guard lowered.blocks.isEmpty else {
            return nil
        }
        var scratch = ScratchAllocator()
        guard let program = try NativeProgram.compile(lowered.program, mode: .encode, scratch: &scratch) else {
            return nil
        }
        return NativeEncode(program: program, scratch: scratch)
    }

    // r[impl ir.stencils]
    private init(program: NativeProgram, scratch: ScratchAllocator) {
        self.program = program
        scratchSize = scratch.size
        scratchAlignment = scratch.alignment
    }

    // r[impl ir.stencils]
    func run(_ base: UnsafeRawPointer) -> [UInt8] {
        typealias EncodeFn = @convention(c) (UnsafeMutablePointer<PhonJITEncodeCtx>) -> Void

        var bytes = [UInt8](repeating: 0, count: program.maxWireSize)
        let byteCount = bytes.count
        var dummyOut: UInt8 = 0
        var status: UInt64 = 0
        var written = 0
        let fn = unsafeBitCast(program.code.entry, to: EncodeFn.self)

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

        precondition(status == 0, "phon JIT encode wrote past its precomputed buffer")
        return Array(bytes.prefix(written))
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
