#if os(macOS)
import Darwin
import Foundation
import Testing

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

    let pool = ShmVarSlotPool(region: region, baseOffset: varPoolOffset, classes: classes)
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

struct ShmVarSlotPoolTests {
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
        let pool = ShmVarSlotPool(
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

    @Test func stressChurnEndsWithNoLeakedSlots() async throws {
        let path = tmpPath("varslot-stress.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let classes = [
            ShmVarSlotClass(slotSize: 64, count: 64),
            ShmVarSlotClass(slotSize: 256, count: 32),
        ]
        let fixture = try makeSegmentFixture(path: path, classes: classes)
        let header = try ShmSegmentView(region: fixture.region).header
        let pool = ShmVarSlotPool(
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
            close(pair.guest)
        }

        let ticket = ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest)
        let guest = try ShmGuestRuntime.attach(ticket: ticket)
        #expect(guest.peerId == 1)
        #expect(try guest.peerState() == .attached)
        _ = fixture
    }

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
            close(pair.guest)
        }

        var state = fcntl(pair.guest, F_GETFL)
        _ = fcntl(pair.guest, F_SETFL, state | O_NONBLOCK)
        state = fcntl(pair.host, F_GETFL)
        _ = fcntl(pair.host, F_SETFL, state | O_NONBLOCK)

        let guest = try ShmGuestRuntime.attach(ticket: ShmBootstrapTicket(peerId: 1, hubPath: path, doorbellFd: pair.guest))

        let ringOffset = try #require(fixture.ringOffsets[1])
        let g2h = try ShmBipBuffer.attach(region: fixture.region, headerOffset: ringOffset)

        let inlinePayload = Array("small".utf8)
        try guest.send(frame: ShmGuestFrame(msgType: 1, id: 10, methodId: 99, payload: inlinePayload))

        let firstReadable = try #require(g2h.tryRead())
        let firstDecoded = try decodeShmFrame(Array(firstReadable))
        guard case .inline(let header, let payload) = firstDecoded else {
            Issue.record("expected inline frame")
            return
        }
        #expect(header.id == 10)
        #expect(payload == inlinePayload)
        try g2h.release(header.totalLen)

        let largePayload = [UInt8](repeating: 0xAB, count: 120)
        try guest.send(frame: ShmGuestFrame(msgType: 2, id: 11, methodId: 100, payload: largePayload))

        let secondReadable = try #require(g2h.tryRead())
        let secondDecoded = try decodeShmFrame(Array(secondReadable))
        guard case .slotRef(let slotHeader, let slotRef) = secondDecoded else {
            Issue.record("expected slot-ref frame")
            return
        }
        #expect(slotHeader.id == 11)

        let segmentHeader = try ShmSegmentView(region: fixture.region).header
        let pool = ShmVarSlotPool(
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
        let copied = Array(UnsafeRawBufferPointer(start: UnsafeRawPointer(payloadPtr), count: largePayload.count))
        #expect(copied == largePayload)

        try g2h.release(slotHeader.totalLen)
        try pool.free(handle)
    }

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

    @Test func doorbellPeerDeathIsReported() throws {
        let pair = try makeDoorbellPair()
        defer { close(pair.host) }

        let host = ShmDoorbell(fd: pair.host)
        close(pair.guest)

        #expect(try host.wait(timeoutMs: 1000) == .peerDead)
    }
}

struct ShmGuestRemapTests {
    @Test func closedTransportSendReturnsConnectionClosed() async throws {
        let path = tmpPath("transport-closed-send.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let fixture = try makeSegmentFixture(path: path, classes: [ShmVarSlotClass(slotSize: 256, count: 2)])
        let transport = try ShmGuestTransport.attach(path: path)
        try await transport.close()

        do {
            try await transport.send(.cancel(connId: 0, requestId: 1))
            Issue.record("expected connectionClosed")
        } catch {
            #expect(isConnectionClosedTransportError(error))
        }
        _ = fixture
    }

    @Test func peerDeathInRecvReturnsConnectionClosed() async throws {
        let path = tmpPath("transport-peer-dead.bin")
        let pair = try makeDoorbellPair()
        defer {
            close(pair.host)
            close(pair.guest)
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

        close(pair.host)

        do {
            _ = try await transport.recv()
            Issue.record("expected connectionClosed")
        } catch {
            #expect(isConnectionClosedTransportError(error))
        }
        _ = fixture
    }

    @Test func remapOnCurrentSizeGrowth() throws {
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
