import Foundation
import PhonEngine
import PhonIR
import PhonSchema

// Message envelope constructors + self-describing framing, mirroring the TypeScript
// `vox-wire/src/types.ts` (`messageRequest`, `messageData`, …) and `codec.ts`
// (`buildMessageDecoder`, handshake framing). The generated `Wire.swift` /
// `HandshakeWire.swift` carry the types + `encodeMessage`/`decodeMessage`; this file
// is the hand-written glue.

// MARK: - Message constructors

func messageRequest(
    requestId: UInt64,
    methodId: UInt64,
    payload: [UInt8],
    metadata: Metadata = .null,
    channels: [UInt64] = [],
    connectionId: UInt64 = 0,
    schemas: [UInt8] = []
) -> Message {
    // r[impl rpc.request]
    // r[impl session.message]
    // r[impl session.message.connection-id]
    // r[impl session.message.payloads]
    Message(
        connectionId: connectionId,
        payload: .requestMessage(RequestMessage(
            id: requestId,
            body: .call(RequestCall(
                methodId: methodId,
                channels: channels,
                metadata: metadata,
                args: Data(payload),
                schemas: Data(schemas))))))
}

func messageResponse(
    requestId: UInt64,
    payload: [UInt8],
    metadata: Metadata = .null,
    connectionId: UInt64 = 0,
    schemas: [UInt8] = []
) -> Message {
    // r[impl rpc.response]
    // r[impl session.message]
    // r[impl session.message.connection-id]
    // r[impl session.message.payloads]
    Message(
        connectionId: connectionId,
        payload: .requestMessage(RequestMessage(
            id: requestId,
            body: .response(RequestResponse(
                metadata: metadata,
                ret: Data(payload),
                schemas: Data(schemas))))))
}

func messageSchema(
    methodId: UInt64,
    direction: SchemaBindingDirection,
    schemas: [UInt8],
    connectionId: UInt64 = 0
) -> Message {
    let wireDirection: BindingDirection
    switch direction {
    case .args: wireDirection = .args
    case .response: wireDirection = .response
    }
    return Message(
        connectionId: connectionId,
        payload: .schemaMessage(SchemaMessage(
            methodId: methodId,
            direction: wireDirection,
            schemas: Data(schemas))))
}

func messageCancel(
    requestId: UInt64,
    metadata: Metadata = .null,
    connectionId: UInt64 = 0
) -> Message {
    // r[impl rpc.cancel]
    Message(
        connectionId: connectionId,
        payload: .requestMessage(RequestMessage(
            id: requestId,
            body: .cancel(RequestCancel(metadata: metadata)))))
}

func messageConnect(
    connectionId: UInt64,
    settings: ConnectionSettings,
    metadata: Metadata = .null
) -> Message {
    // r[impl session.connection-settings.open]
    Message(
        connectionId: connectionId,
        payload: .laneOpen(LaneOpen(connectionSettings: settings, metadata: metadata)))
}

func messageAccept(
    connectionId: UInt64,
    settings: ConnectionSettings,
    metadata: Metadata = .null
) -> Message {
    // r[impl session.connection-settings.open]
    Message(
        connectionId: connectionId,
        payload: .laneAccept(LaneAccept(connectionSettings: settings, metadata: metadata)))
}

func messageReject(connectionId: UInt64, metadata: Metadata = .null) -> Message {
    // r[impl connection.open.rejection]
    Message(connectionId: connectionId, payload: .laneReject(LaneReject(metadata: metadata)))
}

func messageConnectionClose(connectionId: UInt64, metadata: Metadata = .null) -> Message {
    Message(connectionId: connectionId, payload: .laneClose(LaneClose(metadata: metadata)))
}

func messageData(channelId: UInt64, item: [UInt8], connectionId: UInt64 = 0) -> Message {
    Message(
        connectionId: connectionId,
        payload: .channelMessage(ChannelMessage(id: channelId, body: .item(ChannelItem(item: Data(item))))))
}

func messageChannelClose(channelId: UInt64, connectionId: UInt64 = 0, metadata: Metadata = .null) -> Message {
    Message(
        connectionId: connectionId,
        payload: .channelMessage(ChannelMessage(id: channelId, body: .close(ChannelClose(metadata: metadata)))))
}

func messageChannelReset(channelId: UInt64, connectionId: UInt64 = 0, metadata: Metadata = .null) -> Message {
    Message(
        connectionId: connectionId,
        payload: .channelMessage(ChannelMessage(id: channelId, body: .reset(ChannelReset(metadata: metadata)))))
}

func messageCredit(channelId: UInt64, additional: UInt32, connectionId: UInt64 = 0) -> Message {
    Message(
        connectionId: connectionId,
        payload: .channelMessage(ChannelMessage(id: channelId, body: .grantCredit(ChannelGrantCredit(additional: additional)))))
}

func messageProtocolError(description: String, connectionId: UInt64 = 0) -> Message {
    Message(connectionId: connectionId, payload: .protocolError(ProtocolError(description: description)))
}

func messagePing(nonce: UInt64, connectionId: UInt64 = 0) -> Message {
    Message(connectionId: connectionId, payload: .ping(Ping(nonce: nonce)))
}

func messagePong(nonce: UInt64, connectionId: UInt64 = 0) -> Message {
    Message(connectionId: connectionId, payload: .pong(Pong(nonce: nonce)))
}

// MARK: - Message decoder (writer to reader — the ONE decode path)

/// A decoder that builds from the peer's advertised (writer) Message schema to the
/// local reader. There is no same-schema fast path: when writer ≡ reader the SAME
/// `lowerDecode` degenerates to the fused identity. Not `@Sendable`: it captures a
/// (non-Sendable) `MemProgram`; the conduit that holds it is `@unchecked Sendable` and
/// only invokes it from its own recv loop.
public typealias MessageDecoder = ([UInt8]) throws -> Message

/// Build the Message decoder for a peer from its advertised `message_payload_schema`
/// closure (exchanged in the handshake; always present). The decode ALWAYS uses
/// the peer's writer root against the local Message reader — never a cached local-only
/// program. A missing/unparseable closure yields a decoder that throws (loud), not a
/// same-schema fallback.
/// r[impl conduit.typeplan]
public func buildMessageDecoder(peerMessageSchema: [UInt8]) -> MessageDecoder {
    guard let bundle = try? parseSchemaClosure(peerMessageSchema),
        let program = try? lowerDecode(bundle.root, MessageDescriptor, MessageRegistry.with(bundle.schemas))
    else {
        return { _ in
            throw ConnectionError.handshakeFailed(
                "no peer Message schema for compatibility decode (closure missing/unparseable)")
        }
    }
    let decoder = VoxTypedCodec.compileDecode(program).fn
    return { bytes -> Message in try decodeVoxTyped(decoder, bytes) }
}

// MARK: - Handshake self-describing framing
//
// Each handshake message is one Link frame:
//   [u32 schema_len LE][schema-closure bytes][phon-compact value]

func encodeHandshakeFrame(_ msg: HandshakeMessage) -> [UInt8] {
    let value = encodeHandshakeMessage(msg)
    let closure = HandshakeMessageSchemaClosure
    var out = [UInt8]()
    out.reserveCapacity(4 + closure.count + value.count)
    let len = UInt32(closure.count).littleEndian
    withUnsafeBytes(of: len) { out.append(contentsOf: $0) }
    out.append(contentsOf: closure)
    out.append(contentsOf: value)
    return out
}

func decodeHandshakeFrame(_ bytes: [UInt8]) throws -> HandshakeMessage {
    guard bytes.count >= 4 else { throw ConnectionError.handshakeFailed("handshake frame too short") }
    let len = Int(bytes[0]) | (Int(bytes[1]) << 8) | (Int(bytes[2]) << 16) | (Int(bytes[3]) << 24)
    guard bytes.count >= 4 + len else { throw ConnectionError.handshakeFailed("handshake frame truncated") }
    let closure = Array(bytes[4..<(4 + len)])
    let value = Array(bytes[(4 + len)...])
    // ALWAYS use the writer (closure, always present in the frame) against the
    // local HandshakeMessage reader — the one decode path.
    let bundle = try parseSchemaClosure(closure)
    let reg = HandshakeMessageRegistry.with(bundle.schemas)
    let program = try lowerDecode(bundle.root, HandshakeMessageDescriptor, reg)
    let decoder = VoxTypedCodec.compileDecode(program).fn
    return try decodeVoxTyped(decoder, value)
}
