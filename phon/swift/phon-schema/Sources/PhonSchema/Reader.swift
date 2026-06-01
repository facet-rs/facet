// A cursor over input bytes. Scalars are read as little-endian; strings and byte
// runs are length-prefixed (`u32` count). Mirrors `rust/phon-schema/src/bytes.rs`.
//
// Decoding never copies the input wholesale: the reader advances a position over
// the borrowed buffer and assembles scalars in place. Owned `String`s are built
// only where the model owns them (schema names, type-param names), matching the
// Rust decoder.

/// The fixed cap on a zero-sized element's count, where the buffer gives no bound.
public let zstCountCap = 1 << 24

public enum DecodeError: Error, Equatable, Sendable {
    case unexpectedEof(needed: Int, remaining: Int)
    case invalidBool(UInt8)
    case invalidUtf8
    case invalidChar(UInt32)
    case lengthTooLarge(count: UInt64, remaining: Int)
    case depthExceeded
    case unexpectedTag(expected: String, got: UInt8)
    case unknownVariant(String)
    case malformed(String)
    case trailingBytes(Int)
}

public struct Reader {
    private let buf: [UInt8]
    public private(set) var pos: Int

    public init(_ buf: [UInt8]) {
        self.buf = buf
        self.pos = 0
    }

    public var remaining: Int { buf.count - pos }
    public var position: Int { pos }

    private mutating func take(_ n: Int) throws -> ArraySlice<UInt8> {
        guard remaining >= n else {
            throw DecodeError.unexpectedEof(needed: n, remaining: remaining)
        }
        let slice = buf[pos..<(pos + n)]
        pos += n
        return slice
    }

    /// A borrowed run of `n` bytes (a view into the input, no copy).
    public mutating func readSlice(_ n: Int) throws -> ArraySlice<UInt8> {
        try take(n)
    }

    public mutating func readU8() throws -> UInt8 {
        let s = try take(1)
        return s[s.startIndex]
    }

    public mutating func readU16() throws -> UInt16 { try readLE() }
    public mutating func readU32() throws -> UInt32 { try readLE() }
    public mutating func readU64() throws -> UInt64 { try readLE() }

    public mutating func readF32() throws -> Float { Float(bitPattern: try readLE()) }
    public mutating func readF64() throws -> Double { Double(bitPattern: try readLE()) }

    public mutating func readBool() throws -> Bool {
        switch try readU8() {
        case 0: return false
        case 1: return true
        case let b: throw DecodeError.invalidBool(b)
        }
    }

    /// A length-prefixed UTF-8 string (owned). Strict validation — rejects
    /// malformed UTF-8 rather than substituting replacement characters.
    public mutating func readStr() throws -> String {
        let len = try readLen(minElemSize: 1)
        let bytes = try take(len)
        guard let s = String(validating: bytes, as: UTF8.self) else {
            throw DecodeError.invalidUtf8
        }
        return s
    }

    /// A length-prefixed byte run (borrowed view).
    public mutating func readBytes() throws -> ArraySlice<UInt8> {
        let len = try readLen(minElemSize: 1)
        return try take(len)
    }

    /// A `u32` element count, bounded so a corrupt length can't drive a huge
    /// allocation: at most `remaining / minElemSize` (or `zstCountCap` for
    /// zero-sized elements).
    public mutating func readLen(minElemSize: Int) throws -> Int {
        let count = Int(try readU32())
        let max = minElemSize == 0 ? zstCountCap : remaining / minElemSize
        guard count <= max else {
            throw DecodeError.lengthTooLarge(count: UInt64(count), remaining: remaining)
        }
        return count
    }

    /// Read a fixed-width little-endian integer.
    private mutating func readLE<T: FixedWidthInteger & UnsignedInteger>() throws -> T {
        let s = try take(MemoryLayout<T>.size)
        var v: T = 0
        for (k, b) in s.enumerated() { v |= T(b) << (8 * k) }
        return v
    }
}
