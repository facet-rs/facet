// Wire byte primitives: the little-endian scalar layout and length-prefixed
// strings/bytes shared by the self-describing format and the identity canonical
// encoding. Mirrors `rust/phon-schema/src/bytes.rs` byte-for-byte.
//
// The sink primitive is `put(UnsafeRawBufferPointer)` — a *borrowed* byte view,
// like Rust's `Sink::put(&[u8])`. Scalar writers hand it the bytes of a value
// living on the stack (`withUnsafeBytes(of:)`), so writing an integer is a memcpy
// into the destination, never a heap allocation. There is no intermediate
// per-write buffer; the identity path writes straight into the BLAKE3 hasher, and
// serialization appends straight into one growing buffer.

/// A byte sink. Implementations: `ByteSink` (one growing buffer, for
/// serialization) and `Blake3` (the hasher, for identity).
public protocol Sink {
    /// Append a borrowed run of bytes. The buffer is only valid for the call.
    mutating func put(_ bytes: UnsafeRawBufferPointer)
}

public extension Sink {
    mutating func put(_ bytes: [UInt8]) {
        bytes.withUnsafeBytes { put($0) }
    }
    mutating func put(_ bytes: ArraySlice<UInt8>) {
        bytes.withUnsafeBytes { put($0) }
    }

    mutating func writeU8(_ v: UInt8) { putScalar(v) }
    mutating func writeU16(_ v: UInt16) { putScalar(v.littleEndian) }
    mutating func writeU32(_ v: UInt32) { putScalar(v.littleEndian) }
    mutating func writeU64(_ v: UInt64) { putScalar(v.littleEndian) }
    mutating func writeI8(_ v: Int8) { putScalar(v) }
    mutating func writeI16(_ v: Int16) { putScalar(v.littleEndian) }
    mutating func writeI32(_ v: Int32) { putScalar(v.littleEndian) }
    mutating func writeI64(_ v: Int64) { putScalar(v.littleEndian) }
    mutating func writeF32(_ v: Float) { putScalar(v.bitPattern.littleEndian) }
    mutating func writeF64(_ v: Double) { putScalar(v.bitPattern.littleEndian) }

    mutating func writeU128(_ v: UInt128) { putScalar(v.littleEndian) }
    mutating func writeI128(_ v: Int128) { putScalar(v.littleEndian) }

    mutating func writeBool(_ v: Bool) { putScalar(UInt8(v ? 1 : 0)) }

    /// `[u32 LE length][UTF-8 bytes]`. Writes the string's UTF-8 directly from its
    /// contiguous storage — no intermediate `[UInt8]`.
    mutating func writeStr(_ s: String) {
        var s = s
        s.withUTF8 { buf in
            writeU32(UInt32(buf.count))
            put(UnsafeRawBufferPointer(buf))
        }
    }

    /// `[u32 LE length][raw bytes]`.
    mutating func writeBytes(_ b: UnsafeRawBufferPointer) {
        writeU32(UInt32(b.count))
        put(b)
    }

    /// Append the raw little-endian bytes of a stack value — one memcpy, no alloc.
    private mutating func putScalar<T>(_ v: T) {
        var v = v
        withUnsafeBytes(of: &v) { put($0) }
    }
}

/// An in-memory byte buffer. One contiguous `[UInt8]` that grows geometrically;
/// `put` appends into it (never builds-and-joins intermediate arrays).
public struct ByteSink: Sink {
    public private(set) var bytes: [UInt8] = []

    public init(reservingCapacity capacity: Int = 0) {
        if capacity > 0 { bytes.reserveCapacity(capacity) }
    }

    public mutating func put(_ b: UnsafeRawBufferPointer) {
        guard b.count > 0 else { return }
        bytes.append(contentsOf: b)
    }
}
