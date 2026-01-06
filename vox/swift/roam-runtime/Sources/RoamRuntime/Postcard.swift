import Foundation

// MARK: - Postcard encoding helpers (subset for RPC)

/// Encode a string as postcard format (length-prefixed UTF-8)
public func encodeString(_ s: String) -> [UInt8] {
    let bytes = Array(s.utf8)
    return encodeVarint(UInt64(bytes.count)) + bytes
}

/// Encode bytes as postcard format (length-prefixed)
public func encodeBytes(_ bytes: [UInt8]) -> [UInt8] {
    return encodeVarint(UInt64(bytes.count)) + bytes
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
