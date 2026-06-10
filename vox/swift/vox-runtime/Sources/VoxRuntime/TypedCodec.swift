import Foundation
import PhonEngine
import PhonIR

private final class VoxTypedCodecState: @unchecked Sendable {
    let lock = NSLock()
    var engine: any TypedEngine = InterpreterEngine()
    var generation: UInt64 = 0
}

public enum VoxTypedCodec {
    private static let state = VoxTypedCodecState()

    public static var activeEngineName: String {
        state.lock.lock()
        defer { state.lock.unlock() }
        return state.engine.name
    }

    // r[impl conduit.typeplan]
    // r[impl schema.tracking.received]
    public static func configure(engine newEngine: any TypedEngine) {
        state.lock.lock()
        defer { state.lock.unlock() }
        state.engine = newEngine
        state.generation &+= 1
    }

    static func snapshot() -> (generation: UInt64, engine: any TypedEngine) {
        state.lock.lock()
        defer { state.lock.unlock() }
        return (state.generation, state.engine)
    }

    static func compileEncode(_ lowered: Lowered) -> (generation: UInt64, fn: TypedEncodeFn) {
        let current = snapshot()
        if let fn = try? current.engine.compileEncode(lowered) {
            return (current.generation, fn)
        }
        return (current.generation, try! InterpreterEngine().compileEncode(lowered))
    }

    static func compileDecode(_ lowered: Lowered) -> (generation: UInt64, fn: TypedDecodeFn) {
        let current = snapshot()
        if let fn = try? current.engine.compileDecode(lowered) {
            return (current.generation, fn)
        }
        return (current.generation, try! InterpreterEngine().compileDecode(lowered))
    }
}

public final class VoxTypedEncoder: @unchecked Sendable {
    private let lowered: Lowered
    private let lock = NSLock()
    private var cachedGeneration: UInt64?
    private var cached: TypedEncodeFn?

    public init(_ lowered: Lowered) {
        self.lowered = lowered
    }

    // r[impl conduit.typeplan]
    public func encode<T>(_ value: T) -> [UInt8] {
        let fn = compiled()
        var v = value
        return withUnsafeBytes(of: &v) { fn($0.baseAddress!) }
    }

    private func compiled() -> TypedEncodeFn {
        let current = VoxTypedCodec.snapshot()
        lock.lock()
        defer { lock.unlock() }
        if cachedGeneration == current.generation, let cached {
            return cached
        }
        let compiled = VoxTypedCodec.compileEncode(lowered)
        cachedGeneration = compiled.generation
        cached = compiled.fn
        return compiled.fn
    }
}

public func encodeVoxTyped<T>(_ value: T, _ encoder: VoxTypedEncoder) -> [UInt8] {
    encoder.encode(value)
}

public func decodeVoxTyped<T>(_ decoder: TypedDecodeFn, _ bytes: [UInt8]) throws -> T {
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<T>.size, alignment: MemoryLayout<T>.alignment)
    defer { raw.deallocate() }
    try decoder(bytes, raw)
    return raw.assumingMemoryBound(to: T.self).move()
}
