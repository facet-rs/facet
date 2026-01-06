import Foundation

// MARK: - COBS encoding/decoding

/// Encode bytes using COBS (Consistent Overhead Byte Stuffing).
/// r[impl message.cobs]
public func cobsEncode(_ input: [UInt8]) -> [UInt8] {
    var out: [UInt8] = []
    out.reserveCapacity(input.count + 2)

    var codeIndex = 0
    var code: UInt8 = 1
    out.append(0) // placeholder

    for b in input {
        if b == 0 {
            out[codeIndex] = code
            codeIndex = out.count
            out.append(0) // placeholder
            code = 1
        } else {
            out.append(b)
            code &+= 1
            if code == 0xFF {
                out[codeIndex] = code
                codeIndex = out.count
                out.append(0)
                code = 1
            }
        }
    }

    out[codeIndex] = code
    return out
}

/// Decode COBS-encoded bytes.
/// r[impl message.cobs]
public func cobsDecode(_ input: [UInt8]) throws -> [UInt8] {
    var out: [UInt8] = []
    out.reserveCapacity(input.count)

    var i = 0
    while i < input.count {
        let code = input[i]
        i += 1
        guard code != 0 else {
            throw RapaceError.decodeError("cobs: zero code byte")
        }
        let n = Int(code) - 1
        guard i + n <= input.count else {
            throw RapaceError.decodeError("cobs: data overrun")
        }
        if n > 0 {
            out.append(contentsOf: input[i..<(i + n)])
            i += n
        }
        if code != 0xFF && i < input.count {
            out.append(0)
        }
    }

    return out
}
