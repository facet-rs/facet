import Foundation

/// Errors that can occur during varint decoding
public enum VarintError: Error {
    /// The input data ended before the varint was complete
    case unexpectedEndOfData
    /// The varint encoding is too long (more than 10 bytes for u64)
    case overflow
}

/// Encodes an unsigned 64-bit integer using LEB128 varint encoding.
///
/// LEB128 (Little Endian Base 128) encoding stores 7 bits of data per byte,
/// using the high bit as a continuation flag.
///
/// - Parameter value: The value to encode
/// - Returns: The encoded bytes
public func encodeVarint(_ value: UInt64) -> [UInt8] {
    var result: [UInt8] = []
    var remaining = value

    repeat {
        // Take the low 7 bits
        var byte = UInt8(remaining & 0x7F)
        remaining >>= 7

        // If there are more bits, set the continuation flag
        if remaining != 0 {
            byte |= 0x80
        }

        result.append(byte)
    } while remaining != 0

    return result
}

/// Decodes an unsigned 64-bit integer from LEB128 varint encoding.
///
/// - Parameter data: The data to decode from. Will be advanced past the varint.
/// - Returns: The decoded value
/// - Throws: `VarintError.unexpectedEndOfData` if data ends before varint is complete,
///           `VarintError.overflow` if the varint is too long
public func decodeVarint(from data: inout Data) throws -> UInt64 {
    var result: UInt64 = 0
    var shift: UInt64 = 0

    while true {
        guard !data.isEmpty else {
            throw VarintError.unexpectedEndOfData
        }

        let byte = data.removeFirst()

        // Check for overflow before shifting
        if shift >= 64 {
            throw VarintError.overflow
        }

        // Add the 7 data bits to the result
        result |= UInt64(byte & 0x7F) << shift

        // If continuation bit is not set, we're done
        if byte & 0x80 == 0 {
            return result
        }

        shift += 7
    }
}

/// Encodes a signed 64-bit integer using zigzag encoding.
///
/// Zigzag encoding maps signed integers to unsigned integers so that
/// small magnitude values (positive and negative) have small encodings.
/// This is done by interleaving negative and positive values:
/// 0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, 2 -> 4, etc.
///
/// - Parameter value: The signed value to encode
/// - Returns: The unsigned zigzag-encoded value
public func zigzagEncode(_ value: Int64) -> UInt64 {
    return UInt64(bitPattern: (value << 1) ^ (value >> 63))
}

/// Decodes a zigzag-encoded unsigned integer back to a signed integer.
///
/// - Parameter value: The unsigned zigzag-encoded value
/// - Returns: The decoded signed value
public func zigzagDecode(_ value: UInt64) -> Int64 {
    return Int64(bitPattern: (value >> 1) ^ (0 &- (value & 1)))
}

/// Encodes a signed 64-bit integer using zigzag + LEB128 varint encoding.
///
/// - Parameter value: The signed value to encode
/// - Returns: The encoded bytes
public func encodeSignedVarint(_ value: Int64) -> [UInt8] {
    return encodeVarint(zigzagEncode(value))
}

/// Decodes a signed 64-bit integer from zigzag + LEB128 varint encoding.
///
/// - Parameter data: The data to decode from. Will be advanced past the varint.
/// - Returns: The decoded signed value
/// - Throws: `VarintError` if decoding fails
public func decodeSignedVarint(from data: inout Data) throws -> Int64 {
    let unsigned = try decodeVarint(from: &data)
    return zigzagDecode(unsigned)
}
