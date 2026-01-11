import Foundation

// MARK: - Hello

/// Hello message for connection handshake.
public enum Hello: Sendable {
    case v1(maxPayloadSize: UInt32, initialChannelCredit: UInt32)
}

extension Hello {
    public func encode() -> [UInt8] {
        switch self {
        case .v1(let maxPayload, let initialCredit):
            var out: [UInt8] = []
            out += encodeVarint(0)  // V1 discriminant
            out += encodeVarint(UInt64(maxPayload))
            out += encodeVarint(UInt64(initialCredit))
            return out
        }
    }

    public static func decode(from data: Data, offset: inout Int) throws -> Hello {
        let disc = try decodeVarint(from: data, offset: &offset)
        switch disc {
        case 0:
            let maxPayload = try decodeVarintU32(from: data, offset: &offset)
            let initialCredit = try decodeVarintU32(from: data, offset: &offset)
            return .v1(maxPayloadSize: maxPayload, initialChannelCredit: initialCredit)
        default:
            throw WireError.unknownHelloVariant
        }
    }
}

// MARK: - MetadataValue

/// Value in a metadata entry.
public enum MetadataValue: Sendable {
    case string(String)
    case bytes([UInt8])
    case integer(Int64)
}

// MARK: - Message

/// Wire protocol message types.
public enum Message: Sendable {
    case hello(Hello)
    case goodbye(reason: String)
    case request(
        requestId: UInt64, methodId: UInt64, metadata: [(String, MetadataValue)], payload: [UInt8])
    case response(requestId: UInt64, metadata: [(String, MetadataValue)], payload: [UInt8])
    case cancel(requestId: UInt64)
    case data(channelId: UInt64, payload: [UInt8])
    case close(channelId: UInt64)
    case reset(channelId: UInt64)
    case credit(channelId: UInt64, bytes: UInt32)
}

extension Message {
    /// Encode a message to bytes (without COBS framing).
    public func encode() -> [UInt8] {
        switch self {
        case .hello(let hello):
            return [0] + hello.encode()

        case .goodbye(let reason):
            return [1] + encodeString(reason)

        case .request(let requestId, let methodId, let metadata, let payload):
            var out: [UInt8] = [2]
            out += encodeVarint(requestId)
            out += encodeVarint(methodId)
            out += encodeMetadata(metadata)
            out += encodeBytes(payload)
            return out

        case .response(let requestId, let metadata, let payload):
            var out: [UInt8] = [3]
            out += encodeVarint(requestId)
            out += encodeMetadata(metadata)
            out += encodeBytes(payload)
            return out

        case .cancel(let requestId):
            var out: [UInt8] = [4]
            out += encodeVarint(requestId)
            return out

        case .data(let channelId, let payload):
            var out: [UInt8] = [5]
            out += encodeVarint(channelId)
            out += encodeBytes(payload)
            return out

        case .close(let channelId):
            var out: [UInt8] = [6]
            out += encodeVarint(channelId)
            return out

        case .reset(let channelId):
            var out: [UInt8] = [7]
            out += encodeVarint(channelId)
            return out

        case .credit(let channelId, let bytes):
            var out: [UInt8] = [8]
            out += encodeVarint(channelId)
            out += encodeVarint(UInt64(bytes))
            return out
        }
    }

    /// Decode a message from bytes (without COBS framing).
    public static func decode(from data: Data) throws -> Message {
        guard !data.isEmpty else {
            throw WireError.truncated
        }

        var offset = 0
        let disc = try decodeU8(from: data, offset: &offset)

        switch disc {
        case 0:
            let hello = try Hello.decode(from: data, offset: &offset)
            return .hello(hello)

        case 1:
            let reason = try decodeString(from: data, offset: &offset)
            return .goodbye(reason: reason)

        case 2:
            let requestId = try decodeVarint(from: data, offset: &offset)
            let methodId = try decodeVarint(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            let payload = try decodeBytes(from: data, offset: &offset)
            return .request(
                requestId: requestId, methodId: methodId, metadata: metadata,
                payload: Array(payload))

        case 3:
            let requestId = try decodeVarint(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            let payload = try decodeBytes(from: data, offset: &offset)
            return .response(requestId: requestId, metadata: metadata, payload: Array(payload))

        case 4:
            let requestId = try decodeVarint(from: data, offset: &offset)
            return .cancel(requestId: requestId)

        case 5:
            let channelId = try decodeVarint(from: data, offset: &offset)
            let payload = try decodeBytes(from: data, offset: &offset)
            return .data(channelId: channelId, payload: Array(payload))

        case 6:
            let channelId = try decodeVarint(from: data, offset: &offset)
            return .close(channelId: channelId)

        case 7:
            let channelId = try decodeVarint(from: data, offset: &offset)
            return .reset(channelId: channelId)

        case 8:
            let channelId = try decodeVarint(from: data, offset: &offset)
            let bytes = try decodeVarintU32(from: data, offset: &offset)
            return .credit(channelId: channelId, bytes: bytes)

        default:
            throw WireError.unknownMessageVariant
        }
    }
}

// MARK: - Metadata Encoding

func encodeMetadata(_ metadata: [(String, MetadataValue)]) -> [UInt8] {
    var out = encodeVarint(UInt64(metadata.count))
    for (key, value) in metadata {
        out += encodeString(key)
        switch value {
        case .string(let s):
            out += [0] + encodeString(s)
        case .bytes(let b):
            out += [1] + encodeBytes(b)
        case .integer(let i):
            out += [2] + encodeI64(i)
        }
    }
    return out
}

func decodeMetadata(from data: Data, offset: inout Int) throws -> [(String, MetadataValue)] {
    let count = try decodeVarint(from: data, offset: &offset)
    var result: [(String, MetadataValue)] = []
    for _ in 0..<count {
        let key = try decodeString(from: data, offset: &offset)
        let valueDisc = try decodeU8(from: data, offset: &offset)
        let value: MetadataValue
        switch valueDisc {
        case 0:
            value = .string(try decodeString(from: data, offset: &offset))
        case 1:
            value = .bytes(Array(try decodeBytes(from: data, offset: &offset)))
        case 2:
            value = .integer(try decodeI64(from: data, offset: &offset))
        default:
            throw WireError.unknownMetadataVariant
        }
        result.append((key, value))
    }
    return result
}

// MARK: - Errors

public enum WireError: Error, Equatable {
    case truncated
    case unknownHelloVariant
    case unknownMessageVariant
    case unknownMetadataVariant
}
