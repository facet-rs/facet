import Foundation

public enum ShmTransportConvertError: Error, Equatable {
    case unknownMsgType(UInt8)
    case decodeError(String)
    case helloNotSupported
    case creditNotSupported
    case idOutOfRange(UInt64)
}

enum ShmMsgType {
    static let request: UInt8 = 1
    static let response: UInt8 = 2
    static let cancel: UInt8 = 3
    static let data: UInt8 = 4
    static let close: UInt8 = 5
    static let reset: UInt8 = 6
    static let goodbye: UInt8 = 7
    static let connect: UInt8 = 8
    static let accept: UInt8 = 9
    static let reject: UInt8 = 10
}

func messageToShmFrame(_ msg: Message) throws -> ShmGuestFrame {
    switch msg {
    case .hello:
        throw ShmTransportConvertError.helloNotSupported

    case .credit:
        throw ShmTransportConvertError.creditNotSupported

    case .goodbye(_, let reason):
        return ShmGuestFrame(msgType: ShmMsgType.goodbye, id: 0, methodId: 0, payload: Array(reason.utf8))

    case .request(let connId, let requestId, let methodId, let metadata, let channels, let payload):
        let id = try u32Id(requestId)
        return ShmGuestFrame(
            msgType: ShmMsgType.request,
            id: id,
            methodId: methodId,
            payload: encodeCombinedPayload(
                connId: connId,
                metadata: metadata,
                channels: channels,
                payload: payload
            )
        )

    case .response(let connId, let requestId, let metadata, let channels, let payload):
        let id = try u32Id(requestId)
        return ShmGuestFrame(
            msgType: ShmMsgType.response,
            id: id,
            methodId: 0,
            payload: encodeCombinedPayload(
                connId: connId,
                metadata: metadata,
                channels: channels,
                payload: payload
            )
        )

    case .cancel(let connId, let requestId):
        let id = try u32Id(requestId)
        return ShmGuestFrame(
            msgType: ShmMsgType.cancel,
            id: id,
            methodId: 0,
            payload: encodeConnIdLE(connId)
        )

    case .data(let connId, let channelId, let payload):
        let id = try u32Id(channelId)
        return ShmGuestFrame(
            msgType: ShmMsgType.data,
            id: id,
            methodId: 0,
            payload: encodeConnIdLE(connId) + payload
        )

    case .close(let connId, let channelId):
        let id = try u32Id(channelId)
        return ShmGuestFrame(
            msgType: ShmMsgType.close,
            id: id,
            methodId: 0,
            payload: encodeConnIdLE(connId)
        )

    case .reset(let connId, let channelId):
        let id = try u32Id(channelId)
        return ShmGuestFrame(
            msgType: ShmMsgType.reset,
            id: id,
            methodId: 0,
            payload: encodeConnIdLE(connId)
        )

    case .connect(let requestId, let metadata):
        let id = try u32Id(requestId)
        return ShmGuestFrame(
            msgType: ShmMsgType.connect,
            id: id,
            methodId: 0,
            payload: encodeMetadata(metadata)
        )

    case .accept(let requestId, let connId, let metadata):
        let id = try u32Id(requestId)
        var payload = encodeU64(connId)
        payload += encodeMetadata(metadata)
        return ShmGuestFrame(
            msgType: ShmMsgType.accept,
            id: id,
            methodId: 0,
            payload: payload
        )

    case .reject(let requestId, let reason, let metadata):
        let id = try u32Id(requestId)
        var payload = encodeString(reason)
        payload += encodeMetadata(metadata)
        return ShmGuestFrame(
            msgType: ShmMsgType.reject,
            id: id,
            methodId: 0,
            payload: payload
        )
    }
}

func shmFrameToMessage(_ frame: ShmGuestFrame) throws -> Message {
    switch frame.msgType {
    case ShmMsgType.goodbye:
        return .goodbye(connId: 0, reason: String(decoding: frame.payload, as: UTF8.self))

    case ShmMsgType.request:
        let decoded = try decodeCombinedPayload(frame.payload)
        return .request(
            connId: decoded.connId,
            requestId: UInt64(frame.id),
            methodId: frame.methodId,
            metadata: decoded.metadata,
            channels: decoded.channels,
            payload: decoded.payload
        )

    case ShmMsgType.response:
        let decoded = try decodeCombinedPayload(frame.payload)
        return .response(
            connId: decoded.connId,
            requestId: UInt64(frame.id),
            metadata: decoded.metadata,
            channels: decoded.channels,
            payload: decoded.payload
        )

    case ShmMsgType.cancel:
        return .cancel(connId: try decodeConnIdLE(frame.payload), requestId: UInt64(frame.id))

    case ShmMsgType.data:
        let connId = try decodeConnIdLE(frame.payload)
        let payload = frame.payload.count > 8 ? Array(frame.payload[8...]) : []
        return .data(connId: connId, channelId: UInt64(frame.id), payload: payload)

    case ShmMsgType.close:
        return .close(connId: try decodeConnIdLE(frame.payload), channelId: UInt64(frame.id))

    case ShmMsgType.reset:
        return .reset(connId: try decodeConnIdLE(frame.payload), channelId: UInt64(frame.id))

    case ShmMsgType.connect:
        var offset = 0
        let data = Data(frame.payload)
        let metadata = try decodeMetadata(from: data, offset: &offset)
        return .connect(requestId: UInt64(frame.id), metadata: metadata)

    case ShmMsgType.accept:
        var offset = 0
        let data = Data(frame.payload)
        let connId = try decodeU64(from: data, offset: &offset)
        let metadata = try decodeMetadata(from: data, offset: &offset)
        return .accept(requestId: UInt64(frame.id), connId: connId, metadata: metadata)

    case ShmMsgType.reject:
        var offset = 0
        let data = Data(frame.payload)
        let reason = try decodeString(from: data, offset: &offset)
        let metadata = try decodeMetadata(from: data, offset: &offset)
        return .reject(requestId: UInt64(frame.id), reason: reason, metadata: metadata)

    default:
        throw ShmTransportConvertError.unknownMsgType(frame.msgType)
    }
}

public final class ShmGuestTransport: MessageTransport, @unchecked Sendable {
    public let negotiated: Negotiated

    private let lock = NSLock()
    private let diagnosticsId = UUID()
    private var runtime: ShmGuestRuntime
    private var maxFrameSize: Int
    private var closed = false

    public init(runtime: ShmGuestRuntime) {
        let id = diagnosticsId
        self.runtime = runtime
        self.maxFrameSize = Int(runtime.header.maxPayloadSize) + 64
        self.negotiated = Negotiated(
            maxPayloadSize: runtime.header.maxPayloadSize,
            initialCredit: runtime.header.initialCredit,
            maxConcurrentRequests: UInt32.max
        )
        ShmDiagnosticsRegistry.register(id: diagnosticsId) { [weak self] in
            self?.diagnosticsSnapshot()
                ?? ShmTransportDiagnosticsSnapshot(
                    id: id,
                    peerId: 0,
                    maxPayloadSize: 0,
                    initialCredit: 0,
                    maxFrameSize: 0,
                    closed: true,
                    hostGoodbye: true,
                    timestamp: Date()
                )
        }
    }

    public static func attach(ticket: ShmBootstrapTicket) throws -> ShmGuestTransport {
        ShmGuestTransport(runtime: try ShmGuestRuntime.attach(ticket: ticket))
    }

    public static func attach(path: String) throws -> ShmGuestTransport {
        ShmGuestTransport(runtime: try ShmGuestRuntime.attach(path: path))
    }

    public func send(_ message: Message) async throws {
        let frame = try messageToShmFrame(message)
        do {
            try lock.withLock {
                if closed {
                    throw TransportError.connectionClosed
                }
                if frame.payload.count > Int(negotiated.maxPayloadSize) {
                    throw TransportError.protocolViolation(
                        "payload exceeds negotiated maxPayloadSize")
                }
                if frame.payload.count + 64 > maxFrameSize {
                    throw TransportError.frameEncoding("frame exceeds max frame size")
                }

                _ = try runtime.checkRemap()
                try runtime.send(frame: frame)
            }
        } catch let err as TransportError {
            throw err
        } catch let err as ShmGuestSendError {
            switch err {
            case .ringFull, .slotExhausted:
                throw TransportError.wouldBlock
            case .hostGoodbye, .doorbellPeerDead:
                throw TransportError.connectionClosed
            case .payloadTooLarge, .slotError:
                throw TransportError.transportIO("shm send failed: \(err)")
            }
        } catch {
            throw TransportError.transportIO("shm send failed: \(error)")
        }
    }

    public func recv() async throws -> Message? {
        while true {
            var frameToDecode: ShmGuestFrame?
            var sawHostGoodbye = false
            var isClosed = false

            do {
                try lock.withLock {
                    isClosed = closed
                    if isClosed {
                        return
                    }
                    _ = try runtime.checkRemap()
                    frameToDecode = try runtime.receive()
                    sawHostGoodbye = runtime.isHostGoodbye()
                }
            } catch let err as TransportError {
                throw err
            } catch {
                throw TransportError.transportIO("shm receive failed: \(error)")
            }

            if isClosed {
                return nil
            }

            if let frame = frameToDecode {
                do {
                    try lock.withLock {
                        try runtime.signalDoorbell()
                    }
                } catch {
                    throw TransportError.transportIO("doorbell signal failed: \(error)")
                }
                return try shmFrameToMessage(frame)
            }

            if sawHostGoodbye {
                return nil
            }

            do {
                if let wait = try runtime.waitForDoorbell(timeoutMs: 100) {
                    if wait == .peerDead {
                        if runtime.isHostGoodbye() {
                            return nil
                        }
                        throw TransportError.connectionClosed
                    }
                    continue
                }
            } catch let err as TransportError {
                throw err
            } catch {
                throw TransportError.transportIO("doorbell wait failed: \(error)")
            }

            try await Task.sleep(nanoseconds: 1_000_000)
        }

        return nil
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        lock.withLock {
            maxFrameSize = size
        }
    }

    public func close() async throws {
        lock.withLock {
            if closed {
                return
            }
            closed = true
            runtime.detach()
        }
        ShmDiagnosticsRegistry.unregister(id: diagnosticsId)
    }

    deinit {
        ShmDiagnosticsRegistry.unregister(id: diagnosticsId)
    }

    public func diagnosticsSnapshot() -> ShmTransportDiagnosticsSnapshot {
        lock.withLock {
            ShmTransportDiagnosticsSnapshot(
                id: diagnosticsId,
                peerId: runtime.peerId,
                maxPayloadSize: runtime.header.maxPayloadSize,
                initialCredit: runtime.header.initialCredit,
                maxFrameSize: maxFrameSize,
                closed: closed,
                hostGoodbye: runtime.isHostGoodbye(),
                timestamp: Date()
            )
        }
    }
}

private func encodeCombinedPayload(
    connId: UInt64,
    metadata: [MetadataEntry],
    channels: [UInt64],
    payload: [UInt8]
) -> [UInt8] {
    var out: [UInt8] = []
    out += encodeU64(connId)
    out += encodeMetadata(metadata)
    out += encodeVec(channels, encoder: encodeU64)
    out += encodeBytes(payload)
    return out
}

private func decodeCombinedPayload(_ bytes: [UInt8]) throws
    -> (connId: UInt64, metadata: [MetadataEntry], channels: [UInt64], payload: [UInt8])
{
    if bytes.isEmpty {
        return (0, [], [], [])
    }

    var offset = 0
    let data = Data(bytes)
    let connId = try decodeU64(from: data, offset: &offset)
    let metadata = try decodeMetadata(from: data, offset: &offset)
    let channels = try decodeVec(from: data, offset: &offset) { data, offset in
        try decodeU64(from: data, offset: &offset)
    }
    let payload = Array(try decodeBytes(from: data, offset: &offset))
    return (connId, metadata, channels, payload)
}

private func encodeConnIdLE(_ connId: UInt64) -> [UInt8] {
    withUnsafeBytes(of: connId.littleEndian) { Array($0) }
}

private func decodeConnIdLE(_ payload: [UInt8]) throws -> UInt64 {
    guard payload.count >= 8 else {
        throw ShmTransportConvertError.decodeError("payload too short for conn_id")
    }
    return payload.withUnsafeBytes { raw in
        raw.load(as: UInt64.self).littleEndian
    }
}

private func u32Id(_ value: UInt64) throws -> UInt32 {
    guard let id = UInt32(exactly: value) else {
        throw ShmTransportConvertError.idOutOfRange(value)
    }
    return id
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
