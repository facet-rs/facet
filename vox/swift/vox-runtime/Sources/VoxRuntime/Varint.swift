@preconcurrency import NIOCore

/// Encode a 64-bit unsigned integer as a varint into a ByteBuffer.
@inline(__always)
public func encodeVarint(_ value: UInt64, into buffer: inout ByteBuffer) {
    var v = value
    while v >= 0x80 {
        buffer.writeInteger(UInt8(v & 0x7F) | 0x80)
        v >>= 7
    }
    buffer.writeInteger(UInt8(v))
}

/// Decode a varint from a ByteBuffer, advancing the reader index.
@inline(__always)
public func decodeVarint(from buffer: inout ByteBuffer) throws -> UInt64 {
    var result: UInt64 = 0
    var shift: UInt64 = 0
    while buffer.readableBytes > 0 {
        guard let byte: UInt8 = buffer.readInteger() else { throw VarintError.truncated }
        result |= UInt64(byte & 0x7F) << shift
        if byte & 0x80 == 0 { return result }
        shift += 7
        if shift >= 64 { throw VarintError.overflow }
    }
    throw VarintError.truncated
}

/// Decode a varint as UInt32, checking for overflow.
@inline(__always)
public func decodeVarintU32(from buffer: inout ByteBuffer) throws -> UInt32 {
    let value = try decodeVarint(from: &buffer)
    guard value <= UInt32.max else { throw VarintError.overflow }
    return UInt32(value)
}

/// Errors that can occur during varint decoding.
public enum VarintError: Error {
    case truncated
    case overflow
}
