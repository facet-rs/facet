import Foundation

// MARK: - Varint (LEB128) encoding

/// Encode an unsigned 64-bit integer as a varint.
/// r[impl signature.varint]
public func encodeVarint(_ value: UInt64) -> [UInt8] {
    var result: [UInt8] = []
    var remaining = value
    repeat {
        var byte = UInt8(remaining & 0x7F)
        remaining >>= 7
        if remaining != 0 {
            byte |= 0x80
        }
        result.append(byte)
    } while remaining != 0
    return result
}

/// Decode a varint from data at the given offset.
/// Updates the offset to point past the decoded varint.
public func decodeVarint(from data: Data, offset: inout Int) throws -> UInt64 {
    var result: UInt64 = 0
    var shift: UInt64 = 0
    while true {
        guard offset < data.count else {
            throw RapaceError.decodeError("varint: unexpected EOF")
        }
        let byte = data[offset]
        offset += 1
        if shift >= 64 {
            throw RapaceError.decodeError("varint: overflow")
        }
        result |= UInt64(byte & 0x7F) << shift
        if (byte & 0x80) == 0 {
            return result
        }
        shift += 7
    }
}

/// Decode a varint as UInt32 with overflow check.
public func decodeVarintU32(from data: Data, offset: inout Int) throws -> UInt32 {
    let v = try decodeVarint(from: data, offset: &offset)
    guard v <= UInt64(UInt32.max) else {
        throw RapaceError.decodeError("varint u32: overflow")
    }
    return UInt32(v)
}
