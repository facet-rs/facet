import Foundation

enum CborError: Error, Equatable {
    case truncated
    case invalidType(String)
    case invalidUtf8
    case overflow
    case trailingBytes
}

@inline(__always)
private func cborAppendMajor(_ major: UInt8, value: UInt64, to out: inout [UInt8]) {
    precondition(major < 8)
    switch value {
    case 0...23:
        out.append((major << 5) | UInt8(value))
    case 24...0xff:
        out.append((major << 5) | 24)
        out.append(UInt8(value))
    case 0x100...0xffff:
        out.append((major << 5) | 25)
        out.append(UInt8((value >> 8) & 0xff))
        out.append(UInt8(value & 0xff))
    case 0x1_0000...0xffff_ffff:
        out.append((major << 5) | 26)
        out.append(UInt8((value >> 24) & 0xff))
        out.append(UInt8((value >> 16) & 0xff))
        out.append(UInt8((value >> 8) & 0xff))
        out.append(UInt8(value & 0xff))
    default:
        out.append((major << 5) | 27)
        out.append(UInt8((value >> 56) & 0xff))
        out.append(UInt8((value >> 48) & 0xff))
        out.append(UInt8((value >> 40) & 0xff))
        out.append(UInt8((value >> 32) & 0xff))
        out.append(UInt8((value >> 24) & 0xff))
        out.append(UInt8((value >> 16) & 0xff))
        out.append(UInt8((value >> 8) & 0xff))
        out.append(UInt8(value & 0xff))
    }
}

func cborEncodeUnsigned(_ value: UInt64) -> [UInt8] {
    var out: [UInt8] = []
    cborAppendMajor(0, value: value, to: &out)
    return out
}

func cborEncodeBytes(_ value: [UInt8]) -> [UInt8] {
    var out: [UInt8] = []
    cborAppendMajor(2, value: UInt64(value.count), to: &out)
    out += value
    return out
}

func cborEncodeText(_ value: String) -> [UInt8] {
    let bytes = Array(value.utf8)
    var out: [UInt8] = []
    cborAppendMajor(3, value: UInt64(bytes.count), to: &out)
    out += bytes
    return out
}

func cborEncodeArrayHeader(_ count: Int) -> [UInt8] {
    var out: [UInt8] = []
    cborAppendMajor(4, value: UInt64(count), to: &out)
    return out
}

func cborEncodeMapHeader(_ count: Int) -> [UInt8] {
    var out: [UInt8] = []
    cborAppendMajor(5, value: UInt64(count), to: &out)
    return out
}

func cborEncodeNull() -> [UInt8] { [0xf6] }

func cborEncodeBool(_ value: Bool) -> [UInt8] { [value ? 0xf5 : 0xf4] }

@inline(__always)
private func cborReadUIntArgument(_ bytes: [UInt8], offset: inout Int, additional: UInt8) throws -> UInt64 {
    switch additional {
    case 0...23:
        return UInt64(additional)
    case 24:
        guard offset < bytes.count else { throw CborError.truncated }
        defer { offset += 1 }
        return UInt64(bytes[offset])
    case 25:
        guard offset + 2 <= bytes.count else { throw CborError.truncated }
        let value = (UInt64(bytes[offset]) << 8) | UInt64(bytes[offset + 1])
        offset += 2
        return value
    case 26:
        guard offset + 4 <= bytes.count else { throw CborError.truncated }
        let value =
            (UInt64(bytes[offset]) << 24)
            | (UInt64(bytes[offset + 1]) << 16)
            | (UInt64(bytes[offset + 2]) << 8)
            | UInt64(bytes[offset + 3])
        offset += 4
        return value
    case 27:
        guard offset + 8 <= bytes.count else { throw CborError.truncated }
        var value: UInt64 = 0
        for _ in 0..<8 {
            value = (value << 8) | UInt64(bytes[offset])
            offset += 1
        }
        return value
    default:
        throw CborError.invalidType("unsupported CBOR additional info \(additional)")
    }
}

@inline(__always)
private func cborReadInitial(_ bytes: [UInt8], offset: inout Int) throws -> (major: UInt8, additional: UInt8) {
    guard offset < bytes.count else { throw CborError.truncated }
    let initial = bytes[offset]
    offset += 1
    return (initial >> 5, initial & 0x1f)
}

func cborReadUnsigned(_ bytes: [UInt8], offset: inout Int) throws -> UInt64 {
    let (major, additional) = try cborReadInitial(bytes, offset: &offset)
    guard major == 0 else {
        throw CborError.invalidType("expected unsigned integer")
    }
    return try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
}

func cborReadBytes(_ bytes: [UInt8], offset: inout Int) throws -> [UInt8] {
    let (major, additional) = try cborReadInitial(bytes, offset: &offset)
    guard major == 2 else {
        throw CborError.invalidType("expected byte string")
    }
    let length = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
    guard offset + Int(length) <= bytes.count else { throw CborError.truncated }
    let result = Array(bytes[offset..<(offset + Int(length))])
    offset += Int(length)
    return result
}

func cborReadText(_ bytes: [UInt8], offset: inout Int) throws -> String {
    let (major, additional) = try cborReadInitial(bytes, offset: &offset)
    guard major == 3 else {
        throw CborError.invalidType("expected text string")
    }
    let length = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
    guard offset + Int(length) <= bytes.count else { throw CborError.truncated }
    let slice = Array(bytes[offset..<(offset + Int(length))])
    offset += Int(length)
    guard let value = String(bytes: slice, encoding: .utf8) else {
        throw CborError.invalidUtf8
    }
    return value
}

func cborReadArrayHeader(_ bytes: [UInt8], offset: inout Int) throws -> Int {
    let (major, additional) = try cborReadInitial(bytes, offset: &offset)
    guard major == 4 else {
        throw CborError.invalidType("expected array")
    }
    let count = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
    guard count <= UInt64(Int.max) else { throw CborError.overflow }
    return Int(count)
}

func cborReadMapHeader(_ bytes: [UInt8], offset: inout Int) throws -> Int {
    let (major, additional) = try cborReadInitial(bytes, offset: &offset)
    guard major == 5 else {
        throw CborError.invalidType("expected map")
    }
    let count = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
    guard count <= UInt64(Int.max) else { throw CborError.overflow }
    return Int(count)
}

func cborReadBool(_ bytes: [UInt8], offset: inout Int) throws -> Bool {
    guard offset < bytes.count else { throw CborError.truncated }
    let byte = bytes[offset]
    offset += 1
    switch byte {
    case 0xf4: return false
    case 0xf5: return true
    default: throw CborError.invalidType("expected bool")
    }
}

func cborReadNull(_ bytes: [UInt8], offset: inout Int) throws {
    guard offset < bytes.count else { throw CborError.truncated }
    guard bytes[offset] == 0xf6 else {
        throw CborError.invalidType("expected null")
    }
    offset += 1
}

func cborReadOptionalRawValue(_ bytes: [UInt8], offset: inout Int) throws -> [UInt8]? {
    guard offset < bytes.count else { throw CborError.truncated }
    if bytes[offset] == 0xf6 {
        offset += 1
        return nil
    }
    return try cborReadRawValue(bytes, offset: &offset)
}

func cborReadRawValue(_ bytes: [UInt8], offset: inout Int) throws -> [UInt8] {
    let start = offset
    let (major, additional) = try cborReadInitial(bytes, offset: &offset)
    switch major {
    case 0, 1:
        _ = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
    case 2, 3:
        let length = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
        guard offset + Int(length) <= bytes.count else { throw CborError.truncated }
        offset += Int(length)
    case 4:
        let count = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
        for _ in 0..<count {
            _ = try cborReadRawValue(bytes, offset: &offset)
        }
    case 5:
        let count = try cborReadUIntArgument(bytes, offset: &offset, additional: additional)
        for _ in 0..<count {
            _ = try cborReadRawValue(bytes, offset: &offset)
            _ = try cborReadRawValue(bytes, offset: &offset)
        }
    case 7:
        switch additional {
        case 20, 21, 22:
            break
        case 26:
            guard offset + 4 <= bytes.count else { throw CborError.truncated }
            offset += 4
        case 27:
            guard offset + 8 <= bytes.count else { throw CborError.truncated }
            offset += 8
        default:
            throw CborError.invalidType("unsupported simple value")
        }
    default:
        throw CborError.invalidType("unsupported CBOR major type \(major)")
    }
    return Array(bytes[start..<offset])
}
