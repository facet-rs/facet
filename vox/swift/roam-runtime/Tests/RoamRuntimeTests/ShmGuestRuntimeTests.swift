#if os(macOS)
import Darwin
import Foundation
import Testing
import CRoamShmFfi

@testable import RoamRuntime

private func tmpPath(_ suffix: String) -> String {
    "/tmp/roam-swift-guest-\(UUID().uuidString)-\(suffix)"
}

private struct SegmentFixture {
    let path: String
    let region: ShmRegion
    let classes: [ShmVarSlotClass]
    let ringOffsets: [UInt8: Int]
    let bipbufCapacity: UInt32
}

private func makeSegmentFixture(
    path: String,
    maxGuests: UInt32 = 1,
    bipbufCapacity: UInt32 = 256,
    inlineThreshold: UInt32 = 64,
    maxPayloadSize: UInt32 = 4096,
    classes: [ShmVarSlotClass],
    reservedPeer: UInt8? = nil
) throws -> SegmentFixture {
    let peerTableOffset = shmSegmentHeaderSize
    let peerTableSize = Int(maxGuests) * shmPeerEntrySize
    let varPoolOffset = alignUp(peerTableOffset + peerTableSize, to: 64)
    let varPoolSize = ShmVarSlotPool.calculateSize(classes: classes)
    let guestAreasOffset = alignUp(varPoolOffset + varPoolSize, to: 64)

    let perPeerArea = alignUp((shmBipbufHeaderSize + Int(bipbufCapacity)) * 2 + shmChannelEntrySize, to: 64)
    let totalSize = guestAreasOffset + perPeerArea * Int(maxGuests)

    let region = try ShmRegion.create(path: path, size: totalSize, cleanup: .manual)

    var header = [UInt8](repeating: 0, count: shmSegmentHeaderSize)
    for (idx, b) in shmSegmentMagic.enumerated() {
        header[idx] = b
    }
    writeU32LE(2, to: &header, at: 8)
    writeU32LE(UInt32(shmSegmentHeaderSize), to: &header, at: 12)
    writeU64LE(UInt64(totalSize), to: &header, at: 16)
    writeU32LE(maxPayloadSize, to: &header, at: 24)
    writeU32LE(32, to: &header, at: 28)
    writeU32LE(maxGuests, to: &header, at: 32)
    writeU32LE(bipbufCapacity, to: &header, at: 36)
    writeU64LE(UInt64(peerTableOffset), to: &header, at: 40)
    writeU64LE(0, to: &header, at: 48)
    writeU32LE(0, to: &header, at: 56)
    writeU32LE(inlineThreshold, to: &header, at: 60)
    writeU32LE(1, to: &header, at: 64)
    writeU32LE(0, to: &header, at: 68)
    writeU64LE(0, to: &header, at: 72)
    writeU64LE(UInt64(varPoolOffset), to: &header, at: 80)
    writeU64LE(UInt64(totalSize), to: &header, at: 88)
    writeU64LE(UInt64(guestAreasOffset), to: &header, at: 96)
    writeU32LE(UInt32(classes.count), to: &header, at: 104)

    let headerBytes = try region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
    headerBytes.copyBytes(from: header)

    let pool = try ShmVarSlotPool(region: region, baseOffset: varPoolOffset, classes: classes)
    pool.initialize()

    var ringOffsets: [UInt8: Int] = [:]

    for peer in 1...UInt8(maxGuests) {
        let peerOffset = peerTableOffset + Int(peer - 1) * shmPeerEntrySize
        var entry = [UInt8](repeating: 0, count: shmPeerEntrySize)
        let state: UInt32 = reservedPeer == peer ? ShmPeerState.reserved.rawValue : ShmPeerState.empty.rawValue
        writeU32LE(state, to: &entry, at: 0)

        let ringOffset = guestAreasOffset + Int(peer - 1) * perPeerArea
        ringOffsets[peer] = ringOffset
        writeU64LE(UInt64(ringOffset), to: &entry, at: 32)
        writeU64LE(0, to: &entry, at: 40)
        writeU64LE(UInt64(ringOffset + (shmBipbufHeaderSize + Int(bipbufCapacity)) * 2), to: &entry, at: 48)

        let entryBytes = try region.mutableBytes(at: peerOffset, count: shmPeerEntrySize)
        entryBytes.copyBytes(from: entry)

        _ = try ShmBipBuffer.initialize(region: region, headerOffset: ringOffset, capacity: bipbufCapacity)
        _ = try ShmBipBuffer.initialize(
            region: region,
            headerOffset: ringOffset + shmBipbufHeaderSize + Int(bipbufCapacity),
            capacity: bipbufCapacity
        )
    }

    return SegmentFixture(
        path: path,
        region: region,
        classes: classes,
        ringOffsets: ringOffsets,
        bipbufCapacity: bipbufCapacity
    )
}

private func makeDoorbellPair() throws -> (host: Int32, guest: Int32) {
    var fds = [Int32](repeating: -1, count: 2)
    guard socketpair(AF_UNIX, SOCK_STREAM, 0, &fds) == 0 else {
        throw POSIXError(.EIO)
    }
    return (fds[0], fds[1])
}

private func isConnectionClosedTransportError(_ error: Error) -> Bool {
    guard let transportError = error as? TransportError else {
        return false
    }
    if case .connectionClosed = transportError {
        return true
    }
    return false
}

private func isConnectionClosedConnectionError(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .connectionClosed = connError {
        return true
    }
    return false
}

private func isTimeoutConnectionError(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .timeout = connError {
        return true
    }
    return false
}

private func isTransportConnectionError(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .transportError = connError {
        return true
    }
    return false
}

private enum ShmHarnessError: Error {
    case missingRingOffset(UInt8)
    case timeout(String)
    case unexpectedFrame(String)
}

private actor PendingHarnessError {
    private var error: Error?

    func set(_ error: Error) {
        self.error = error
    }

    func get() -> Error? {
        error
    }
}

private struct ShmNoopDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId _: UInt64,
        registry _: ChannelRegistry,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

private func hostPeerBuffers(
    fixture: SegmentFixture,
    peerId: UInt8
) throws -> (guestToHost: ShmBipBuffer, hostToGuest: ShmBipBuffer) {
    guard let ringOffset = fixture.ringOffsets[peerId] else {
        throw ShmHarnessError.missingRingOffset(peerId)
    }
    let guestToHost = try ShmBipBuffer.attach(region: fixture.region, headerOffset: ringOffset)
    let hostToGuest = try ShmBipBuffer.attach(
        region: fixture.region,
        headerOffset: ringOffset + shmBipbufHeaderSize + Int(guestToHost.capacity)
    )
    return (guestToHost: guestToHost, hostToGuest: hostToGuest)
}

private func hostReadMessage(
    from guestToHost: ShmBipBuffer,
    timeoutMs: UInt64 = 1_000
) async throws -> MessageV7? {
    guard let payload = try await hostReadRawPayload(from: guestToHost, timeoutMs: timeoutMs) else {
        return nil
    }
    return try MessageV7.decode(from: Data(payload))
}

private func hostReadRawPayload(
    from guestToHost: ShmBipBuffer,
    timeoutMs: UInt64 = 1_000
) async throws -> [UInt8]? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        if let readable = guestToHost.tryRead() {
            let bytes = Array(readable)
            let decoded = try decodeShmFrame(bytes)
            switch decoded {
            case .inline(let header, let payload):
                try guestToHost.release(header.totalLen)
                return payload
            case .slotRef(let header, _):
                try guestToHost.release(header.totalLen)
                throw ShmHarnessError.unexpectedFrame("host received slot-ref frame in test harness")
            case .mmapRef(let header, _):
                try guestToHost.release(header.totalLen)
                throw ShmHarnessError.unexpectedFrame("host received mmap-ref frame in test harness")
            }
        }
        try? await Task.sleep(nanoseconds: 1_000_000)
    }
    return nil
}

private func hostSendMessage(
    _ message: MessageV7,
    to hostToGuest: ShmBipBuffer,
    doorbell: ShmDoorbell,
    timeoutMs: UInt64 = 1_000
) async throws {
    try await hostSendRawPayload(message.encode(), to: hostToGuest, doorbell: doorbell, timeoutMs: timeoutMs)
}

private func hostSendRawPayload(
    _ payload: [UInt8],
    to hostToGuest: ShmBipBuffer,
    doorbell: ShmDoorbell,
    timeoutMs: UInt64 = 1_000
) async throws {
    let frame = encodeShmInlineFrame(payload: payload)
    let frameLen = UInt32(frame.count)
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        if let grant = try hostToGuest.tryGrant(frameLen) {
            grant.copyBytes(from: frame)
            try hostToGuest.commit(frameLen)
            try doorbell.signal()
            return
        }
        try? await Task.sleep(nanoseconds: 1_000_000)
    }
    throw ShmHarnessError.timeout("host send timed out waiting for ring grant")
}

private func setHostGoodbye(_ fixture: SegmentFixture) throws {
    var header = Array(try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize))
    writeU32LE(1, to: &header, at: 68)
    let headerBytes = try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
    headerBytes.copyBytes(from: header)
}

private func establishShmInitiator(
    path: String,
    guestDoorbellFd: Int32,
    hostDoorbell: ShmDoorbell,
    guestToHost: ShmBipBuffer,
    hostToGuest: ShmBipBuffer
) async throws -> (ShmGuestTransport, Connection, Driver) {
    let transport = try ShmGuestTransport.attach(
        ticket: ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: guestDoorbellFd)
    )
    let establishTask = Task {
        try await establishShmGuest(
            transport: transport,
            dispatcher: ShmNoopDispatcher(),
            role: .initiator
        )
    }
    let establishFailure = PendingHarnessError()
    Task {
        do {
            _ = try await establishTask.value
        } catch {
            await establishFailure.set(error)
        }
    }

    guard let transportHello = try await hostReadRawPayload(from: guestToHost) else {
        if let error = await establishFailure.get() {
            throw error
        }
        throw ShmHarnessError.timeout("did not receive transport hello")
    }
    let requested = try decodeTransportHello(transportHello)
    guard requested == .bare else {
        throw ShmHarnessError.unexpectedFrame("expected bare transport hello")
    }
    try await hostSendRawPayload(
        encodeTransportAccept(.bare),
        to: hostToGuest,
        doorbell: hostDoorbell
    )

    guard let helloBytes = try await hostReadRawPayload(from: guestToHost) else {
        if let error = await establishFailure.get() {
            throw error
        }
        throw ShmHarnessError.timeout("did not receive initiator hello")
    }
    let hello = try HandshakeMessage.decodeCbor(helloBytes)
    guard case .hello = hello else {
        throw ShmHarnessError.unexpectedFrame("expected hello during handshake")
    }

    try await hostSendRawPayload(
        HandshakeMessage.helloYourself(
            HandshakeHelloYourself(
                connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
                messagePayloadSchemaCbor: wireMessageSchemasCbor,
                supportsRetry: true,
                resumeKey: nil
            )
        ).encodeCbor(),
        to: hostToGuest,
        doorbell: hostDoorbell
    )

    guard let letsGoBytes = try await hostReadRawPayload(from: guestToHost) else {
        if let error = await establishFailure.get() {
            throw error
        }
        throw ShmHarnessError.timeout("did not receive lets-go")
    }
    guard case .letsGo = try HandshakeMessage.decodeCbor(letsGoBytes) else {
        throw ShmHarnessError.unexpectedFrame("expected lets-go during handshake")
    }

    let (handle, driver, _, _) = try await establishTask.value
    return (transport, handle, driver)
}

struct ShmVarSlotPoolTests {
    // r[verify shm.varslot]
    // r[verify shm.varslot.allocate]
    // r[verify shm.varslot.free]
    // r[verify shm.varslot.selection]
    // r[verify shm.varslot.freelist]
    // r[verify shm.varslot.classes]
    // r[verify shm.varslot.slot-meta]
    @Test func allocFreeAndGenerationTransitions() throws {
        let path = tmpPath("varslot.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(
            path: path,
            classes: [
                ShmVarSlotClass(slotSize: 64, count: 1),
                ShmVarSlotClass(slotSize: 128, count: 1),
            ]
        )

        let header = try ShmSegmentView(region: fixture.region).header
        let pool = try ShmVarSlotPool(
            region: fixture.region,
            baseOffset: Int(header.varSlotPoolOffset),
            classes: fixture.classes
        )

        let first = try #require(pool.alloc(size: 32, owner: 1))
        #expect(first.classIdx == 0)
        #expect(pool.slotState(first) == .allocated)

        let second = try #require(pool.alloc(size: 32, owner: 1))
        #expect(second.classIdx == 1)

        try pool.markInFlight(first)
        #expect(pool.slotState(first) == .inFlight)
        try pool.free(first)
        #expect(pool.slotState(first) == .free)

        let reused = try #require(pool.alloc(size: 32, owner: 1))
        #expect(reused.classIdx == 0)
        #expect(reused.generation > first.generation)
    }

    // r[verify shm.varslot]
    // r[verify shm.varslot.allocate]
    // r[verify shm.varslot.free]
    @Test func stressChurnEndsWithNoLeakedSlots() async throws {
        let path = tmpPath("varslot-stress.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let classes = [
            ShmVarSlotClass(slotSize: 64, count: 64),
            ShmVarSlotClass(slotSize: 256, count: 32),
        ]
        let fixture = try makeSegmentFixture(path: path, classes: classes)
        let header = try ShmSegmentView(region: fixture.region).header
        let pool = try ShmVarSlotPool(
            region: fixture.region,
            baseOffset: Int(header.varSlotPoolOffset),
            classes: classes
        )

        let workers = 8
        let iterations = 400

        try await withThrowingTaskGroup(of: Void.self) { group in
            for worker in 0..<workers {
                group.addTask {
                    var rng = UInt64(0x9E3779B97F4A7C15 ^ UInt64(worker))
                    var owned: [ShmVarSlotHandle] = []

                    func next() -> UInt64 {
                        rng = rng &* 6364136223846793005 &+ 1
                        return rng
                    }

                    for _ in 0..<iterations {
                        if owned.isEmpty || (next() % 3 != 0) {
                            let size: UInt32 = (next() & 1) == 0 ? 48 : 180
                            if let handle = pool.alloc(size: size, owner: UInt8((worker % 3) + 1)) {
                                if (next() & 1) == 0 {
                                    try pool.markInFlight(handle)
                                    try pool.free(handle)
                                } else {
                                    owned.append(handle)
                                }
                            }
                        } else {
                            let idx = Int(next() % UInt64(owned.count))
                            let handle = owned.remove(at: idx)
                            if pool.slotState(handle) == .allocated {
                                try pool.markInFlight(handle)
                            }
                            try pool.free(handle)
                        }
                    }

                    for handle in owned {
                        switch pool.slotState(handle) {
                        case .allocated:
                            try pool.freeAllocated(handle)
                        case .inFlight:
                            try pool.free(handle)
                        case .free:
                            break
                        }
                    }
                }
            }
            try await group.waitForAll()
        }

        #expect(try countNonFreeSlots(region: fixture.region, header: header, classes: classes) == 0)
    }
}

struct ShmGuestLifecycleTests {
    // r[verify shm.architecture]
    // r[verify shm.signal]
    // r[verify shm.topology]
    // r[verify shm.topology.peer-id]
    // r[verify shm.topology.max-guests]
    // r[verify shm.topology.communication]
    // r[verify shm.topology.bidirectional]
    // r[verify shm.guest.attach]
    // r[verify shm.guest.detach]
    @Test func attachDetachAndTicketValidation() throws {
        let path = tmpPath("guest-lifecycle.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(path: path, classes: [ShmVarSlotClass(slotSize: 256, count: 4)])

        let guest = try ShmGuestRuntime.attach(path: path)
        #expect(guest.peerId == 1)
        #expect(try guest.peerState() == .attached)

        guest.detach()
        #expect(try guest.peerState() == .goodbye)

        let badTicket = ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: -1)
        #expect(throws: ShmGuestAttachError.slotNotReserved) {
            _ = try ShmGuestRuntime.attach(ticket: badTicket)
        }

        _ = fixture
    }

    // r[verify shm.guest.attach]
    @Test func reservedTicketAttachSucceeds() throws {
        let path = tmpPath("guest-ticket.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(
            path: path,
            classes: [ShmVarSlotClass(slotSize: 256, count: 4)],
            reservedPeer: 1
        )

        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
        }

        let ticket = ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest)
        let guest = try ShmGuestRuntime.attach(ticket: ticket)
        #expect(guest.peerId == 1)
        #expect(try guest.peerState() == .attached)
        _ = fixture
    }

    // r[verify shm.guest.attach-failure]
    @Test func invalidTicketPeerIsRejected() throws {
        let path = tmpPath("guest-invalid-peer.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(path: path, maxGuests: 1, classes: [ShmVarSlotClass(slotSize: 256, count: 4)])

        let ticket = ShmBootstrapTicket(peerId: 2, hubPath: path, doorbellFd: -1)
        #expect(throws: ShmGuestAttachError.invalidTicketPeer(2)) {
            _ = try ShmGuestRuntime.attach(ticket: ticket)
        }
        _ = fixture
    }

    // r[verify shm.host.goodbye]
    // r[verify shm.guest.attach-failure]
    @Test func hostGoodbyeRejectsAttach() throws {
        let path = tmpPath("guest-host-goodbye.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(path: path, classes: [ShmVarSlotClass(slotSize: 256, count: 4)])

        var headerBytes = Array(try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize))
        writeU32LE(1, to: &headerBytes, at: 68)
        let headerView = try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
        headerView.copyBytes(from: headerBytes)

        #expect(throws: ShmGuestAttachError.hostGoodbye) {
            _ = try ShmGuestRuntime.attach(path: path)
        }
    }
}

struct ShmDoorbellAndPayloadTests {
    // r[verify zerocopy.send.shm]
    // r[verify zerocopy.recv.shm.inline]
    // r[verify zerocopy.recv.shm.slotref]
    // r[verify shm.signal.doorbell.integration]
    // r[verify shm.signal.doorbell.optional]
    // r[verify shm.framing.inline]
    // r[verify shm.framing.slot-ref]
    // r[verify shm.framing.threshold]
    @Test func mixedInlineAndSlotRefPathsRoundTrip() throws {
        let path = tmpPath("guest-payload.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(
            path: path,
            inlineThreshold: 64,
            classes: [ShmVarSlotClass(slotSize: 256, count: 4)],
            reservedPeer: 1
        )

        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
        }

        var state = fcntl(pair.guest, F_GETFL)
        _ = fcntl(pair.guest, F_SETFL, state | O_NONBLOCK)
        state = fcntl(pair.host, F_GETFL)
        _ = fcntl(pair.host, F_SETFL, state | O_NONBLOCK)

        let guest = try ShmGuestRuntime.attach(
            ticket: ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest)
        )

        let ringOffset = try #require(fixture.ringOffsets[1])
        let g2h = try ShmBipBuffer.attach(region: fixture.region, headerOffset: ringOffset)

        let inlinePayload = Array("small".utf8)
        try guest.send(frame: ShmGuestFrame(payload: inlinePayload))

        let firstReadable = try #require(g2h.tryRead())
        let firstDecoded = try decodeShmFrame(Array(firstReadable))
        guard case .inline(let header, let payload) = firstDecoded else {
            Issue.record("expected inline frame")
            return
        }
        #expect(payload.starts(with: inlinePayload))
        try g2h.release(header.totalLen)

        let largePayload = [UInt8](repeating: 0xAB, count: 120)
        try guest.send(frame: ShmGuestFrame(payload: largePayload))

        let secondReadable = try #require(g2h.tryRead())
        let secondDecoded = try decodeShmFrame(Array(secondReadable))
        guard case .slotRef(let slotHeader, let slotRef) = secondDecoded else {
            Issue.record("expected slot-ref frame")
            return
        }

        let segmentHeader = try ShmSegmentView(region: fixture.region).header
        let pool = try ShmVarSlotPool(
            region: fixture.region,
            baseOffset: Int(segmentHeader.varSlotPoolOffset),
            classes: fixture.classes
        )
        let handle = ShmVarSlotHandle(
            classIdx: slotRef.classIdx,
            extentIdx: slotRef.extentIdx,
            slotIdx: slotRef.slotIdx,
            generation: slotRef.slotGeneration
        )
        let payloadPtr = try #require(pool.payloadPointer(handle))
        let storedLen = payloadPtr.load(as: UInt32.self).littleEndian
        #expect(storedLen == UInt32(largePayload.count))
        let copied = Array(
            UnsafeRawBufferPointer(
                start: UnsafeRawPointer(payloadPtr.advanced(by: 4)),
                count: largePayload.count
            ))
        #expect(copied == largePayload)

        try g2h.release(slotHeader.totalLen)
        try pool.free(handle)
    }

    // r[verify zerocopy.recv.shm.mmap]
    @Test func mmapRefReceivePathResolvesAttachment() async throws {
        let path = tmpPath("guest-mmap-recv.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 4096,
                maxPayloadSize: 16 * 1024,
                inlineThreshold: 64,
                sizeClasses: [ShmVarSlotClass(slotSize: 64, count: 2)]
            )
        )
        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        let host = try prepared.intoTransport()
        let hostConduit = host.bareConduit()
        let guest = try ShmGuestRuntime.attach(ticket: ticket)
        try await withAsyncCleanup({
            try? await host.close()
        }) {
            let expected = String(repeating: "m", count: 5_000)
            try await hostConduit.send(.protocolError(description: expected))

            let frame = try #require(try guest.receive())
            let msg = try MessageV7.decode(from: Data(frame.payload))
            guard case .protocolError(let error) = msg.payload else {
                Issue.record("expected protocol error payload")
                return
            }
            #expect(error.description == expected)
        }
    }

    // r[verify shm.mmap.ordering]
    // r[verify zerocopy.send.shm]
    @Test func mmapRefSendPathEmitsAttachmentAndFrame() async throws {
        let path = tmpPath("guest-mmap-send.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 4096,
                maxPayloadSize: 16 * 1024,
                inlineThreshold: 64,
                sizeClasses: [ShmVarSlotClass(slotSize: 64, count: 2)]
            )
        )
        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        let host = try prepared.intoTransport()
        let hostConduit = host.bareConduit()
        let guest = try ShmGuestRuntime.attach(ticket: ticket)
        try await withAsyncCleanup({
            try? await host.close()
        }) {
            let expected = String(repeating: "g", count: 5_000)
            let outbound = MessageV7.protocolError(description: expected)
            try guest.send(frame: ShmGuestFrame(payload: outbound.encode()))

            let inbound = try await withThrowingTaskGroup(of: MessageV7?.self) { group in
                group.addTask {
                    try await hostConduit.recv()
                }
                group.addTask {
                    try await Task.sleep(nanoseconds: 1_000_000_000)
                    throw ShmHarnessError.timeout("host recv timed out")
                }
                let first = try await group.next()!
                group.cancelAll()
                return first
            }
            let msg = try #require(inbound)
            guard case .protocolError(let error) = msg.payload else {
                Issue.record("expected protocol error payload")
                return
            }
            #expect(error.description == expected)
        }
    }

    // r[verify zerocopy.recv.shm.mmap]
    // r[verify shm.mmap.attach]
    @Test func mmapRefReceiveFailsWhenAttachmentIsMissing() async throws {
        let path = tmpPath("guest-mmap-recv-missing-attachment.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 4096,
                maxPayloadSize: 16 * 1024,
                inlineThreshold: 64,
                sizeClasses: [ShmVarSlotClass(slotSize: 64, count: 2)]
            )
        )
        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        let host = try prepared.intoTransport()
        let hostConduit = host.bareConduit()
        let guest = try ShmGuestRuntime.attach(
            ticket: ShmBootstrapTicket(
                peerId: ticket.peerId,
                hubPath: ticket.hubPath,
                doorbellFd: ticket.doorbellFd,
                shmFd: ticket.shmFd,
                mmapControlFd: -1
            )
        )
        defer {
            if ticket.mmapControlFd >= 0 {
                close(ticket.mmapControlFd)
            }
        }
        try await withAsyncCleanup({
            try? await host.close()
        }) {
            let expected = String(repeating: "x", count: 5_000)
            try await hostConduit.send(.protocolError(description: expected))

            do {
                _ = try guest.receive()
                Issue.record("expected malformedFrame when mmap attachment is missing")
            } catch let error as ShmGuestReceiveError {
                #expect(error == .malformedFrame)
            }
        }
    }

    // r[verify zerocopy.send.shm]
    // r[verify shm.mmap.attach]
    @Test func mmapRefSendFailsFastWhenControlPeerIsClosed() async throws {
        let path = tmpPath("guest-mmap-send-broken-control.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let segment = try ShmHostSegment.create(
            path: path,
            config: ShmHostSegmentConfig(
                maxGuests: 1,
                bipbufCapacity: 4096,
                maxPayloadSize: 16 * 1024,
                inlineThreshold: 64,
                sizeClasses: [ShmVarSlotClass(slotSize: 64, count: 2)]
            )
        )
        let prepared = try segment.reservePeer()
        let ticket = try prepared.makeGuestTicket()
        let host = try prepared.intoTransport()
        let guest = try ShmGuestRuntime.attach(ticket: ticket)

        try await host.close()

        let payload = MessageV7.protocolError(description: String(repeating: "y", count: 5_000)).encode()
        let start = ContinuousClock.now
        do {
            try guest.send(frame: ShmGuestFrame(payload: payload))
            Issue.record("expected fast send failure after peer close")
        } catch let error as ShmGuestSendError {
            switch error {
            case .hostGoodbye, .doorbellPeerDead, .mmapControlError:
                break
            default:
                Issue.record("unexpected send error after peer close: \(error)")
            }
        }
        #expect(ContinuousClock.now - start < Duration.milliseconds(250))
    }

    // r[verify shm.signal.doorbell]
    // r[verify shm.signal.doorbell.signal]
    // r[verify shm.signal.doorbell.wait]
    @Test func doorbellSignalWaitDrain() throws {
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            close(pair.guest)
        }

        let host = ShmDoorbell(fd: pair.host)
        let guest = ShmDoorbell(fd: pair.guest)

        try guest.signal()
        #expect(try host.wait(timeoutMs: 1000) == .signaled)
        #expect(try host.wait(timeoutMs: 10) == .timeout)
    }

    // r[verify shm.signal.doorbell]
    // r[verify shm.signal.doorbell.signal]
    // r[verify shm.signal.doorbell.wait]
    @Test func doorbellBurstSignalsCoalesce() throws {
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            close(pair.guest)
        }

        let host = ShmDoorbell(fd: pair.host)
        let guest = ShmDoorbell(fd: pair.guest)

        for _ in 0..<32 {
            try guest.signal()
        }

        #expect(try host.wait(timeoutMs: 1000) == .signaled)
        #expect(try host.wait(timeoutMs: 10) == .timeout)
    }

    // r[verify shm.signal.doorbell.death]
    @Test func doorbellPeerDeathIsReported() throws {
        let pair = try makeDoorbellPair()
        defer { close(pair.host) }

        let host = ShmDoorbell(fd: pair.host)
        close(pair.guest)

        #expect(try host.wait(timeoutMs: 1000) == .peerDead)
    }
}

struct ShmGuestRemapTests {
    // r[verify transport.shm]
    @Test func closedTransportSendReturnsConnectionClosed() async throws {
        let path = tmpPath("transport-closed-send.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(path: path, classes: [ShmVarSlotClass(slotSize: 256, count: 2)])
        let transport = try ShmGuestTransport.attach(path: path)
        let conduit = transport.bareConduit()
        try await transport.close()

        do {
            try await conduit.send(.cancel(connId: 0, requestId: 1))
            Issue.record("expected connectionClosed")
        } catch {
            #expect(isConnectionClosedTransportError(error))
        }
        _ = fixture
    }

    // r[verify transport.shm]
    // r[verify shm.signal.doorbell.death]
    @Test func peerDeathInRecvReturnsConnectionClosed() async throws {
        let path = tmpPath("transport-peer-dead.bin")
        let pair = try makeDoorbellPair()
        defer {
            try? FileManager.default.removeItem(atPath: path)
        }

        let fixture = try makeSegmentFixture(
            path: path,
            classes: [ShmVarSlotClass(slotSize: 256, count: 2)],
            reservedPeer: 1
        )
        let transport = try ShmGuestTransport.attach(
            ticket: ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest)
        )
        let conduit = transport.bareConduit()

        close(pair.host)
        let start = ContinuousClock.now

        do {
            _ = try await withThrowingTaskGroup(of: MessageV7?.self) { group in
                group.addTask {
                    try await conduit.recv()
                }
                group.addTask {
                    try await Task.sleep(nanoseconds: 300_000_000)
                    throw ShmHarnessError.timeout("recv did not fail after peer death")
                }
                let first = try await group.next()
                group.cancelAll()
                return first ?? nil
            }
            Issue.record("expected connectionClosed")
        } catch {
            #expect(isConnectionClosedTransportError(error))
        }
        #expect(ContinuousClock.now - start < Duration.milliseconds(300))
        _ = fixture
    }

    // r[verify transport.shm]
    // r[verify shm.host.goodbye]
    // r[verify shm.signal.doorbell]
    @Test func hostGoodbyeWakeUnblocksRecvWithoutBusyLoop() async throws {
        let path = tmpPath("transport-host-goodbye-wake.bin")
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            try? FileManager.default.removeItem(atPath: path)
        }

        let fixture = try makeSegmentFixture(
            path: path,
            classes: [ShmVarSlotClass(slotSize: 256, count: 2)],
            reservedPeer: 1
        )
        let transport = try ShmGuestTransport.attach(
            ticket: ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest)
        )
        let conduit = transport.bareConduit()
        try await withAsyncCleanup({
            try? await transport.close()
        }) {
            let hostDoorbell = ShmDoorbell(fd: pair.host)
            let recvTask = Task<MessageV7?, Error> {
                try await conduit.recv()
            }
            defer { recvTask.cancel() }

            try await Task.sleep(nanoseconds: 20_000_000)
            try setHostGoodbye(fixture)
            try hostDoorbell.signal()

            let start = ContinuousClock.now
            let result = try await withThrowingTaskGroup(of: MessageV7?.self) { group in
                group.addTask {
                    try await recvTask.value
                }
                group.addTask {
                    try await Task.sleep(nanoseconds: 300_000_000)
                    throw ShmHarnessError.timeout("recv did not unblock after host goodbye wake")
                }
                let first = try await group.next()
                group.cancelAll()
                return first ?? nil
            }

            #expect(result == nil)
            #expect(ContinuousClock.now - start < Duration.milliseconds(300))
        }
    }

    @Test func remapOnCurrentSizeGrowth() throws {
        // r[verify shm.varslot.extents]
        let path = tmpPath("guest-remap.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(path: path, classes: [ShmVarSlotClass(slotSize: 256, count: 2)])
        let guest = try ShmGuestRuntime.attach(path: path)

        let newSize = fixture.region.length + 4096
        try fixture.region.resize(newSize: newSize)

        var header = Array(try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize))
        writeU64LE(UInt64(newSize), to: &header, at: 88)
        let headerBytes = try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
        headerBytes.copyBytes(from: header)

        #expect(try guest.checkRemap())
        #expect(guest.region.length == newSize)
        #expect(!(try guest.checkRemap()))
    }

    // r[verify shm.varslot.extents.notification]
    @Test func doorbellWakeTriggersRemapFromCurrentSize() throws {
        let path = tmpPath("guest-remap-doorbell.bin")
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            try? FileManager.default.removeItem(atPath: path)
        }

        let fixture = try makeSegmentFixture(
            path: path,
            classes: [ShmVarSlotClass(slotSize: 256, count: 2)],
            reservedPeer: 1
        )
        let guest = try ShmGuestRuntime.attach(
            ticket: ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest)
        )

        let newSize = fixture.region.length + 4096
        try fixture.region.resize(newSize: newSize)
        var header = Array(try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize))
        writeU64LE(UInt64(newSize), to: &header, at: 88)
        let headerBytes = try fixture.region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
        headerBytes.copyBytes(from: header)

        let hostDoorbell = ShmDoorbell(fd: pair.host)
        try hostDoorbell.signal()

        #expect(try guest.waitForDoorbell(timeoutMs: 1000) == .signaled)
        #expect(guest.region.length == newSize)
    }
}

struct ShmDriverRaceTests {
    // r[verify transport.shm]
    @Test func timedOutCallSendsCancelOverShm() async throws {
        let path = tmpPath("driver-timeout-cancel.bin")
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            try? FileManager.default.removeItem(atPath: path)
        }

        let fixture = try makeSegmentFixture(
            path: path,
            bipbufCapacity: 2048,
            inlineThreshold: 1024,
            maxPayloadSize: 1_000_000,
            classes: [ShmVarSlotClass(slotSize: 4096, count: 8)],
            reservedPeer: 1
        )
        let hostPeer = try hostPeerBuffers(fixture: fixture, peerId: 1)
        let hostDoorbell = ShmDoorbell(fd: pair.host)
        let (transport, handle, driver) = try await establishShmInitiator(
            path: path,
            guestDoorbellFd: pair.guest,
            hostDoorbell: hostDoorbell,
            guestToHost: hostPeer.guestToHost,
            hostToGuest: hostPeer.hostToGuest
        )

        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask = Task {
                try await handle.callRaw(methodId: 42, payload: [1, 2, 3], timeout: 0.05)
            }

            guard let requestMessage = try await hostReadMessage(from: hostPeer.guestToHost) else {
                throw ShmHarnessError.timeout("did not receive request call")
            }
            let requestId: UInt64
            switch requestMessage.payload {
            case .requestMessage(let request):
                guard case .call = request.body else {
                    throw ShmHarnessError.unexpectedFrame("expected request call message")
                }
                requestId = request.id
            default:
                throw ShmHarnessError.unexpectedFrame("expected request message payload")
            }

            do {
                _ = try await callTask.value
                Issue.record("expected timeout")
            } catch {
                #expect(isTimeoutConnectionError(error))
            }

            guard let cancelMessage = try await hostReadMessage(from: hostPeer.guestToHost) else {
                throw ShmHarnessError.timeout("did not receive cancel message after timeout")
            }
            switch cancelMessage.payload {
            case .requestMessage(let request):
                guard case .cancel = request.body else {
                    throw ShmHarnessError.unexpectedFrame("expected request cancel message")
                }
                #expect(request.id == requestId)
            default:
                throw ShmHarnessError.unexpectedFrame("expected request message payload for cancel")
            }
        }
    }

    // r[verify transport.shm]
    @Test func pipelinedInFlightCallsFailFastWhenHostGoodbyeRaces() async throws {
        let path = tmpPath("driver-pipeline-goodbye-race.bin")
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            try? FileManager.default.removeItem(atPath: path)
        }

        let fixture = try makeSegmentFixture(
            path: path,
            bipbufCapacity: 2048,
            inlineThreshold: 1024,
            maxPayloadSize: 1_000_000,
            classes: [ShmVarSlotClass(slotSize: 4096, count: 16)],
            reservedPeer: 1
        )
        let hostPeer = try hostPeerBuffers(fixture: fixture, peerId: 1)
        let hostDoorbell = ShmDoorbell(fd: pair.host)
        let (transport, handle, driver) = try await establishShmInitiator(
            path: path,
            guestDoorbellFd: pair.guest,
            hostDoorbell: hostDoorbell,
            guestToHost: hostPeer.guestToHost,
            hostToGuest: hostPeer.hostToGuest
        )

        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let callCount = 24
            let calls = (0..<callCount).map { idx in
                Task<Result<[UInt8], Error>, Never> {
                    do {
                        let response = try await handle.callRaw(
                            methodId: UInt64(100 + idx),
                            payload: [UInt8(truncatingIfNeeded: idx)],
                            timeout: 1.0
                        )
                        return .success(response)
                    } catch {
                        return .failure(error)
                    }
                }
            }

            var seenCalls = 0
            let readStart = ContinuousClock.now
            let readBudget = Duration.milliseconds(400)
            while seenCalls < 6 && ContinuousClock.now - readStart < readBudget {
                if let msg = try await hostReadMessage(from: hostPeer.guestToHost, timeoutMs: 25),
                   case .requestMessage(let request) = msg.payload,
                   case .call = request.body
                {
                    seenCalls += 1
                }
            }
            #expect(seenCalls >= 2)

            try setHostGoodbye(fixture)
            try hostDoorbell.signal()

            var results: [Result<[UInt8], Error>] = []
            results.reserveCapacity(calls.count)
            for task in calls {
                results.append(await task.value)
            }

            let successCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial + 1
                case .failure:
                    return partial
                }
            }
            let closedCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isConnectionClosedConnectionError(error) ? 1 : 0)
                }
            }
            let timeoutCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isTimeoutConnectionError(error) ? 1 : 0)
                }
            }
            let transportErrorCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isTransportConnectionError(error) ? 1 : 0)
                }
            }

            #expect(successCount == 0)
            #expect(timeoutCount == 0)
            #expect(closedCount > 0)
            #expect(closedCount + transportErrorCount == callCount)
        }
    }
}

@inline(__always)
private func alignUp(_ value: Int, to alignment: Int) -> Int {
    let mask = alignment - 1
    return (value + mask) & ~mask
}

private func countNonFreeSlots(
    region: ShmRegion,
    header: ShmSegmentHeader,
    classes: [ShmVarSlotClass]
) throws -> Int {
    let baseOffset = Int(header.varSlotPoolOffset)
    let headerSize = classes.count * 64
    var offset = baseOffset + headerSize
    var nonFree = 0

    for cls in classes {
        offset = alignUp(offset, to: 16)
        let metaBase = offset
        offset += Int(cls.count) * 16
        offset = alignUp(offset, to: 64)
        offset += Int(cls.count) * Int(cls.slotSize)

        for slot in 0..<Int(cls.count) {
            let metaOffset = metaBase + slot * 16
            let bytes = Array(try region.mutableBytes(at: metaOffset + 4, count: 4))
            let state = readU32LE(bytes, at: 0)
            if state != ShmSlotState.free.rawValue {
                nonFree += 1
            }
        }
    }

    return nonFree
}

@inline(__always)
private func writeU32LE(_ value: UInt32, to bytes: inout [UInt8], at index: Int) {
    let le = value.littleEndian
    bytes[index] = UInt8(truncatingIfNeeded: le)
    bytes[index + 1] = UInt8(truncatingIfNeeded: le >> 8)
    bytes[index + 2] = UInt8(truncatingIfNeeded: le >> 16)
    bytes[index + 3] = UInt8(truncatingIfNeeded: le >> 24)
}

@inline(__always)
private func readU32LE(_ bytes: [UInt8], at index: Int) -> UInt32 {
    UInt32(bytes[index])
        | (UInt32(bytes[index + 1]) << 8)
        | (UInt32(bytes[index + 2]) << 16)
        | (UInt32(bytes[index + 3]) << 24)
}

@inline(__always)
private func writeU64LE(_ value: UInt64, to bytes: inout [UInt8], at index: Int) {
    let le = value.littleEndian
    bytes[index] = UInt8(truncatingIfNeeded: le)
    bytes[index + 1] = UInt8(truncatingIfNeeded: le >> 8)
    bytes[index + 2] = UInt8(truncatingIfNeeded: le >> 16)
    bytes[index + 3] = UInt8(truncatingIfNeeded: le >> 24)
    bytes[index + 4] = UInt8(truncatingIfNeeded: le >> 32)
    bytes[index + 5] = UInt8(truncatingIfNeeded: le >> 40)
    bytes[index + 6] = UInt8(truncatingIfNeeded: le >> 48)
    bytes[index + 7] = UInt8(truncatingIfNeeded: le >> 56)
}
#endif
