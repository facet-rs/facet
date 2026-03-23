import Foundation

extension Driver {
    func makeKeepaliveRuntime() -> DriverKeepaliveRuntime? {
        guard let keepalive else {
            return nil
        }
        let pingIntervalNs = Self.timeoutToNanoseconds(keepalive.pingInterval)
        let pongTimeoutNs = Self.timeoutToNanoseconds(keepalive.pongTimeout)
        if pingIntervalNs == 0 || pongTimeoutNs == 0 {
            warnLog("keepalive disabled due to non-positive interval/timeout")
            return nil
        }
        let now = DispatchTime.now().uptimeNanoseconds
        return DriverKeepaliveRuntime(
            pingIntervalNs: pingIntervalNs,
            pongTimeoutNs: pongTimeoutNs,
            nextPingAtNs: Self.saturatingAdd(now, pingIntervalNs),
            waitingPongNonce: nil,
            pongDeadlineNs: 0,
            nextPingNonce: 1
        )
    }

    func handlePong(nonce: UInt64, keepaliveRuntime: inout DriverKeepaliveRuntime?) {
        guard var runtime = keepaliveRuntime else {
            return
        }
        guard runtime.waitingPongNonce == nonce else {
            return
        }
        runtime.waitingPongNonce = nil
        runtime.pongDeadlineNs = 0
        runtime.nextPingAtNs = Self.saturatingAdd(
            DispatchTime.now().uptimeNanoseconds,
            runtime.pingIntervalNs
        )
        keepaliveRuntime = runtime
    }

    func handleKeepaliveTick(keepaliveRuntime: inout DriverKeepaliveRuntime?) async throws {
        guard var runtime = keepaliveRuntime else {
            return
        }
        let now = DispatchTime.now().uptimeNanoseconds

        if let waitingNonce = runtime.waitingPongNonce,
            now >= runtime.pongDeadlineNs
        {
            warnLog(
                "keepalive timeout waiting for pong nonce=\(waitingNonce) "
                    + "timeout_ns=\(runtime.pongTimeoutNs)"
            )
            await failAllPending()
            throw ConnectionError.connectionClosed
        }

        guard runtime.waitingPongNonce == nil else {
            keepaliveRuntime = runtime
            return
        }
        guard now >= runtime.nextPingAtNs else {
            keepaliveRuntime = runtime
            return
        }

        let nonce = runtime.nextPingNonce
        do {
            try await conduit.send(.ping(.init(nonce: nonce)))
            runtime.waitingPongNonce = nonce
            runtime.pongDeadlineNs = Self.saturatingAdd(now, runtime.pongTimeoutNs)
            runtime.nextPingAtNs = Self.saturatingAdd(now, runtime.pingIntervalNs)
            runtime.nextPingNonce = nonce &+ 1
        } catch TransportError.wouldBlock {
        } catch {
            throw error
        }

        keepaliveRuntime = runtime
    }

    static func timeoutToNanoseconds(_ timeout: TimeInterval) -> UInt64 {
        if timeout <= 0 {
            return 0
        }
        let nanoseconds = timeout * 1_000_000_000
        if nanoseconds >= Double(UInt64.max) {
            return UInt64.max
        }
        return UInt64(nanoseconds)
    }

    static func saturatingAdd(_ lhs: UInt64, _ rhs: UInt64) -> UInt64 {
        if lhs > UInt64.max - rhs {
            return UInt64.max
        }
        return lhs + rhs
    }
}
