#if os(macOS)
import Darwin
import Foundation
import Testing
import CRoamShmFfi

@testable import RoamRuntime

private func hostTmpPath(_ suffix: String) -> String {
    "/tmp/roam-swift-host-\(UUID().uuidString)-\(suffix)"
}

private func hostSocketPair() throws -> (Int32, Int32) {
    var fds = [Int32](repeating: -1, count: 2)
    guard socketpair(AF_UNIX, SOCK_STREAM, 0, &fds) == 0 else {
        throw POSIXError(.EIO)
    }
    return (fds[0], fds[1])
}

private func socketType(fd: Int32) -> Int32? {
    var value: Int32 = 0
    var len = socklen_t(MemoryLayout<Int32>.size)
    let rc = withUnsafeMutablePointer(to: &value) { ptr in
        getsockopt(fd, SOL_SOCKET, SO_TYPE, ptr, &len)
    }
    guard rc == 0 else {
        return nil
    }
    return value
}

private func withHostTimeout<T: Sendable>(
    milliseconds: UInt64,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: milliseconds * 1_000_000)
            throw POSIXError(.ETIMEDOUT)
        }
        let result = try await group.next()!
        group.cancelAll()
        return result
    }
}

private func protocolErrorDescription(_ msg: MessageV7) -> String? {
    guard case .protocolError(let payload) = msg.payload else {
        return nil
    }
    return payload.description
}

private struct ShmHostNoopDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        requestId _: UInt64,
        registry _: ChannelRegistry,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

struct ShmHostRuntimeTests {
    @Test func reservePeerUsesDatagramSocketForMmapControl() throws {
        let path = hostTmpPath("mmap-control-socket-type.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 512,
                sizeClasses: [ShmVarSlotClass(slotSize: 512, count: 4)]
            )
        )

        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        defer {
            close(ticket.doorbellFd)
            if ticket.mmapControlFd >= 0 {
                close(ticket.mmapControlFd)
            }
            prepared.releaseReservation()
        }

        #expect(socketType(fd: ticket.doorbellFd) == SOCK_STREAM)
        #expect(socketType(fd: ticket.mmapControlFd) == SOCK_DGRAM)
    }

    @Test func reserveReleaseReservationAndReusePeer() throws {
        let path = hostTmpPath("reserve-release.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 512,
                sizeClasses: [ShmVarSlotClass(slotSize: 512, count: 4)]
            )
        )

        let first = try segment.reservePeer()
        #expect(first.peerId == 1)
        #expect(throws: ShmHostSegmentError.noFreePeerSlots) {
            _ = try segment.reservePeer()
        }

        first.releaseReservation()

        let second = try segment.reservePeer()
        #expect(second.peerId == 1)
        second.releaseReservation()
    }

    @Test func hostGuestTransportRoundTripsInlineAndMmapPayloads() async throws {
        let path = hostTmpPath("transport-roundtrip.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 2048,
                maxPayloadSize: 128 * 1024,
                inlineThreshold: 64,
                sizeClasses: [
                    ShmVarSlotClass(slotSize: 256, count: 8),
                    ShmVarSlotClass(slotSize: 1024, count: 4),
                ]
            )
        )

        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        let host = try prepared.intoTransport()
        let guest = try ShmGuestTransport.attach(ticket: ticket)

        defer {
            close(ticket.doorbellFd)
            if ticket.mmapControlFd >= 0 {
                close(ticket.mmapControlFd)
            }
        }

        defer {
            Task {
                try? await guest.close()
                try? await host.close()
            }
        }

        try await guest.send(.protocolError(description: "guest-inline"))
        let hostInline = try await withHostTimeout(milliseconds: 2_000) {
            try await host.recv()
        }
        if let hostInline {
            #expect(protocolErrorDescription(hostInline) == "guest-inline")
        } else {
            Issue.record("expected inline frame from guest")
        }

        try await host.send(.protocolError(description: "host-inline"))
        let guestInline = try await withHostTimeout(milliseconds: 2_000) {
            try await guest.recv()
        }
        if let guestInline {
            #expect(protocolErrorDescription(guestInline) == "host-inline")
        } else {
            Issue.record("expected inline frame from host")
        }

        let largeGuestText = String(repeating: "g", count: 12_000)
        try await guest.send(.protocolError(description: largeGuestText))
        let hostLarge = try await withHostTimeout(milliseconds: 2_000) {
            try await host.recv()
        }
        if let hostLarge {
            #expect(protocolErrorDescription(hostLarge) == largeGuestText)
        } else {
            Issue.record("expected large frame from guest")
        }

        let largeHostText = String(repeating: "h", count: 12_000)
        try await host.send(.protocolError(description: largeHostText))
        let guestLarge = try await withHostTimeout(milliseconds: 2_000) {
            try await guest.recv()
        }
        if let guestLarge {
            #expect(protocolErrorDescription(guestLarge) == largeHostText)
        } else {
            Issue.record("expected large frame from host")
        }

        try await guest.close()
        try await host.close()
    }

    @Test func sendBootstrapSuccessTransfersDoorbellSegmentAndMmapFds() throws {
        let path = hostTmpPath("bootstrap-send-success.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 512,
                sizeClasses: [ShmVarSlotClass(slotSize: 512, count: 4)]
            )
        )

        let prepared = try segment.reservePeer()
        let sockets = try hostSocketPair()
        defer {
            close(sockets.0)
            close(sockets.1)
        }

        try prepared.sendBootstrapSuccess(controlFd: sockets.0, hubPath: path)

        var info = RoamShmBootstrapResponseInfo(status: 0, peer_id: 0, payload_len: 0)
        var doorbellFd: Int32 = -1
        var segmentFd: Int32 = -1
        var mmapControlFd: Int32 = -1
        var payload = [UInt8](repeating: 0, count: 512)

        let recvRc = payload.withUnsafeMutableBufferPointer { payloadBuf in
            withUnsafeMutablePointer(to: &info) { infoPtr in
                roam_shm_bootstrap_response_recv_unix(
                    sockets.1,
                    payloadBuf.baseAddress,
                    UInt(payloadBuf.count),
                    infoPtr,
                    &doorbellFd,
                    &segmentFd,
                    &mmapControlFd
                )
            }
        }

        #expect(recvRc == 0)
        #expect(info.status == 0)
        #expect(info.peer_id == UInt32(prepared.peerId))
        #expect(doorbellFd >= 0)
        #expect(segmentFd >= 0)
        #expect(mmapControlFd >= 0)

        let payloadLen = Int(info.payload_len)
        let payloadPath = String(decoding: payload.prefix(payloadLen), as: UTF8.self)
        #expect(payloadPath == path)

        close(doorbellFd)
        close(segmentFd)
        close(mmapControlFd)
        prepared.releaseReservation()
    }

    @Test func hostTransportSupportsAcceptorHandshakeWithGuestInitiator() async throws {
        let path = hostTmpPath("acceptor-handshake.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 1024,
                maxPayloadSize: 1024 * 1024,
                inlineThreshold: 256,
                sizeClasses: [ShmVarSlotClass(slotSize: 4096, count: 8)]
            )
        )

        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        let host = try prepared.intoTransport()
        let guest = try ShmGuestTransport.attach(ticket: ticket)

        defer {
            close(ticket.doorbellFd)
            if ticket.mmapControlFd >= 0 {
                close(ticket.mmapControlFd)
            }
        }
        defer {
            Task {
                try? await guest.close()
                try? await host.close()
            }
        }

        let hostTask = Task {
            try await establishAcceptor(transport: host, dispatcher: ShmHostNoopDispatcher())
        }
        let guestTask = Task {
            try await establishInitiator(transport: guest, dispatcher: ShmHostNoopDispatcher())
        }

        _ = try await withHostTimeout(milliseconds: 2_000) { try await hostTask.value }
        _ = try await withHostTimeout(milliseconds: 2_000) { try await guestTask.value }

        try await guest.close()
        try await host.close()
    }
}
#endif
