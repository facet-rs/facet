@preconcurrency import NIOCore

// MARK: - Encoding
//
// r[impl rpc.channel.payload-encoding] - Payloads are Postcard-encoded.

@inline(__always)
public func encodeBool(_ v: Bool, into buffer: inout ByteBuffer) {
    buffer.writeInteger(v ? UInt8(1) : UInt8(0))
}

@inline(__always)
public func encodeU8(_ v: UInt8, into buffer: inout ByteBuffer) {
    buffer.writeInteger(v)
}

@inline(__always)
public func encodeI8(_ v: Int8, into buffer: inout ByteBuffer) {
    buffer.writeInteger(UInt8(bitPattern: v))
}

@inline(__always)
public func encodeU16(_ v: UInt16, into buffer: inout ByteBuffer) {
    encodeVarint(UInt64(v), into: &buffer)
}

@inline(__always)
public func encodeI16(_ v: Int16, into buffer: inout ByteBuffer) {
    let zigzag = UInt16(bitPattern: (v >> 15) ^ (v << 1))
    encodeVarint(UInt64(zigzag), into: &buffer)
}

@inline(__always)
public func encodeU32(_ v: UInt32, into buffer: inout ByteBuffer) {
    encodeVarint(UInt64(v), into: &buffer)
}

@inline(__always)
public func encodeI32(_ v: Int32, into buffer: inout ByteBuffer) {
    let zigzag = UInt32(bitPattern: (v >> 31) ^ (v << 1))
    encodeVarint(UInt64(zigzag), into: &buffer)
}

@inline(__always)
public func encodeU64(_ v: UInt64, into buffer: inout ByteBuffer) {
    encodeVarint(v, into: &buffer)
}

@inline(__always)
public func encodeI64(_ v: Int64, into buffer: inout ByteBuffer) {
    let zigzag = UInt64(bitPattern: (v >> 63) ^ (v << 1))
    encodeVarint(zigzag, into: &buffer)
}

@inline(__always)
public func encodeF32(_ v: Float, into buffer: inout ByteBuffer) {
    buffer.writeInteger(v.bitPattern, endianness: .little)
}

@inline(__always)
public func encodeF64(_ v: Double, into buffer: inout ByteBuffer) {
    buffer.writeInteger(v.bitPattern, endianness: .little)
}

@inline(__always)
public func encodeString(_ s: String, into buffer: inout ByteBuffer) {
    let utf8 = s.utf8
    encodeVarint(UInt64(utf8.count), into: &buffer)
    buffer.writeBytes(utf8)
}

/// Encode raw bytes (a ByteBuffer slice) with a varint length prefix.
@inline(__always)
public func encodeBytes(_ bytes: ByteBuffer, into buffer: inout ByteBuffer) {
    encodeVarint(UInt64(bytes.readableBytes), into: &buffer)
    var copy = bytes
    buffer.writeBuffer(&copy)
}

/// Encode a sequence of bytes with a varint length prefix.
@inline(__always)
public func encodeByteSeq(_ bytes: some Collection<UInt8>, into buffer: inout ByteBuffer) {
    encodeVarint(UInt64(bytes.count), into: &buffer)
    buffer.writeBytes(bytes)
}

@inline(__always)
public func encodeOption<T>(
    _ value: T?, into buffer: inout ByteBuffer, encoder: (T, inout ByteBuffer) -> Void
) {
    if let v = value {
        buffer.writeInteger(UInt8(1))
        encoder(v, &buffer)
    } else {
        buffer.writeInteger(UInt8(0))
    }
}

@inline(__always)
public func encodeVec<T>(
    _ values: [T], into buffer: inout ByteBuffer, encoder: (T, inout ByteBuffer) -> Void
) {
    encodeVarint(UInt64(values.count), into: &buffer)
    for v in values {
        encoder(v, &buffer)
    }
}

// MARK: - Decoding

@inline(__always)
public func decodeBool(from buffer: inout ByteBuffer) throws -> Bool {
    guard let v: UInt8 = buffer.readInteger() else { throw PostcardError.truncated }
    return v != 0
}

@inline(__always)
public func decodeU8(from buffer: inout ByteBuffer) throws -> UInt8 {
    guard let v: UInt8 = buffer.readInteger() else { throw PostcardError.truncated }
    return v
}

@inline(__always)
public func decodeI8(from buffer: inout ByteBuffer) throws -> Int8 {
    guard let v: UInt8 = buffer.readInteger() else { throw PostcardError.truncated }
    return Int8(bitPattern: v)
}

@inline(__always)
public func decodeU16(from buffer: inout ByteBuffer) throws -> UInt16 {
    let v = try decodeVarint(from: &buffer)
    guard v <= UInt64(UInt16.max) else { throw PostcardError.overflow }
    return UInt16(v)
}

@inline(__always)
public func decodeI16(from buffer: inout ByteBuffer) throws -> Int16 {
    let zigzag = try decodeVarint(from: &buffer)
    let unsigned = UInt16(truncatingIfNeeded: zigzag)
    return Int16(bitPattern: (unsigned >> 1) ^ (0 &- (unsigned & 1)))
}

@inline(__always)
public func decodeU32(from buffer: inout ByteBuffer) throws -> UInt32 {
    let v = try decodeVarint(from: &buffer)
    guard v <= UInt64(UInt32.max) else { throw PostcardError.overflow }
    return UInt32(v)
}

@inline(__always)
public func decodeI32(from buffer: inout ByteBuffer) throws -> Int32 {
    let zigzag = try decodeVarint(from: &buffer)
    let unsigned = UInt32(truncatingIfNeeded: zigzag)
    return Int32(bitPattern: (unsigned >> 1) ^ (0 &- (unsigned & 1)))
}

@inline(__always)
public func decodeU64(from buffer: inout ByteBuffer) throws -> UInt64 {
    try decodeVarint(from: &buffer)
}

@inline(__always)
public func decodeI64(from buffer: inout ByteBuffer) throws -> Int64 {
    let zigzag = try decodeVarint(from: &buffer)
    return Int64(bitPattern: (zigzag >> 1) ^ (0 &- (zigzag & 1)))
}

@inline(__always)
public func decodeF32(from buffer: inout ByteBuffer) throws -> Float {
    guard let bits: UInt32 = buffer.readInteger(endianness: .little) else {
        throw PostcardError.truncated
    }
    return Float(bitPattern: bits)
}

@inline(__always)
public func decodeF64(from buffer: inout ByteBuffer) throws -> Double {
    guard let bits: UInt64 = buffer.readInteger(endianness: .little) else {
        throw PostcardError.truncated
    }
    return Double(bitPattern: bits)
}

@inline(__always)
public func decodeString(from buffer: inout ByteBuffer) throws -> String {
    let len = try decodeVarint(from: &buffer)
    guard let bytes = buffer.readBytes(length: Int(len)) else { throw PostcardError.truncated }
    guard let s = String(bytes: bytes, encoding: .utf8) else { throw PostcardError.invalidUtf8 }
    return s
}

/// Decode a length-prefixed byte sequence, returning a ByteBuffer slice (no copy).
@inline(__always)
public func decodeBytes(from buffer: inout ByteBuffer) throws -> ByteBuffer {
    let len = try decodeVarint(from: &buffer)
    guard let slice = buffer.readSlice(length: Int(len)) else { throw PostcardError.truncated }
    return slice
}

@inline(__always)
public func decodeOption<T>(
    from buffer: inout ByteBuffer,
    decoder: (inout ByteBuffer) throws -> T
) throws -> T? {
    guard let tag: UInt8 = buffer.readInteger() else { throw PostcardError.truncated }
    if tag == 0 { return nil }
    return try decoder(&buffer)
}

@inline(__always)
public func decodeVec<T>(
    from buffer: inout ByteBuffer,
    decoder: (inout ByteBuffer) throws -> T
) throws -> [T] {
    let count = try decodeVarint(from: &buffer)
    var result: [T] = []
    result.reserveCapacity(Int(count))
    for _ in 0..<count {
        result.append(try decoder(&buffer))
    }
    return result
}

@inline(__always)
public func decodeTuple2<A, B>(
    from buffer: inout ByteBuffer,
    decoderA: (inout ByteBuffer) throws -> A,
    decoderB: (inout ByteBuffer) throws -> B
) throws -> (A, B) {
    let a = try decoderA(&buffer)
    let b = try decoderB(&buffer)
    return (a, b)
}

// MARK: - Errors

public enum PostcardError: Error {
    case truncated
    case invalidUtf8
    case unknownVariant
    case overflow
}

// MARK: - RPC Result Decoding

/// RPC error codes matching the Vox spec.
/// r[impl rpc.fallible.vox-error] - VoxError wraps call results.
/// r[impl session.protocol-error] - Protocol errors use discriminants 1-3.
public enum RpcErrorCode: UInt8, Sendable {
    /// User-defined application error
    case user = 0
    /// r[impl rpc.unknown-method] - Method ID not recognized
    case unknownMethod = 1
    /// r[impl rpc.error.scope] - Request payload deserialization failed
    case invalidPayload = 2
    /// Call was cancelled
    case cancelled = 3
    /// Runtime refused to guess after recovery
    case indeterminate = 4
}

// MARK: - Response Envelope Decoding

/// Decode a response for an infallible method (one that cannot return a user-level error).
///
/// The wire envelope is:
///   - varint discriminant: 0 = success, 1 = VoxError
///   - If 0: success payload, decoded by `decode`
///   - If 1: u8 VoxError code (system errors only; code 0 throws decodeError)
@inline(__always)
public func decodeInfallibleResponse<T>(
    _ response: [UInt8],
    decode: (inout ByteBuffer) throws -> T
) throws -> T {
    var buf = ByteBufferAllocator().buffer(capacity: response.count)
    buf.writeBytes(response)
    let resultDisc = try decodeVarint(from: &buf)
    switch resultDisc {
    case 0:
        return try decode(&buf)
    case 1:
        let errorCode = try decodeU8(from: &buf)
        switch errorCode {
        case 0:
            throw VoxError.decodeError("unexpected user error for infallible method")
        case 1:
            throw VoxError.unknownMethod
        case 2:
            throw VoxError.decodeError("invalid payload")
        case 3:
            throw VoxError.cancelled
        case 4:
            throw VoxError.indeterminate
        default:
            throw VoxError.decodeError("invalid VoxError discriminant: \(errorCode)")
        }
    default:
        throw VoxError.decodeError("invalid Result discriminant: \(resultDisc)")
    }
}

/// Decode a response for a fallible method (one that can return a user-level error).
///
/// The wire envelope is:
///   - varint discriminant: 0 = success, 1 = VoxError
///   - If 0: success payload, decoded by `decodeOk`
///   - If 1: u8 VoxError code; code 0 (user error) is decoded by `decodeErr`
@inline(__always)
public func decodeFallibleResponse<T, E>(
    _ response: [UInt8],
    decodeOk: (inout ByteBuffer) throws -> T,
    decodeErr: (inout ByteBuffer) throws -> E
) throws -> Result<T, E> {
    var buf = ByteBufferAllocator().buffer(capacity: response.count)
    buf.writeBytes(response)
    let resultDisc = try decodeVarint(from: &buf)
    switch resultDisc {
    case 0:
        return .success(try decodeOk(&buf))
    case 1:
        let errorCode = try decodeU8(from: &buf)
        switch errorCode {
        case 0:
            return .failure(try decodeErr(&buf))
        case 1:
            throw VoxError.unknownMethod
        case 2:
            throw VoxError.decodeError("invalid payload")
        case 3:
            throw VoxError.cancelled
        case 4:
            throw VoxError.indeterminate
        default:
            throw VoxError.decodeError("invalid VoxError discriminant: \(errorCode)")
        }
    default:
        throw VoxError.decodeError("invalid Result discriminant: \(resultDisc)")
    }
}
