import Foundation

struct ShmTransportReceivePoll {
    let isClosed: Bool
    let frame: ShmGuestFrame?
    let sawGoodbye: Bool
}

func sendShmTransportFrame<SendError: Error>(
    bytes: [UInt8],
    negotiated: Negotiated,
    maxFrameSize: Int,
    sendErrorPrefix: String,
    mapSendError: (SendError) -> TransportError,
    performLockedSend: (ShmGuestFrame) throws -> Void
) throws {
    let frame = ShmGuestFrame(payload: bytes)

    do {
        if frame.payload.count > Int(negotiated.maxPayloadSize) {
            throw TransportError.protocolViolation("payload exceeds negotiated maxPayloadSize")
        }
        if frame.payload.count + 64 > maxFrameSize {
            throw TransportError.frameEncoding("frame exceeds max frame size")
        }

        try performLockedSend(frame)
    } catch let err as TransportError {
        throw err
    } catch let err as SendError {
        throw mapSendError(err)
    } catch {
        throw TransportError.transportIO("\(sendErrorPrefix): \(error)")
    }
}

func recvShmTransportFrame(
    receiveErrorPrefix: String,
    pollLockedReceive: () throws -> ShmTransportReceivePoll,
    signalDoorbell: () throws -> Void,
    waitForDoorbell: (_ timeoutMs: Int32?) throws -> ShmDoorbellWaitResult?,
    shouldTreatPeerDeadAsGoodbye: () -> Bool
) async throws -> [UInt8]? {
    while true {
        let poll: ShmTransportReceivePoll
        do {
            poll = try pollLockedReceive()
        } catch let err as TransportError {
            throw err
        } catch {
            throw TransportError.transportIO("\(receiveErrorPrefix): \(error)")
        }

        if poll.isClosed {
            return nil
        }

        if let frame = poll.frame {
            do {
                try signalDoorbell()
            } catch {
                throw TransportError.transportIO("doorbell signal failed: \(error)")
            }
            return frame.payload
        }

        if poll.sawGoodbye {
            return nil
        }

        do {
            if let wait = try waitForDoorbell(100) {
                if wait == .peerDead {
                    if shouldTreatPeerDeadAsGoodbye() {
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
}

extension NSLock {
    @inline(__always)
    func withShmLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
