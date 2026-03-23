import Foundation

/// Encode a 64-bit unsigned integer as a varint.
public func encodeVarint(_ value: UInt64) -> [UInt8] {
    var result: [UInt8] = []
    var v = value
    while v >= 0x80 {
        result.append(UInt8(v & 0x7F) | 0x80)
        v >>= 7
    }
    result.append(UInt8(v))
    return result
}

/// Decode a varint from data, returning the value and advancing the offset.
public func decodeVarint(from data: Data, offset: inout Int) throws -> UInt64 {
    var result: UInt64 = 0
    var shift: UInt64 = 0

    while offset < data.count {
        let byte = data[data.startIndex + offset]
        offset += 1

        result |= UInt64(byte & 0x7F) << shift

        if byte & 0x80 == 0 {
            return result
        }

        shift += 7
        if shift >= 64 {
            throw VarintError.overflow
        }
    }

    throw VarintError.truncated
}

/// Decode a varint as UInt32, checking for overflow.
public func decodeVarintU32(from data: Data, offset: inout Int) throws -> UInt32 {
    let value = try decodeVarint(from: data, offset: &offset)
    guard value <= UInt32.max else {
        throw VarintError.overflow
    }
    return UInt32(value)
}

/// Errors that can occur during varint decoding.
public enum VarintError: Error {
    case truncated
    case overflow
}
