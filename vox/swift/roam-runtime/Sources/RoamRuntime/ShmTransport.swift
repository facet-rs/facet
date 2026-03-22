import Foundation

public enum ShmTransportConvertError: Error, Equatable {
    case decodeError(String)
}

// r[impl transport.shm]
// r[impl zerocopy.framing.link.shm]
func messageToShmFrame(_ msg: Message) throws -> ShmGuestFrame {
    ShmGuestFrame(payload: msg.encode())
}

// r[impl transport.shm]
// r[impl zerocopy.framing.link.shm]
func shmFrameToMessage(_ frame: ShmGuestFrame) throws -> Message {
    do {
        return try Message.decode(from: Data(frame.payload))
    } catch WireError.trailingBytes {
        // Inline SHM frames are 4-byte aligned and can carry up to 3 trailing zero bytes.
        // Message is self-delimiting; retry decode after trimming zero padding.
        for pad in 1...3 {
            guard frame.payload.count >= pad else {
                break
            }
            let suffix = frame.payload[(frame.payload.count - pad)...]
            guard suffix.allSatisfy({ $0 == 0 }) else {
                break
            }
            let trimmed = Array(frame.payload.dropLast(pad))
            do {
                return try Message.decode(from: Data(trimmed))
            } catch {
                continue
            }
        }
        throw ShmTransportConvertError.decodeError("\(WireError.trailingBytes)")
    } catch {
        throw ShmTransportConvertError.decodeError("\(error)")
    }
}

// r[impl transport.shm]
public final class ShmGuestTransport: Link, @unchecked Sendable {
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

    public func sendFrame(_ bytes: [UInt8]) async throws {
        try await sendRawPrologue(bytes)
    }

    public func sendRawPrologue(_ bytes: [UInt8]) async throws {
        try sendShmTransportFrame(
            bytes: bytes,
            negotiated: negotiated,
            maxFrameSize: maxFrameSize,
            sendErrorPrefix: "shm send failed",
            mapSendError: { (err: ShmGuestSendError) in
                switch err {
                case .ringFull, .slotExhausted:
                    return .wouldBlock
                case .hostGoodbye, .doorbellPeerDead:
                    return .connectionClosed
                case .payloadTooLarge, .slotError, .mmapAllocationFailed, .mmapUnavailable, .mmapControlError:
                    return .transportIO("shm send failed: \(err)")
                }
            },
            performLockedSend: { frame in
                try lock.withShmLock {
                    if closed {
                        throw TransportError.connectionClosed
                    }
                    _ = try runtime.checkRemap()
                    try runtime.send(frame: frame)
                }
            }
        )
    }

    public func recvFrame() async throws -> [UInt8]? {
        try await recvRawPrologue()
    }

    public func recvRawPrologue() async throws -> [UInt8]? {
        try await recvShmTransportFrame(
            receiveErrorPrefix: "shm receive failed",
            pollLockedReceive: {
                try lock.withShmLock {
                    if closed {
                        return ShmTransportReceivePoll(isClosed: true, frame: nil, sawGoodbye: false)
                    }
                    _ = try runtime.checkRemap()
                    return ShmTransportReceivePoll(
                        isClosed: false,
                        frame: try runtime.receive(),
                        sawGoodbye: runtime.isHostGoodbye()
                    )
                }
            },
            signalDoorbell: {
                try lock.withShmLock {
                    try runtime.signalDoorbell()
                }
            },
            waitForDoorbell: { timeoutMs in
                try runtime.waitForDoorbell(timeoutMs: timeoutMs)
            },
            shouldTreatPeerDeadAsGoodbye: {
                runtime.isHostGoodbye()
            }
        )
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
