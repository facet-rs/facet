// Wire byte primitives: the little-endian scalar layout and length-prefixed
// strings/bytes shared by the self-describing format and the identity canonical
// encoding. Mirrors `rust/phon-schema/src/bytes.rs` byte-for-byte.

/// A byte sink. The two implementations are an in-memory buffer (`ByteSink`,
/// for serialization) and the BLAKE3 hasher (for identity) — the canonical
/// encoding is written straight into the hasher, never an intermediate buffer.
public protocol Sink {
    mutating func put(_ bytes: ArraySlice<UInt8>)
}

public extension Sink {
    mutating func put(_ bytes: [UInt8]) { put(bytes[...]) }

    mutating func writeU8(_ v: UInt8) { put([v]) }
    mutating func writeU16(_ v: UInt16) { putLE(v) }
    mutating func writeU32(_ v: UInt32) { putLE(v) }
    mutating func writeU64(_ v: UInt64) { putLE(v) }
    mutating func writeI8(_ v: Int8) { put([UInt8(bitPattern: v)]) }
    mutating func writeI16(_ v: Int16) { putLE(UInt16(bitPattern: v)) }
    mutating func writeI32(_ v: Int32) { putLE(UInt32(bitPattern: v)) }
    mutating func writeI64(_ v: Int64) { putLE(UInt64(bitPattern: v)) }
    mutating func writeF32(_ v: Float) { putLE(v.bitPattern) }
    mutating func writeF64(_ v: Double) { putLE(v.bitPattern) }

    // 128-bit values are written as two little-endian `u64` halves (low then
    // high) — identical bytes to a single LE `u128`, but available before
    // macOS 15's `UInt128`. Used by the value codec, not by identity.
    mutating func writeU128(low: UInt64, high: UInt64) { putLE(low); putLE(high) }

    mutating func writeBool(_ v: Bool) { put([v ? 1 : 0]) }

    /// `[u32 LE length][UTF-8 bytes]`.
    mutating func writeStr(_ s: String) {
        let u = Array(s.utf8)
        writeU32(UInt32(u.count))
        put(u)
    }

    /// `[u32 LE length][raw bytes]`.
    mutating func writeBytes(_ b: [UInt8]) {
        writeU32(UInt32(b.count))
        put(b)
    }

    private mutating func putLE<T: FixedWidthInteger>(_ v: T) {
        var le = v.littleEndian
        withUnsafeBytes(of: &le) { put(Array($0)) }
    }
}

/// An in-memory byte buffer.
public struct ByteSink: Sink {
    public private(set) var bytes: [UInt8] = []
    public init() {}
    public mutating func put(_ b: ArraySlice<UInt8>) { bytes.append(contentsOf: b) }
}
