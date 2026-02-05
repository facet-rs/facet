/// COBS (Consistent Overhead Byte Stuffing) framing.
///
/// r[impl transport.bytestream.cobs] - Messages are COBS-encoded with 0x00 delimiter.
/// r[impl transport.message.binary] - All messages are binary (not text).
/// r[impl transport.message.one-to-one] - Each frame contains exactly one roam message.
///
/// COBS encodes data so that it contains no zero bytes, allowing
/// zero to be used as a frame delimiter.

/// Encode data using COBS.
///
/// The output will contain no zero bytes. A zero byte should be
/// appended as a frame delimiter after sending.
public func cobsEncode(_ data: [UInt8]) -> [UInt8] {
    // Canonical COBS encoding of empty input is a single code byte (0x01).
    guard !data.isEmpty else {
        return [1]
    }

    var output: [UInt8] = []
    output.reserveCapacity(data.count + data.count / 254 + 1)

    var codeIndex = 0
    var code: UInt8 = 1
    output.append(0)  // Placeholder for first code byte

    for byte in data {
        if byte == 0 {
            output[codeIndex] = code
            code = 1
            codeIndex = output.count
            output.append(0)  // Placeholder for next code byte
        } else {
            output.append(byte)
            code += 1
            if code == 0xFF {
                output[codeIndex] = code
                code = 1
                codeIndex = output.count
                output.append(0)  // Placeholder for next code byte
            }
        }
    }

    output[codeIndex] = code
    return output
}

/// Decode COBS-encoded data.
///
/// Input should NOT include the trailing zero delimiter.
public func cobsDecode(_ encoded: [UInt8]) throws -> [UInt8] {
    guard !encoded.isEmpty else {
        return []
    }

    var output: [UInt8] = []
    output.reserveCapacity(encoded.count)

    var i = 0
    while i < encoded.count {
        let code = encoded[i]
        if code == 0 {
            throw COBSError.unexpectedZero
        }

        i += 1
        let copyCount = Int(code) - 1

        if i + copyCount > encoded.count {
            throw COBSError.truncated
        }

        for j in 0..<copyCount {
            let byte = encoded[i + j]
            if byte == 0 {
                throw COBSError.unexpectedZero
            }
            output.append(byte)
        }
        i += copyCount

        // If code < 0xFF, we implicitly have a zero (unless at end)
        if code < 0xFF && i < encoded.count {
            output.append(0)
        }
    }

    return output
}

/// Errors that can occur during COBS decoding.
public enum COBSError: Error {
    case unexpectedZero
    case truncated
}
