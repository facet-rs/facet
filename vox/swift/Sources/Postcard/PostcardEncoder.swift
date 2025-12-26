import Foundation

/// Errors that can occur during postcard encoding
public enum PostcardEncoderError: Error {
    case unsupportedType(String)
    case stringEncodingFailed
}

/// A buffer for building postcard-encoded data
public struct PostcardEncoder {
    private var buffer: [UInt8] = []

    public init() {}

    /// Get the encoded bytes
    public var bytes: [UInt8] { buffer }

    /// Get the encoded data
    public var data: Data { Data(buffer) }

    /// Reset the encoder for reuse
    public mutating func reset() {
        buffer.removeAll(keepingCapacity: true)
    }

    // MARK: - Primitive Types

    /// Encode a boolean (1 byte: 0x00 or 0x01)
    public mutating func encode(_ value: Bool) {
        buffer.append(value ? 0x01 : 0x00)
    }

    /// Encode a UInt8 (1 byte, raw)
    public mutating func encode(_ value: UInt8) {
        buffer.append(value)
    }

    /// Encode an Int8 (1 byte, two's complement)
    public mutating func encode(_ value: Int8) {
        buffer.append(UInt8(bitPattern: value))
    }

    /// Encode a UInt16 (varint)
    public mutating func encode(_ value: UInt16) {
        appendVarint(UInt64(value))
    }

    /// Encode an Int16 (zigzag + varint)
    public mutating func encode(_ value: Int16) {
        appendSignedVarint(Int64(value))
    }

    /// Encode a UInt32 (varint)
    public mutating func encode(_ value: UInt32) {
        appendVarint(UInt64(value))
    }

    /// Encode an Int32 (zigzag + varint)
    public mutating func encode(_ value: Int32) {
        appendSignedVarint(Int64(value))
    }

    /// Encode a UInt64 (varint)
    public mutating func encode(_ value: UInt64) {
        appendVarint(value)
    }

    /// Encode an Int64 (zigzag + varint)
    public mutating func encode(_ value: Int64) {
        appendSignedVarint(value)
    }

    /// Encode a Float32 (4 bytes, little-endian)
    public mutating func encode(_ value: Float) {
        var v = value
        withUnsafeBytes(of: &v) { ptr in
            buffer.append(contentsOf: ptr)
        }
    }

    /// Encode a Float64 (8 bytes, little-endian)
    public mutating func encode(_ value: Double) {
        var v = value
        withUnsafeBytes(of: &v) { ptr in
            buffer.append(contentsOf: ptr)
        }
    }

    // MARK: - String and Bytes

    /// Encode a String (varint length + UTF-8 bytes)
    public mutating func encode(_ value: String) {
        let utf8 = Array(value.utf8)
        appendVarint(UInt64(utf8.count))
        buffer.append(contentsOf: utf8)
    }

    /// Encode raw bytes (varint length + bytes)
    public mutating func encode(_ value: [UInt8]) {
        appendVarint(UInt64(value.count))
        buffer.append(contentsOf: value)
    }

    /// Encode Data (varint length + bytes)
    public mutating func encode(_ value: Data) {
        appendVarint(UInt64(value.count))
        buffer.append(contentsOf: value)
    }

    // MARK: - Optional

    /// Encode an optional value
    public mutating func encode<T>(_ value: T?, using encode: (inout PostcardEncoder, T) -> Void) {
        if let v = value {
            buffer.append(0x01)  // Some
            encode(&self, v)
        } else {
            buffer.append(0x00)  // None
        }
    }

    // MARK: - Sequences

    /// Encode an array (varint length + elements)
    public mutating func encode<T>(_ values: [T], using encode: (inout PostcardEncoder, T) -> Void) {
        appendVarint(UInt64(values.count))
        for value in values {
            encode(&self, value)
        }
    }

    /// Encode an array of strings
    public mutating func encode(_ values: [String]) {
        appendVarint(UInt64(values.count))
        for value in values {
            encode(value)
        }
    }

    // MARK: - Varint Helpers

    private mutating func appendVarint(_ value: UInt64) {
        buffer.append(contentsOf: Postcard.encodeVarint(value))
    }

    private mutating func appendSignedVarint(_ value: Int64) {
        buffer.append(contentsOf: Postcard.encodeSignedVarint(value))
    }
}

// MARK: - Protocol for encodable types

/// Protocol for types that can be encoded to postcard format
public protocol PostcardEncodable {
    func encode(to encoder: inout PostcardEncoder)
}

// MARK: - Convenience extensions

extension PostcardEncoder {
    /// Encode a PostcardEncodable value
    public mutating func encode(_ value: PostcardEncodable) {
        value.encode(to: &self)
    }

    /// Encode a value and return the bytes
    public static func encode(_ value: PostcardEncodable) -> [UInt8] {
        var encoder = PostcardEncoder()
        value.encode(to: &encoder)
        return encoder.bytes
    }
}

// MARK: - Standard type conformances

extension Bool: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension UInt8: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension Int8: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension UInt16: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension Int16: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension UInt32: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension Int32: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension UInt64: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension Int64: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension Float: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension Double: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}

extension String: PostcardEncodable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(self)
    }
}
