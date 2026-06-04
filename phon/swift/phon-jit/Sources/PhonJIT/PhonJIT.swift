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
        if let native = try NativeScalarEncode.compile(lowered) {
            return { base in native.run(base) }
        }
        return { base in encodeWith(lowered, base) }
    }

    // r[impl exec.jit-optional]
    // r[impl ir.stencils]
    public func compileDecode(_ writerRoot: SchemaId, _ reader: Descriptor, _ reg: Registry) throws -> TypedDecodeFn {
        let lowered = try lowerDecode(writerRoot, reader, reg)
        if let native = try NativeScalarDecode.compile(lowered) {
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
private struct ScalarProgram {
    var words: [UInt64]
    var wireSize: Int

    // r[impl ir.stencils]
    var opCount: Int {
        words.count / 3
    }
}

// r[impl ir.stencils]
final class NativeScalarDecode {
    private let code: ExecutableBuffer
    private let program: [UInt64]

    // r[impl ir.stencils]
    static func compile(_ lowered: Lowered) throws -> NativeScalarDecode? {
        guard let scalar = scalarProgram(lowered) else {
            return nil
        }
        return try NativeScalarDecode(scalar)
    }

    // r[impl ir.stencils]
    private init(_ scalar: ScalarProgram) throws {
        program = scalar.words
        code = try ExecutableBuffer.compileChain(
            opCount: scalar.opCount,
            stencil: try scalarDecodeStencil()
        )
    }

    // r[impl ir.stencils]
    func run(_ bytes: [UInt8], _ out: UnsafeMutableRawPointer) throws {
        typealias DecodeFn = @convention(c) (UnsafeMutablePointer<PhonJITDecodeCtx>) -> Void

        var dummyWire: UInt8 = 0
        var status: UInt64 = 0
        var remaining = 0
        let fn = unsafeBitCast(code.entry, to: DecodeFn.self)

        program.withUnsafeBufferPointer { prog in
            bytes.withUnsafeBytes { raw in
                withUnsafePointer(to: &dummyWire) { dummy in
                    let wire = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) ?? dummy
                    var ctx = PhonJITDecodeCtx(
                        wire: wire,
                        wire_start: wire,
                        wire_end: wire.advanced(by: bytes.count),
                        base: out.assumingMemoryBound(to: UInt8.self),
                        prog: prog.baseAddress,
                        status: 0
                    )
                    withUnsafeMutablePointer(to: &ctx) { fn($0) }
                    status = ctx.status
                    remaining = ctx.wire.distance(to: ctx.wire_end)
                }
            }
        }

        if status != 0 {
            throw CompactError.decode(.unexpectedEof(needed: 1, remaining: 0))
        }
        if remaining != 0 {
            throw CompactError.decode(.trailingBytes(remaining))
        }
    }
}

// r[impl ir.stencils]
final class NativeScalarEncode {
    private let code: ExecutableBuffer
    private let program: [UInt64]
    private let wireSize: Int

    // r[impl ir.stencils]
    static func compile(_ lowered: Lowered) throws -> NativeScalarEncode? {
        guard let scalar = scalarProgram(lowered) else {
            return nil
        }
        return try NativeScalarEncode(scalar)
    }

    // r[impl ir.stencils]
    private init(_ scalar: ScalarProgram) throws {
        program = scalar.words
        wireSize = scalar.wireSize
        code = try ExecutableBuffer.compileChain(
            opCount: scalar.opCount,
            stencil: try scalarEncodeStencil()
        )
    }

    // r[impl ir.stencils]
    func run(_ base: UnsafeRawPointer) -> [UInt8] {
        typealias EncodeFn = @convention(c) (UnsafeMutablePointer<PhonJITEncodeCtx>) -> Void

        var bytes = [UInt8](repeating: 0, count: wireSize)
        let byteCount = bytes.count
        var dummyOut: UInt8 = 0
        var status: UInt64 = 0
        let fn = unsafeBitCast(code.entry, to: EncodeFn.self)

        program.withUnsafeBufferPointer { prog in
            bytes.withUnsafeMutableBytes { raw in
                withUnsafeMutablePointer(to: &dummyOut) { dummy in
                    let out = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) ?? dummy
                    var ctx = PhonJITEncodeCtx(
                        base: base.assumingMemoryBound(to: UInt8.self),
                        prog: prog.baseAddress,
                        out: out,
                        out_start: out,
                        out_end: out.advanced(by: byteCount),
                        status: 0
                    )
                    withUnsafeMutablePointer(to: &ctx) { fn($0) }
                    status = ctx.status
                }
            }
        }

        precondition(status == 0, "phon JIT encode wrote past its precomputed buffer")
        return bytes
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
    static func compileChain(opCount: Int, stencil: NativeStencil) throws -> ExecutableBuffer {
        let done = try doneStencil()
        let totalSize = opCount * stencil.bytes.count + done.bytes.count
        return try ExecutableBuffer(byteCount: totalSize) { dst in
            var opStarts: [UnsafeMutableRawPointer] = []
            var cursor = dst
            for _ in 0..<opCount {
                opStarts.append(cursor)
                memcpy(cursor, stencil.bytes.baseAddress!, stencil.bytes.count)
                cursor = cursor.advanced(by: stencil.bytes.count)
            }

            let doneStart = cursor
            memcpy(doneStart, done.bytes.baseAddress!, done.bytes.count)

            guard let branchOffset = stencil.branchOffset else {
                return
            }
            for (index, opStart) in opStarts.enumerated() {
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
private func scalarProgram(_ lowered: Lowered) -> ScalarProgram? {
    guard lowered.blocks.isEmpty else {
        return nil
    }

    var words: [UInt64] = []
    var wireSize = 0
    for op in lowered.program {
        guard case .scalar(let offset, let size, let align) = op else {
            return nil
        }
        guard offset >= 0, size >= 0, align > 0, align & (align - 1) == 0 else {
            return nil
        }
        guard let offsetWord = UInt64(exactly: offset),
              let sizeWord = UInt64(exactly: size),
              let alignWord = UInt64(exactly: align)
        else {
            return nil
        }
        let pad = (align - (wireSize & (align - 1))) & (align - 1)
        wireSize += pad + size
        words.append(offsetWord)
        words.append(sizeWord)
        words.append(alignWord)
    }

    return ScalarProgram(words: words, wireSize: wireSize)
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
