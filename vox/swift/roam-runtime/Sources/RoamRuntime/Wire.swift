import Foundation

// MARK: - Hello

/// Hello message for connection handshake.
///
/// r[impl message.hello.structure] - Hello contains version, maxPayloadSize, initialChannelCredit.
/// r[impl message.hello.version] - Version field determines protocol version.
public enum Hello: Sendable {
    case v1(maxPayloadSize: UInt32, initialChannelCredit: UInt32)
    case v2(maxPayloadSize: UInt32, initialChannelCredit: UInt32)
}

extension Hello {
    public var maxPayloadSize: UInt32 {
        switch self {
        case .v1(let size, _), .v2(let size, _):
            return size
        }
    }

    public var initialChannelCredit: UInt32 {
        switch self {
        case .v1(_, let credit), .v2(_, let credit):
            return credit
        }
    }

    public func encode() -> [UInt8] {
        switch self {
        case .v1(let maxPayload, let initialCredit):
            var out: [UInt8] = []
            out += encodeVarint(0)  // V1 discriminant
            out += encodeVarint(UInt64(maxPayload))
            out += encodeVarint(UInt64(initialCredit))
            return out
        case .v2(let maxPayload, let initialCredit):
            var out: [UInt8] = []
            out += encodeVarint(1)  // V2 discriminant
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
        case 1:
            let maxPayload = try decodeVarintU32(from: data, offset: &offset)
            let initialCredit = try decodeVarintU32(from: data, offset: &offset)
            return .v2(maxPayloadSize: maxPayload, initialChannelCredit: initialCredit)
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

/// Wire protocol message types (v2 protocol).
///
/// r[impl wire.message-types] - All wire message types.
/// r[impl core.call] - Request/Response messages implement the call abstraction.
/// r[impl core.call.request-id] - Request ID links requests to responses.
/// r[impl core.channel] - Data/Close/Reset/Credit messages operate on channels.
/// r[impl message.conn-id] - All messages except Hello/Connect/Accept/Reject have conn_id.
/// r[impl call.cancel.no-response-required] - Cancel message indicates no response needed.
public enum Message: Sendable {
    // Discriminant 0: Hello (link control - no conn_id)
    case hello(Hello)

    // Discriminant 1: Connect (virtual connection control - no conn_id)
    /// r[impl message.connect.initiate] - Request a new virtual connection.
    case connect(requestId: UInt64, metadata: [(String, MetadataValue)])

    // Discriminant 2: Accept (virtual connection control - no conn_id)
    /// r[impl message.accept.response] - Accept a virtual connection request.
    case accept(requestId: UInt64, connId: UInt64, metadata: [(String, MetadataValue)])

    // Discriminant 3: Reject (virtual connection control - no conn_id)
    /// r[impl message.reject.response] - Reject a virtual connection request.
    case reject(requestId: UInt64, reason: String, metadata: [(String, MetadataValue)])

    // Discriminant 4: Goodbye (connection control - scoped to conn_id)
    /// r[impl message.goodbye.send] - Close a virtual connection.
    /// r[impl message.goodbye.connection-zero] - Goodbye on conn 0 closes entire link.
    case goodbye(connId: UInt64, reason: String)

    // Discriminant 5: Request (RPC - scoped to conn_id)
    /// r[impl channeling.request.channels] - Channel IDs listed explicitly for proxy support.
    case request(
        connId: UInt64, requestId: UInt64, methodId: UInt64, metadata: [(String, MetadataValue)],
        channels: [UInt64], payload: [UInt8])

    // Discriminant 6: Response (RPC - scoped to conn_id)
    case response(
        connId: UInt64, requestId: UInt64, metadata: [(String, MetadataValue)],
        channels: [UInt64], payload: [UInt8])

    // Discriminant 7: Cancel (RPC - scoped to conn_id)
    case cancel(connId: UInt64, requestId: UInt64)

    // Discriminant 8: Data (channels - scoped to conn_id)
    case data(connId: UInt64, channelId: UInt64, payload: [UInt8])

    // Discriminant 9: Close (channels - scoped to conn_id)
    case close(connId: UInt64, channelId: UInt64)

    // Discriminant 10: Reset (channels - scoped to conn_id)
    case reset(connId: UInt64, channelId: UInt64)

    // Discriminant 11: Credit (channels - scoped to conn_id)
    case credit(connId: UInt64, channelId: UInt64, bytes: UInt32)
}

extension Message {
    /// Encode a message to bytes (without COBS framing).
    public func encode() -> [UInt8] {
        switch self {
        case .hello(let hello):
            return [0] + hello.encode()

        case .connect(let requestId, let metadata):
            var out: [UInt8] = [1]
            out += encodeVarint(requestId)
            out += encodeMetadata(metadata)
            return out

        case .accept(let requestId, let connId, let metadata):
            var out: [UInt8] = [2]
            out += encodeVarint(requestId)
            out += encodeVarint(connId)
            out += encodeMetadata(metadata)
            return out

        case .reject(let requestId, let reason, let metadata):
            var out: [UInt8] = [3]
            out += encodeVarint(requestId)
            out += encodeString(reason)
            out += encodeMetadata(metadata)
            return out

        case .goodbye(let connId, let reason):
            var out: [UInt8] = [4]
            out += encodeVarint(connId)
            out += encodeString(reason)
            return out

        case .request(
            let connId, let requestId, let methodId, let metadata, let channels, let payload):
            var out: [UInt8] = [5]
            out += encodeVarint(connId)
            out += encodeVarint(requestId)
            out += encodeVarint(methodId)
            out += encodeMetadata(metadata)
            // r[impl call.request.channels] - Encode channel IDs as Vec<u64>
            out += encodeVarint(UInt64(channels.count))
            for channelId in channels {
                out += encodeVarint(channelId)
            }
            out += encodeBytes(payload)
            return out

        case .response(let connId, let requestId, let metadata, let channels, let payload):
            var out: [UInt8] = [6]
            out += encodeVarint(connId)
            out += encodeVarint(requestId)
            out += encodeMetadata(metadata)
            // Encode channel IDs as Vec<u64>
            out += encodeVarint(UInt64(channels.count))
            for channelId in channels {
                out += encodeVarint(channelId)
            }
            out += encodeBytes(payload)
            return out

        case .cancel(let connId, let requestId):
            var out: [UInt8] = [7]
            out += encodeVarint(connId)
            out += encodeVarint(requestId)
            return out

        case .data(let connId, let channelId, let payload):
            var out: [UInt8] = [8]
            out += encodeVarint(connId)
            out += encodeVarint(channelId)
            out += encodeBytes(payload)
            return out

        case .close(let connId, let channelId):
            var out: [UInt8] = [9]
            out += encodeVarint(connId)
            out += encodeVarint(channelId)
            return out

        case .reset(let connId, let channelId):
            var out: [UInt8] = [10]
            out += encodeVarint(connId)
            out += encodeVarint(channelId)
            return out

        case .credit(let connId, let channelId, let bytes):
            var out: [UInt8] = [11]
            out += encodeVarint(connId)
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
        case 0:  // Hello
            let hello = try Hello.decode(from: data, offset: &offset)
            return .hello(hello)

        case 1:  // Connect
            let requestId = try decodeVarint(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            return .connect(requestId: requestId, metadata: metadata)

        case 2:  // Accept
            let requestId = try decodeVarint(from: data, offset: &offset)
            let connId = try decodeVarint(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            return .accept(requestId: requestId, connId: connId, metadata: metadata)

        case 3:  // Reject
            let requestId = try decodeVarint(from: data, offset: &offset)
            let reason = try decodeString(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            return .reject(requestId: requestId, reason: reason, metadata: metadata)

        case 4:  // Goodbye
            let connId = try decodeVarint(from: data, offset: &offset)
            let reason = try decodeString(from: data, offset: &offset)
            return .goodbye(connId: connId, reason: reason)

        case 5:  // Request
            let connId = try decodeVarint(from: data, offset: &offset)
            let requestId = try decodeVarint(from: data, offset: &offset)
            let methodId = try decodeVarint(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            // r[impl call.request.channels] - Decode channel IDs as Vec<u64>
            let channelCount = try decodeVarint(from: data, offset: &offset)
            var channels: [UInt64] = []
            channels.reserveCapacity(Int(channelCount))
            for _ in 0..<channelCount {
                let channelId = try decodeVarint(from: data, offset: &offset)
                channels.append(channelId)
            }
            let payload = try decodeBytes(from: data, offset: &offset)
            return .request(
                connId: connId, requestId: requestId, methodId: methodId, metadata: metadata,
                channels: channels, payload: Array(payload))

        case 6:  // Response
            let connId = try decodeVarint(from: data, offset: &offset)
            let requestId = try decodeVarint(from: data, offset: &offset)
            let metadata = try decodeMetadata(from: data, offset: &offset)
            // Decode channel IDs as Vec<u64>
            let channelCount = try decodeVarint(from: data, offset: &offset)
            var channels: [UInt64] = []
            channels.reserveCapacity(Int(channelCount))
            for _ in 0..<channelCount {
                let channelId = try decodeVarint(from: data, offset: &offset)
                channels.append(channelId)
            }
            let payload = try decodeBytes(from: data, offset: &offset)
            return .response(
                connId: connId, requestId: requestId, metadata: metadata,
                channels: channels, payload: Array(payload))

        case 7:  // Cancel
            let connId = try decodeVarint(from: data, offset: &offset)
            let requestId = try decodeVarint(from: data, offset: &offset)
            return .cancel(connId: connId, requestId: requestId)

        case 8:  // Data
            let connId = try decodeVarint(from: data, offset: &offset)
            let channelId = try decodeVarint(from: data, offset: &offset)
            let payload = try decodeBytes(from: data, offset: &offset)
            return .data(connId: connId, channelId: channelId, payload: Array(payload))

        case 9:  // Close
            let connId = try decodeVarint(from: data, offset: &offset)
            let channelId = try decodeVarint(from: data, offset: &offset)
            return .close(connId: connId, channelId: channelId)

        case 10:  // Reset
            let connId = try decodeVarint(from: data, offset: &offset)
            let channelId = try decodeVarint(from: data, offset: &offset)
            return .reset(connId: connId, channelId: channelId)

        case 11:  // Credit
            let connId = try decodeVarint(from: data, offset: &offset)
            let channelId = try decodeVarint(from: data, offset: &offset)
            let bytes = try decodeVarintU32(from: data, offset: &offset)
            return .credit(connId: connId, channelId: channelId, bytes: bytes)

        default:
            throw WireError.unknownMessageVariant
        }
    }
}

// MARK: - Metadata Encoding

/// r[impl core.metadata] - Metadata is key-value pairs attached to requests/responses.
/// r[impl call.metadata.type] - Values can be string, bytes, or integer.
/// r[impl call.metadata.keys] - Keys are UTF-8 strings.
/// r[impl call.metadata.order] - Metadata entries preserve insertion order.
/// r[impl call.metadata.duplicates] - Duplicate keys are allowed.
/// r[impl call.metadata.unknown] - Unknown metadata keys are ignored.
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
