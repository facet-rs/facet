import PhonIR
import PhonSchema

// The typed encode/decode SEAM, and the backend abstraction the JIT plugs into.
//
// `encodeTyped`/`decodeTyped` are the single boundary generated code and the runtime
// use (replacing scattered `withUnsafeBytes(of:)` + `UnsafeMutableRawPointer.allocate`
// boilerplate). `TypedEngine` lets a test harness — or the runtime — run the SAME
// descriptor through the tree-walk interpreter today and the copy-and-patch JIT later,
// asserting they agree byte-for-byte. Adding the JIT is one more `TypedEngine`.

// MARK: - The seam

/// Encode a typed value through a (pre-lowered) program: the value's in-memory bytes
/// are read by the program's witnesses. The one place the unsafe value→bytes boundary
/// lives.
public func encodeTyped<T>(_ value: T, _ lowered: Lowered) -> [UInt8] {
    var v = value
    return withUnsafeBytes(of: &v) { encodeWith(lowered, $0.baseAddress!) }
}

/// Decode a typed value through a (pre-lowered) program into a fresh `T`.
public func decodeTyped<T>(_ lowered: Lowered, _ bytes: [UInt8]) throws -> T {
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<T>.size, alignment: MemoryLayout<T>.alignment)
    defer { raw.deallocate() }
    try decodeInto(lowered, bytes, raw)
    return raw.assumingMemoryBound(to: T.self).move()
}

// MARK: - Backend abstraction (interpreter today, JIT tomorrow)

/// A prepared encoder: read a value at `base` and emit its wire bytes. Not `@Sendable`
/// — it captures the (non-Sendable) lowered program; callers run it locally.
public typealias TypedEncodeFn = (_ base: UnsafeRawPointer) -> [UInt8]
/// A prepared decoder: translate `bytes` into the reader value at `out`.
public typealias TypedDecodeFn = (_ bytes: [UInt8], _ out: UnsafeMutableRawPointer) throws -> Void

/// A typed codec backend. Each `compile*` lowers the descriptor to a `MemProgram` and
/// prepares its representation — the interpreter keeps the program; the JIT compiles it
/// to machine code. Both must produce byte-identical output for any value.
public protocol TypedEngine: Sendable {
    var name: String { get }
    /// Prepare an own-schema encoder for `descriptor`.
    func compileEncode(_ descriptor: Descriptor, _ reg: Registry) throws -> TypedEncodeFn
    /// Prepare a compat decoder: `writerRoot` → `reader` (same root ⇒ the fused identity).
    func compileDecode(_ writerRoot: SchemaId, _ reader: Descriptor, _ reg: Registry) throws -> TypedDecodeFn
}

/// The tree-walk interpreter backend (lowers, then walks the `MemProgram` per op).
public struct InterpreterEngine: TypedEngine {
    public let name = "interpreter"
    public init() {}

    public func compileEncode(_ descriptor: Descriptor, _ reg: Registry) throws -> TypedEncodeFn {
        let program = try lowerTyped(descriptor, reg)
        return { base in encodeWith(program, base) }
    }

    public func compileDecode(_ writerRoot: SchemaId, _ reader: Descriptor, _ reg: Registry) throws -> TypedDecodeFn {
        let program = try lowerDecode(writerRoot, reader, reg)
        return { bytes, out in try decodeInto(program, bytes, out) }
    }
}

public extension Descriptor {
    /// The descriptor's own concrete root id (its writer == reader root).
    var rootId: SchemaId {
        guard case .concrete(let id, _) = schema else {
            preconditionFailure("descriptor root must be a concrete schema id")
        }
        return id
    }
}
