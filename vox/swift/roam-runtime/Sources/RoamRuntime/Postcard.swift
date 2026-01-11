import Foundation

// MARK: - Encoding

public func encodeBool(_ v: Bool) -> [UInt8] { [v ? 1 : 0] }
public func encodeU8(_ v: UInt8) -> [UInt8] { [v] }
public func encodeI8(_ v: Int8) -> [UInt8] { [UInt8(bitPattern: v)] }

public func encodeU16(_ v: UInt16) -> [UInt8] {
    // Postcard uses varint for u16
    encodeVarint(UInt64(v))
}

public func encodeI16(_ v: Int16) -> [UInt8] {
    // Postcard uses zigzag + varint for signed integers
    let zigzag = UInt16(bitPattern: (v >> 15) ^ (v << 1))
    return encodeVarint(UInt64(zigzag))
}

public func encodeU32(_ v: UInt32) -> [UInt8] {
    // Postcard uses varint for u32
    encodeVarint(UInt64(v))
}

public func encodeI32(_ v: Int32) -> [UInt8] {
    // Postcard uses zigzag + varint for signed integers
    let zigzag = UInt32(bitPattern: (v >> 31) ^ (v << 1))
    return encodeVarint(UInt64(zigzag))
}

public func encodeU64(_ v: UInt64) -> [UInt8] {
    encodeVarint(v)
}

public func encodeI64(_ v: Int64) -> [UInt8] {
    // Zigzag encoding for signed
    let zigzag = UInt64(bitPattern: (v >> 63) ^ (v << 1))
    return encodeVarint(zigzag)
}

public func encodeF32(_ v: Float) -> [UInt8] {
    withUnsafeBytes(of: v.bitPattern.littleEndian) { Array($0) }
}

public func encodeF64(_ v: Double) -> [UInt8] {
    withUnsafeBytes(of: v.bitPattern.littleEndian) { Array($0) }
}

public func encodeString(_ s: String) -> [UInt8] {
    let bytes = Array(s.utf8)
    return encodeVarint(UInt64(bytes.count)) + bytes
}

public func encodeBytes(_ bytes: [UInt8]) -> [UInt8] {
    encodeVarint(UInt64(bytes.count)) + bytes
}

public func encodeOption<T>(_ value: T?, encoder: (T) -> [UInt8]) -> [UInt8] {
    if let v = value {
        return [1] + encoder(v)
    } else {
        return [0]
    }
}

public func encodeVec<T>(_ values: [T], encoder: (T) -> [UInt8]) -> [UInt8] {
    var result = encodeVarint(UInt64(values.count))
    for v in values {
        result += encoder(v)
    }
    return result
}

// MARK: - Decoding

public func decodeBool(from data: Data, offset: inout Int) throws -> Bool {
    guard offset < data.count else { throw PostcardError.truncated }
    let v = data[data.startIndex + offset]
    offset += 1
    return v != 0
}

public func decodeU8(from data: Data, offset: inout Int) throws -> UInt8 {
    guard offset < data.count else { throw PostcardError.truncated }
    let v = data[data.startIndex + offset]
    offset += 1
    return v
}

public func decodeI8(from data: Data, offset: inout Int) throws -> Int8 {
    guard offset < data.count else { throw PostcardError.truncated }
    let v = data[data.startIndex + offset]
    offset += 1
    return Int8(bitPattern: v)
}

public func decodeU16(from data: Data, offset: inout Int) throws -> UInt16 {
    guard offset + 2 <= data.count else { throw PostcardError.truncated }
    let v = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + 2))
        .withUnsafeBytes { $0.load(as: UInt16.self) }
    offset += 2
    return UInt16(littleEndian: v)
}

public func decodeI16(from data: Data, offset: inout Int) throws -> Int16 {
    guard offset + 2 <= data.count else { throw PostcardError.truncated }
    let v = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + 2))
        .withUnsafeBytes { $0.load(as: Int16.self) }
    offset += 2
    return Int16(littleEndian: v)
}

public func decodeU32(from data: Data, offset: inout Int) throws -> UInt32 {
    guard offset + 4 <= data.count else { throw PostcardError.truncated }
    let v = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + 4))
        .withUnsafeBytes { $0.load(as: UInt32.self) }
    offset += 4
    return UInt32(littleEndian: v)
}

public func decodeI32(from data: Data, offset: inout Int) throws -> Int32 {
    // Postcard uses zigzag + varint for signed integers
    let zigzag = try decodeVarint(from: data, offset: &offset)
    let unsigned = UInt32(truncatingIfNeeded: zigzag)
    return Int32(bitPattern: (unsigned >> 1) ^ (0 &- (unsigned & 1)))
}

public func decodeU64(from data: Data, offset: inout Int) throws -> UInt64 {
    try decodeVarint(from: data, offset: &offset)
}

public func decodeI64(from data: Data, offset: inout Int) throws -> Int64 {
    // Zigzag decoding
    let zigzag = try decodeVarint(from: data, offset: &offset)
    return Int64(bitPattern: (zigzag >> 1) ^ (0 &- (zigzag & 1)))
}

public func decodeF32(from data: Data, offset: inout Int) throws -> Float {
    guard offset + 4 <= data.count else { throw PostcardError.truncated }
    let bits = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + 4))
        .withUnsafeBytes { $0.load(as: UInt32.self) }
    offset += 4
    return Float(bitPattern: UInt32(littleEndian: bits))
}

public func decodeF64(from data: Data, offset: inout Int) throws -> Double {
    guard offset + 8 <= data.count else { throw PostcardError.truncated }
    let bits = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + 8))
        .withUnsafeBytes { $0.load(as: UInt64.self) }
    offset += 8
    return Double(bitPattern: UInt64(littleEndian: bits))
}

public func decodeString(from data: Data, offset: inout Int) throws -> String {
    let len = try decodeVarint(from: data, offset: &offset)
    guard offset + Int(len) <= data.count else { throw PostcardError.truncated }
    let bytes = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + Int(len)))
    offset += Int(len)
    guard let s = String(data: bytes, encoding: .utf8) else {
        throw PostcardError.invalidUtf8
    }
    return s
}

public func decodeBytes(from data: Data, offset: inout Int) throws -> Data {
    let len = try decodeVarint(from: data, offset: &offset)
    guard offset + Int(len) <= data.count else { throw PostcardError.truncated }
    let bytes = data.subdata(in: (data.startIndex + offset)..<(data.startIndex + offset + Int(len)))
    offset += Int(len)
    return bytes
}

public func decodeOption<T>(
    from data: Data,
    offset: inout Int,
    decoder: (Data, inout Int) throws -> T
) throws -> T? {
    let tag = try decodeU8(from: data, offset: &offset)
    if tag == 0 {
        return nil
    } else {
        return try decoder(data, &offset)
    }
}

public func decodeVec<T>(
    from data: Data,
    offset: inout Int,
    decoder: (Data, inout Int) throws -> T
) throws -> [T] {
    let count = try decodeVarint(from: data, offset: &offset)
    var result: [T] = []
    result.reserveCapacity(Int(count))
    for _ in 0..<count {
        result.append(try decoder(data, &offset))
    }
    return result
}

public func decodeTuple2<A, B>(
    from data: Data,
    offset: inout Int,
    decoderA: (Data, inout Int) throws -> A,
    decoderB: (Data, inout Int) throws -> B
) throws -> (A, B) {
    let a = try decoderA(data, &offset)
    let b = try decoderB(data, &offset)
    return (a, b)
}

// MARK: - Errors

public enum PostcardError: Error {
    case truncated
    case invalidUtf8
    case unknownVariant
}
