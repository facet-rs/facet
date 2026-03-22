import Foundation

public struct ResumeKeyBytes: Sendable, Equatable {
    public let bytes: [UInt8]

    public init(bytes: [UInt8]) {
        self.bytes = bytes
    }

    func encodeCbor() -> [UInt8] {
        cborEncodeMapHeader(1)
            + cborEncodeText("bytes")
            + cborEncodeBytes(bytes)
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var value: [UInt8] = []
        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "bytes":
                value = try cborReadBytes(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }
        return .init(bytes: value)
    }
}

struct HandshakeHello: Sendable, Equatable {
    let parity: Parity
    let connectionSettings: ConnectionSettings
    let messagePayloadSchemaCbor: [UInt8]
    let supportsRetry: Bool
    let resumeKey: ResumeKeyBytes?

    func encodeCbor() -> [UInt8] {
        cborEncodeMapHeader(5)
            + cborEncodeText("parity")
            + parity.encodeCbor()
            + cborEncodeText("connection_settings")
            + connectionSettings.encodeCbor()
            + cborEncodeText("message_payload_schema")
            + messagePayloadSchemaCbor
            + cborEncodeText("supports_retry")
            + cborEncodeBool(supportsRetry)
            + cborEncodeText("resume_key")
            + (resumeKey?.encodeCbor() ?? cborEncodeNull())
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var parity: Parity = .odd
        var connectionSettings = ConnectionSettings(parity: .odd, maxConcurrentRequests: 64)
        var messagePayloadSchemaCbor: [UInt8] = []
        var supportsRetry = false
        var resumeKey: ResumeKeyBytes?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "parity":
                parity = try Parity.decodeCbor(bytes, offset: &offset)
            case "connection_settings":
                connectionSettings = try ConnectionSettings.decodeCbor(bytes, offset: &offset)
            case "message_payload_schema":
                messagePayloadSchemaCbor = try cborReadRawValue(bytes, offset: &offset)
            case "supports_retry":
                supportsRetry = try cborReadBool(bytes, offset: &offset)
            case "resume_key":
                if let raw = try cborReadOptionalRawValue(bytes, offset: &offset) {
                    var nestedOffset = 0
                    resumeKey = try ResumeKeyBytes.decodeCbor(raw, offset: &nestedOffset)
                } else {
                    resumeKey = nil
                }
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        return Self(
            parity: parity,
            connectionSettings: connectionSettings,
            messagePayloadSchemaCbor: messagePayloadSchemaCbor,
            supportsRetry: supportsRetry,
            resumeKey: resumeKey
        )
    }
}

struct HandshakeHelloYourself: Sendable, Equatable {
    let connectionSettings: ConnectionSettings
    let messagePayloadSchemaCbor: [UInt8]
    let supportsRetry: Bool
    let resumeKey: ResumeKeyBytes?

    func encodeCbor() -> [UInt8] {
        cborEncodeMapHeader(4)
            + cborEncodeText("connection_settings")
            + connectionSettings.encodeCbor()
            + cborEncodeText("message_payload_schema")
            + messagePayloadSchemaCbor
            + cborEncodeText("supports_retry")
            + cborEncodeBool(supportsRetry)
            + cborEncodeText("resume_key")
            + (resumeKey?.encodeCbor() ?? cborEncodeNull())
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var connectionSettings = ConnectionSettings(parity: .even, maxConcurrentRequests: 64)
        var messagePayloadSchemaCbor: [UInt8] = []
        var supportsRetry = false
        var resumeKey: ResumeKeyBytes?

        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "connection_settings":
                connectionSettings = try ConnectionSettings.decodeCbor(bytes, offset: &offset)
            case "message_payload_schema":
                messagePayloadSchemaCbor = try cborReadRawValue(bytes, offset: &offset)
            case "supports_retry":
                supportsRetry = try cborReadBool(bytes, offset: &offset)
            case "resume_key":
                if let raw = try cborReadOptionalRawValue(bytes, offset: &offset) {
                    var nestedOffset = 0
                    resumeKey = try ResumeKeyBytes.decodeCbor(raw, offset: &nestedOffset)
                } else {
                    resumeKey = nil
                }
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }

        return Self(
            connectionSettings: connectionSettings,
            messagePayloadSchemaCbor: messagePayloadSchemaCbor,
            supportsRetry: supportsRetry,
            resumeKey: resumeKey
        )
    }
}

struct HandshakeLetsGo: Sendable, Equatable {
    func encodeCbor() -> [UInt8] { cborEncodeMapHeader(0) }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        guard count == 0 else {
            for _ in 0..<count {
                _ = try cborReadText(bytes, offset: &offset)
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
            return .init()
        }
        return .init()
    }
}

struct HandshakeSorry: Sendable, Equatable {
    let reason: String

    func encodeCbor() -> [UInt8] {
        cborEncodeMapHeader(1)
            + cborEncodeText("reason")
            + cborEncodeText(reason)
    }

    static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var reason = ""
        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "reason":
                reason = try cborReadText(bytes, offset: &offset)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }
        return .init(reason: reason)
    }
}

enum HandshakeMessage: Sendable, Equatable {
    case hello(HandshakeHello)
    case helloYourself(HandshakeHelloYourself)
    case letsGo(HandshakeLetsGo)
    case sorry(HandshakeSorry)

    func encodeCbor() -> [UInt8] {
        switch self {
        case .hello(let hello):
            return cborEncodeMapHeader(1) + cborEncodeText("Hello") + hello.encodeCbor()
        case .helloYourself(let helloYourself):
            return cborEncodeMapHeader(1) + cborEncodeText("HelloYourself")
                + helloYourself.encodeCbor()
        case .letsGo(let letsGo):
            return cborEncodeMapHeader(1) + cborEncodeText("LetsGo") + letsGo.encodeCbor()
        case .sorry(let sorry):
            return cborEncodeMapHeader(1) + cborEncodeText("Sorry") + sorry.encodeCbor()
        }
    }

    static func decodeCbor(_ bytes: [UInt8]) throws -> Self {
        var offset = 0
        let count = try cborReadMapHeader(bytes, offset: &offset)
        guard count == 1 else {
            throw CborError.invalidType("handshake message must be a single-entry map")
        }
        let tag = try cborReadText(bytes, offset: &offset)
        let rawPayload = try cborReadRawValue(bytes, offset: &offset)
        guard offset == bytes.count else {
            throw CborError.trailingBytes
        }
        var payloadOffset = 0
        switch tag {
        case "Hello":
            return .hello(try HandshakeHello.decodeCbor(rawPayload, offset: &payloadOffset))
        case "HelloYourself":
            return .helloYourself(
                try HandshakeHelloYourself.decodeCbor(rawPayload, offset: &payloadOffset))
        case "LetsGo":
            return .letsGo(try HandshakeLetsGo.decodeCbor(rawPayload, offset: &payloadOffset))
        case "Sorry":
            return .sorry(try HandshakeSorry.decodeCbor(rawPayload, offset: &payloadOffset))
        default:
            throw CborError.invalidType("unknown handshake tag \(tag)")
        }
    }
}

extension Parity {
    fileprivate func encodeCbor() -> [UInt8] {
        switch self {
        case .odd:
            return cborEncodeMapHeader(1) + cborEncodeText("Odd") + cborEncodeNull()
        case .even:
            return cborEncodeMapHeader(1) + cborEncodeText("Even") + cborEncodeNull()
        }
    }

    fileprivate static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        guard count == 1 else {
            throw CborError.invalidType("parity must be a single-entry map")
        }
        let key = try cborReadText(bytes, offset: &offset)
        try cborReadNull(bytes, offset: &offset)
        switch key {
        case "Odd": return .odd
        case "Even": return .even
        default: throw CborError.invalidType("unknown parity variant \(key)")
        }
    }
}

extension ConnectionSettings {
    fileprivate func encodeCbor() -> [UInt8] {
        cborEncodeMapHeader(2)
            + cborEncodeText("parity")
            + parity.encodeCbor()
            + cborEncodeText("max_concurrent_requests")
            + cborEncodeUnsigned(UInt64(maxConcurrentRequests))
    }

    fileprivate static func decodeCbor(_ bytes: [UInt8], offset: inout Int) throws -> Self {
        let count = try cborReadMapHeader(bytes, offset: &offset)
        var parity: Parity = .odd
        var maxConcurrentRequests: UInt32 = 64
        for _ in 0..<count {
            let key = try cborReadText(bytes, offset: &offset)
            switch key {
            case "parity":
                parity = try Parity.decodeCbor(bytes, offset: &offset)
            case "max_concurrent_requests":
                let value = try cborReadUnsigned(bytes, offset: &offset)
                guard value <= UInt64(UInt32.max) else { throw CborError.overflow }
                maxConcurrentRequests = UInt32(value)
            default:
                _ = try cborReadRawValue(bytes, offset: &offset)
            }
        }
        return .init(parity: parity, maxConcurrentRequests: maxConcurrentRequests)
    }
}

func sendHandshake(_ link: any Link, _ message: HandshakeMessage) async throws {
    traceLog(.handshake, "send \(message)")
    try await link.sendRawPrologue(message.encodeCbor())
}

func recvHandshake(_ link: any Link) async throws -> HandshakeMessage {
    traceLog(.handshake, "recv waiting")
    guard let bytes = try await link.recvRawPrologue() else {
        throw ConnectionError.connectionClosed
    }
    let message = try HandshakeMessage.decodeCbor(bytes)
    traceLog(.handshake, "recv got \(message)")
    return message
}

func handshakeMessageSchemasMatch(_ peerSchemasCbor: [UInt8]) -> Bool {
    peerSchemasCbor == wireMessageSchemasCbor
}
