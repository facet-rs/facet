import Foundation

public enum ShmTransportConvertError: Error, Equatable {
    case decodeError(String)
}

// r[impl transport.shm]
// r[impl zerocopy.framing.link.shm]
func messageToShmFrame(_ msg: MessageV7) throws -> ShmGuestFrame {
    ShmGuestFrame(payload: msg.encode())
}

// r[impl transport.shm]
// r[impl zerocopy.framing.link.shm]
func shmFrameToMessage(_ frame: ShmGuestFrame) throws -> MessageV7 {
    do {
        return try MessageV7.decode(from: Data(frame.payload))
    } catch WireV7Error.trailingBytes {
        // Inline SHM frames are 4-byte aligned and can carry up to 3 trailing zero bytes.
        // MessageV7 is self-delimiting; retry decode after trimming zero padding.
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
                return try MessageV7.decode(from: Data(trimmed))
            } catch {
                continue
            }
        }
        throw ShmTransportConvertError.decodeError("\(WireV7Error.trailingBytes)")
    } catch {
        throw ShmTransportConvertError.decodeError("\(error)")
    }
}

// r[impl transport.shm]
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

    public func send(_ message: MessageV7) async throws {
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
            case .payloadTooLarge, .slotError, .mmapAllocationFailed, .mmapUnavailable, .mmapControlError:
                throw TransportError.transportIO("shm send failed: \(err)")
            }
        } catch {
            throw TransportError.transportIO("shm send failed: \(error)")
        }
    }

    public func recv() async throws -> MessageV7? {
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

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
