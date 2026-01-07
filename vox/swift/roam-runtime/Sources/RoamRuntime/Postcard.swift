import Foundation

// MARK: - Primitive Encoding

/// Encode a boolean as postcard format (1 byte: 0 or 1)
public func encodeBool(_ value: Bool) -> [UInt8] {
    return [value ? 1 : 0]
}

/// Encode an unsigned 8-bit integer (1 byte, no varint)
public func encodeU8(_ value: UInt8) -> [UInt8] {
    return [value]
}

/// Encode a signed 8-bit integer (1 byte)
public func encodeI8(_ value: Int8) -> [UInt8] {
    return [UInt8(bitPattern: value)]
}

/// Encode an unsigned 16-bit integer as little-endian
public func encodeU16(_ value: UInt16) -> [UInt8] {
    return [UInt8(value & 0xFF), UInt8((value >> 8) & 0xFF)]
}

/// Encode a signed 16-bit integer as little-endian
public func encodeI16(_ value: Int16) -> [UInt8] {
    return encodeU16(UInt16(bitPattern: value))
}

/// Encode an unsigned 32-bit integer as varint
public func encodeU32(_ value: UInt32) -> [UInt8] {
    return encodeVarint(UInt64(value))
}

/// Encode a signed 32-bit integer as zigzag varint
public func encodeI32(_ value: Int32) -> [UInt8] {
    // Zigzag encoding: (n << 1) ^ (n >> 31)
    let zigzag = UInt32(bitPattern: (value << 1) ^ (value >> 31))
    return encodeVarint(UInt64(zigzag))
}

/// Encode a signed 64-bit integer as zigzag varint
public func encodeI64(_ value: Int64) -> [UInt8] {
    // Zigzag encoding: (n << 1) ^ (n >> 63)
    let zigzag = UInt64(bitPattern: (value << 1) ^ (value >> 63))
    return encodeVarint(zigzag)
}

/// Encode an unsigned 128-bit integer (16 bytes, little-endian)
/// Note: Swift UInt128 is available from macOS 15.0+
@available(macOS 15.0, iOS 18.0, watchOS 11.0, tvOS 18.0, visionOS 2.0, *)
public func encodeU128(_ value: UInt128) -> [UInt8] {
    var result = [UInt8](repeating: 0, count: 16)
    var v = value
    for i in 0..<16 {
        result[i] = UInt8(truncatingIfNeeded: v)
        v >>= 8
    }
    return result
}

/// Encode a signed 128-bit integer (16 bytes, little-endian)
@available(macOS 15.0, iOS 18.0, watchOS 11.0, tvOS 18.0, visionOS 2.0, *)
public func encodeI128(_ value: Int128) -> [UInt8] {
    return encodeU128(UInt128(bitPattern: value))
}

/// Encode a 32-bit float (4 bytes, little-endian)
public func encodeF32(_ value: Float) -> [UInt8] {
    let bits = value.bitPattern
    return [
        UInt8(bits & 0xFF),
        UInt8((bits >> 8) & 0xFF),
        UInt8((bits >> 16) & 0xFF),
        UInt8((bits >> 24) & 0xFF),
    ]
}

/// Encode a 64-bit float (8 bytes, little-endian)
public func encodeF64(_ value: Double) -> [UInt8] {
    let bits = value.bitPattern
    var result = [UInt8](repeating: 0, count: 8)
    for i in 0..<8 {
        result[i] = UInt8((bits >> (i * 8)) & 0xFF)
    }
    return result
}

/// Encode a string as postcard format (length-prefixed UTF-8)
public func encodeString(_ s: String) -> [UInt8] {
    let bytes = Array(s.utf8)
    return encodeVarint(UInt64(bytes.count)) + bytes
}

/// Encode bytes as postcard format (length-prefixed)
public func encodeBytes(_ bytes: [UInt8]) -> [UInt8] {
    return encodeVarint(UInt64(bytes.count)) + bytes
}

// MARK: - Container Encoding

/// Encode a vector/array with a length prefix
public func encodeVec<T>(_ items: [T], encoder: (T) -> [UInt8]) -> [UInt8] {
    var result = encodeVarint(UInt64(items.count))
    for item in items {
        result.append(contentsOf: encoder(item))
    }
    return result
}

/// Encode an optional value (0 for None, 1 + value for Some)
public func encodeOption<T>(_ value: T?, encoder: (T) -> [UInt8]) -> [UInt8] {
    if let v = value {
        return [1] + encoder(v)
    } else {
        return [0]
    }
}

// MARK: - Primitive Decoding

/// Decode a boolean from postcard format
public func decodeBool(from data: Data, offset: inout Int) throws -> Bool {
    guard offset < data.count else {
        throw RoamError.decodeError("bool: unexpected EOF")
    }
    let byte = data[offset]
    offset += 1
    switch byte {
    case 0: return false
    case 1: return true
    default: throw RoamError.decodeError("bool: invalid value \(byte)")
    }
}

/// Decode an unsigned 8-bit integer
public func decodeU8(from data: Data, offset: inout Int) throws -> UInt8 {
    guard offset < data.count else {
        throw RoamError.decodeError("u8: unexpected EOF")
    }
    let byte = data[offset]
    offset += 1
    return byte
}

/// Decode a signed 8-bit integer
public func decodeI8(from data: Data, offset: inout Int) throws -> Int8 {
    let u = try decodeU8(from: data, offset: &offset)
    return Int8(bitPattern: u)
}

/// Decode an unsigned 16-bit integer (little-endian)
public func decodeU16(from data: Data, offset: inout Int) throws -> UInt16 {
    guard offset + 2 <= data.count else {
        throw RoamError.decodeError("u16: unexpected EOF")
    }
    let result = UInt16(data[offset]) | (UInt16(data[offset + 1]) << 8)
    offset += 2
    return result
}

/// Decode a signed 16-bit integer (little-endian)
public func decodeI16(from data: Data, offset: inout Int) throws -> Int16 {
    let u = try decodeU16(from: data, offset: &offset)
    return Int16(bitPattern: u)
}

/// Decode an unsigned 32-bit integer (varint)
public func decodeU32(from data: Data, offset: inout Int) throws -> UInt32 {
    let v = try decodeVarint(from: data, offset: &offset)
    guard v <= UInt64(UInt32.max) else {
        throw RoamError.decodeError("u32: overflow")
    }
    return UInt32(v)
}

/// Decode a signed 32-bit integer (zigzag varint)
public func decodeI32(from data: Data, offset: inout Int) throws -> Int32 {
    let v = try decodeU32(from: data, offset: &offset)
    // Zigzag decode: (n >> 1) ^ -(n & 1)
    return Int32(bitPattern: (v >> 1) ^ (0 &- (v & 1)))
}

/// Decode a signed 64-bit integer (zigzag varint)
public func decodeI64(from data: Data, offset: inout Int) throws -> Int64 {
    let v = try decodeVarint(from: data, offset: &offset)
    // Zigzag decode: (n >> 1) ^ -(n & 1)
    return Int64(bitPattern: (v >> 1) ^ (0 &- (v & 1)))
}

/// Decode an unsigned 128-bit integer (16 bytes, little-endian)
@available(macOS 15.0, iOS 18.0, watchOS 11.0, tvOS 18.0, visionOS 2.0, *)
public func decodeU128(from data: Data, offset: inout Int) throws -> UInt128 {
    guard offset + 16 <= data.count else {
        throw RoamError.decodeError("u128: unexpected EOF")
    }
    var result: UInt128 = 0
    for i in 0..<16 {
        result |= UInt128(data[offset + i]) << (i * 8)
    }
    offset += 16
    return result
}

/// Decode a signed 128-bit integer (16 bytes, little-endian)
@available(macOS 15.0, iOS 18.0, watchOS 11.0, tvOS 18.0, visionOS 2.0, *)
public func decodeI128(from data: Data, offset: inout Int) throws -> Int128 {
    let u = try decodeU128(from: data, offset: &offset)
    return Int128(bitPattern: u)
}

/// Decode a 32-bit float (4 bytes, little-endian)
public func decodeF32(from data: Data, offset: inout Int) throws -> Float {
    guard offset + 4 <= data.count else {
        throw RoamError.decodeError("f32: unexpected EOF")
    }
    var bits: UInt32 = 0
    for i in 0..<4 {
        bits |= UInt32(data[offset + i]) << (i * 8)
    }
    offset += 4
    return Float(bitPattern: bits)
}

/// Decode a 64-bit float (8 bytes, little-endian)
public func decodeF64(from data: Data, offset: inout Int) throws -> Double {
    guard offset + 8 <= data.count else {
        throw RoamError.decodeError("f64: unexpected EOF")
    }
    var bits: UInt64 = 0
    for i in 0..<8 {
        bits |= UInt64(data[offset + i]) << (i * 8)
    }
    offset += 8
    return Double(bitPattern: bits)
}

/// Decode a string from postcard format
public func decodeString(from data: Data, offset: inout Int) throws -> String {
    let length = try decodeVarint(from: data, offset: &offset)
    guard offset + Int(length) <= data.count else {
        throw RoamError.decodeError("string: unexpected EOF")
    }
    let bytes = data[offset..<(offset + Int(length))]
    offset += Int(length)
    guard let str = String(bytes: bytes, encoding: .utf8) else {
        throw RoamError.decodeError("string: invalid UTF-8")
    }
    return str
}

/// Decode bytes from postcard format
public func decodeBytes(from data: Data, offset: inout Int) throws -> Data {
    let length = try decodeVarint(from: data, offset: &offset)
    guard offset + Int(length) <= data.count else {
        throw RoamError.decodeError("bytes: unexpected EOF")
    }
    let bytes = data[offset..<(offset + Int(length))]
    offset += Int(length)
    return bytes
}

// MARK: - Container Decoding

/// Decode a vector/array
public func decodeVec<T>(
    from data: Data,
    offset: inout Int,
    decoder: (Data, inout Int) throws -> T
) throws -> [T] {
    let length = try decodeVarint(from: data, offset: &offset)
    var result: [T] = []
    result.reserveCapacity(Int(length))
    for _ in 0..<length {
        result.append(try decoder(data, &offset))
    }
    return result
}

/// Decode an optional value
public func decodeOption<T>(
    from data: Data,
    offset: inout Int,
    decoder: (Data, inout Int) throws -> T
) throws -> T? {
    let tag = try decodeU8(from: data, offset: &offset)
    switch tag {
    case 0: return nil
    case 1: return try decoder(data, &offset)
    default: throw RoamError.decodeError("option: invalid tag \(tag)")
    }
}

/// Decode a 2-tuple
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

// MARK: - Result encoding

/// Encode Result::Ok variant
/// r[impl unary.response.ok]
public func encodeResultOk<T>(_ value: T, encoder: (T) -> [UInt8]) -> [UInt8] {
    var result: [UInt8] = []
    result.append(0)  // Ok discriminant
    result.append(contentsOf: encoder(value))
    return result
}

/// Encode Result::Err variant with RoamError
/// r[impl unary.response.error]
public func encodeResultErr<E>(_ error: CallError<E>, errorEncoder: ((E) -> [UInt8])? = nil)
    -> [UInt8]
{
    var result: [UInt8] = []
    result.append(1)  // Err discriminant

    switch error {
    case .user(let userError):
        result.append(0)  // RoamError::User discriminant
        if let errorEncoder = errorEncoder {
            result.append(contentsOf: errorEncoder(userError))
        }
    case .unknownMethod:
        result.append(1)  // RoamError::UnknownMethod
    case .invalidPayload:
        result.append(2)  // RoamError::InvalidPayload
    case .cancelled:
        result.append(3)  // RoamError::Cancelled
    }

    return result
}

/// Encode Result::Err with UnknownMethod
/// r[impl unary.error.unknown-method]
public func encodeUnknownMethodError() -> [UInt8] {
    return encodeResultErr(CallError<Never>.unknownMethodError)
}

/// Encode Result::Err with InvalidPayload
/// r[impl unary.error.invalid-payload]
public func encodeInvalidPayloadError() -> [UInt8] {
    return encodeResultErr(CallError<Never>.invalidPayloadError)
}

// Never type for errors that can't occur
public enum Never {}
