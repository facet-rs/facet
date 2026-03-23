#if os(macOS)
import Darwin
import Foundation
import CVoxShmFfi

// r[impl shm.segment.config]
public struct ShmHostSegmentConfig: Sendable {
    public var maxGuests: UInt8
    public var bipbufCapacity: UInt32
    public var maxPayloadSize: UInt32
    public var inlineThreshold: UInt32
    public var heartbeatInterval: UInt64
    public var sizeClasses: [ShmVarSlotClass]

    public init(
        maxGuests: UInt8 = 1,
        bipbufCapacity: UInt32 = 64 * 1024,
        maxPayloadSize: UInt32 = 1024 * 1024,
        inlineThreshold: UInt32 = shmDefaultInlineThreshold,
        heartbeatInterval: UInt64 = 0,
        sizeClasses: [ShmVarSlotClass] = [ShmVarSlotClass(slotSize: 4096, count: 8)]
    ) {
        self.maxGuests = maxGuests
        self.bipbufCapacity = bipbufCapacity
        self.maxPayloadSize = maxPayloadSize
        self.inlineThreshold = inlineThreshold
        self.heartbeatInterval = heartbeatInterval
        self.sizeClasses = sizeClasses
    }
}

public enum ShmHostSegmentError: Error, Equatable {
    case invalidMaxGuests(UInt8)
    case invalidBipbufCapacity(UInt32)
    case missingVarSlotClasses
    case noFreePeerSlots
    case invalidPeerId(UInt8)
    case socketPairFailed(errno: Int32)
}

public enum ShmHostPeerError: Error, Equatable {
    case alreadyConsumed
    case bootstrapSendFailed
    case fdDupFailed(errno: Int32)
}

public enum ShmHostSendError: Error, Equatable {
    case peerClosed
    case payloadTooLarge
    case ringFull
    case slotExhausted
    case slotError
    case mmapAllocationFailed
    case mmapUnavailable
    case mmapControlError(errno: Int32)
}

public enum ShmHostReceiveError: Error, Equatable {
    case malformedFrame
    case slotError
    case payloadTooLarge
}

// r[impl shm.segment]
// r[impl shm.peer-table]
// r[impl shm.spawn]
public final class ShmHostSegment: @unchecked Sendable {
    public let path: String
    public private(set) var header: ShmSegmentHeader

    fileprivate let region: ShmRegion
    fileprivate let classes: [ShmVarSlotClass]
    fileprivate let varPoolOffset: Int
    fileprivate let maxVarSlotPayload: UInt32

    private let ringBaseOffset: Int
    private let ringStride: Int
    private let lock = NSLock()
    private let slotPool: ShmVarSlotPool

    public static func create(
        path: String,
        config: ShmHostSegmentConfig,
        cleanup: ShmFileCleanup = .manual
    ) throws -> ShmHostSegment {
        guard config.maxGuests > 0 else {
            throw ShmHostSegmentError.invalidMaxGuests(config.maxGuests)
        }
        guard config.bipbufCapacity > 0 else {
            throw ShmHostSegmentError.invalidBipbufCapacity(config.bipbufCapacity)
        }
        guard !config.sizeClasses.isEmpty else {
            throw ShmHostSegmentError.missingVarSlotClasses
        }

        let peerTableOffset = shmSegmentHeaderSize
        let peerTableSize = Int(config.maxGuests) * shmPeerEntrySize
        let ringBaseOffset = peerTableOffset + peerTableSize
        let ringStride = 2 * (shmBipbufHeaderSize + Int(config.bipbufCapacity))
        let ringAreaSize = Int(config.maxGuests) * ringStride
        let varPoolOffset = shmHostAlignUp(ringBaseOffset + ringAreaSize, to: 64)
        let varPoolSize = ShmVarSlotPool.calculateSize(classes: config.sizeClasses)
        let totalSize = varPoolOffset + varPoolSize

        let region = try ShmRegion.create(path: path, size: totalSize, cleanup: cleanup)
        try writeV7SegmentHeader(
            to: region,
            totalSize: totalSize,
            config: config,
            peerTableOffset: peerTableOffset,
            varPoolOffset: varPoolOffset
        )

        let slotPool = try ShmVarSlotPool(
            region: region,
            baseOffset: varPoolOffset,
            classes: config.sizeClasses
        )
        slotPool.initialize()

        for i in 0..<Int(config.maxGuests) {
            let peerId = UInt8(i + 1)
            let peerOffset = peerTableOffset + i * shmPeerEntrySize
            let ringOffset = ringBaseOffset + i * ringStride

            var entry = [UInt8](repeating: 0, count: shmPeerEntrySize)
            writeU32LEHost(ShmPeerState.empty.rawValue, to: &entry, at: 0)
            writeU32LEHost(0, to: &entry, at: 4)
            writeU64LEHost(0, to: &entry, at: 8)
            writeU64LEHost(UInt64(ringOffset), to: &entry, at: 16)
            let entryBytes = try region.mutableBytes(at: peerOffset, count: shmPeerEntrySize)
            entryBytes.copyBytes(from: entry)

            _ = try ShmBipBuffer.initialize(region: region, headerOffset: ringOffset, capacity: config.bipbufCapacity)
            _ = try ShmBipBuffer.initialize(
                region: region,
                headerOffset: ringOffset + shmBipbufHeaderSize + Int(config.bipbufCapacity),
                capacity: config.bipbufCapacity
            )

            _ = peerId
        }

        let header = try ShmSegmentView(region: region).header
        let maxVarSlotPayload = shmMaxVarSlotPayload(classes: config.sizeClasses)

        return ShmHostSegment(
            path: path,
            header: header,
            region: region,
            classes: config.sizeClasses,
            varPoolOffset: varPoolOffset,
            maxVarSlotPayload: maxVarSlotPayload,
            ringBaseOffset: ringBaseOffset,
            ringStride: ringStride,
            slotPool: slotPool
        )
    }

    private init(
        path: String,
        header: ShmSegmentHeader,
        region: ShmRegion,
        classes: [ShmVarSlotClass],
        varPoolOffset: Int,
        maxVarSlotPayload: UInt32,
        ringBaseOffset: Int,
        ringStride: Int,
        slotPool: ShmVarSlotPool
    ) {
        self.path = path
        self.header = header
        self.region = region
        self.classes = classes
        self.varPoolOffset = varPoolOffset
        self.maxVarSlotPayload = maxVarSlotPayload
        self.ringBaseOffset = ringBaseOffset
        self.ringStride = ringStride
        self.slotPool = slotPool
    }

    public func reservePeer() throws -> ShmPreparedHostPeer {
        lock.lock()
        defer { lock.unlock() }

        let maxGuests = UInt8(header.maxGuests)
        for peerId in UInt8(1)...maxGuests {
            let statePtr = try peerStatePointer(peerId: peerId)
            if transitionShmPeerState(statePtr: statePtr, from: .empty, to: .reserved, bumpEpochOnSuccess: true) {
                let doorbellPair = try makeStreamSocketPair()
                let mmapPair = try makeDatagramSocketPair()
                return ShmPreparedHostPeer(
                    segment: self,
                    peerId: peerId,
                    hostDoorbellFd: doorbellPair[0],
                    guestDoorbellFd: doorbellPair[1],
                    hostMmapControlFd: mmapPair[0],
                    guestMmapControlFd: mmapPair[1]
                )
            }
        }

        throw ShmHostSegmentError.noFreePeerSlots
    }

    public func releaseReservedPeer(peerId: UInt8) {
        lock.lock()
        defer { lock.unlock() }
        do {
            let statePtr = try peerStatePointer(peerId: peerId)
            _ = transitionShmPeerState(statePtr: statePtr, from: .reserved, to: .empty)
        } catch {
            return
        }
    }

    fileprivate func setHostGoodbye() throws {
        let ptr = try region.pointer(at: 64)
        atomicStoreU32Release(ptr, 1)
    }

    fileprivate func peerState(peerId: UInt8) throws -> UInt32 {
        let ptr = try peerStatePointer(peerId: peerId)
        return atomicLoadU32Acquire(ptr)
    }

    fileprivate func peerStatePointer(peerId: UInt8) throws -> UnsafeMutableRawPointer {
        guard peerId >= 1, UInt32(peerId) <= header.maxGuests else {
            throw ShmHostSegmentError.invalidPeerId(peerId)
        }
        do {
            return try shmPeerStatePointer(region: region, header: header, peerId: peerId)
        } catch ShmLayoutError.invalidPeerId {
            throw ShmHostSegmentError.invalidPeerId(peerId)
        } catch {
            throw error
        }
    }

    fileprivate func bipBuffers(peerId: UInt8) throws -> (guestToHost: ShmBipBuffer, hostToGuest: ShmBipBuffer) {
        guard peerId >= 1, UInt32(peerId) <= header.maxGuests else {
            throw ShmHostSegmentError.invalidPeerId(peerId)
        }
        let view = try ShmSegmentView(region: region)
        return try attachShmPeerBuffers(region: region, view: view, peerId: peerId)
    }

    fileprivate func makeRuntimeSlotPool() throws -> ShmVarSlotPool {
        try ShmVarSlotPool(region: region, baseOffset: varPoolOffset, classes: classes)
    }
}

public final class ShmPreparedHostPeer: @unchecked Sendable {
    public let peerId: UInt8

    private let segment: ShmHostSegment
    private var hostDoorbellFd: Int32
    private var guestDoorbellFd: Int32
    private var hostMmapControlFd: Int32
    private var guestMmapControlFd: Int32
    private var consumed = false
    private var guestEndpointsSent = false

    fileprivate init(
        segment: ShmHostSegment,
        peerId: UInt8,
        hostDoorbellFd: Int32,
        guestDoorbellFd: Int32,
        hostMmapControlFd: Int32,
        guestMmapControlFd: Int32
    ) {
        self.segment = segment
        self.peerId = peerId
        self.hostDoorbellFd = hostDoorbellFd
        self.guestDoorbellFd = guestDoorbellFd
        self.hostMmapControlFd = hostMmapControlFd
        self.guestMmapControlFd = guestMmapControlFd
    }

    deinit {
        if !consumed {
            segment.releaseReservedPeer(peerId: peerId)
        }
        closeIfValid(&hostDoorbellFd)
        closeIfValid(&guestDoorbellFd)
        closeIfValid(&hostMmapControlFd)
        closeIfValid(&guestMmapControlFd)
    }

    public func makeGuestTicket(hubPath: String? = nil) throws -> ShmBootstrapTicket {
        let path = hubPath ?? segment.path
        let doorbellDup = dup(guestDoorbellFd)
        guard doorbellDup >= 0 else {
            throw ShmHostPeerError.fdDupFailed(errno: errno)
        }

        let shmDup = dup(segment.region.rawFd)
        guard shmDup >= 0 else {
            close(doorbellDup)
            throw ShmHostPeerError.fdDupFailed(errno: errno)
        }

        let mmapDup = dup(guestMmapControlFd)
        guard mmapDup >= 0 else {
            close(doorbellDup)
            close(shmDup)
            throw ShmHostPeerError.fdDupFailed(errno: errno)
        }

        return ShmBootstrapTicket(
            peerId: peerId,
            hubPath: path,
            doorbellFd: doorbellDup,
            shmFd: shmDup,
            mmapControlFd: mmapDup
        )
    }

    public func sendBootstrapSuccess(
        controlFd: Int32,
        hubPath: String? = nil
    ) throws {
        let payloadPath = hubPath ?? segment.path
        let payload = [UInt8](payloadPath.utf8)

        let rc = payload.withUnsafeBufferPointer { buf in
            vox_shm_bootstrap_response_send_unix(
                controlFd,
                0,
                UInt32(peerId),
                buf.baseAddress,
                UInt(buf.count),
                guestDoorbellFd,
                segment.region.rawFd,
                guestMmapControlFd
            )
        }

        guard rc == 0 else {
            throw ShmHostPeerError.bootstrapSendFailed
        }

        guestEndpointsSent = true
    }

    /// Close guest-side endpoints after bootstrap handoff is known to be complete.
    ///
    /// Keeping these fds alive until the peer completes startup avoids
    /// startup races where the handoff can be observed as prematurely closed.
    public func closeGuestEndpoints() {
        closeIfValid(&guestDoorbellFd)
        closeIfValid(&guestMmapControlFd)
    }

    public func intoTransport() throws -> ShmHostTransport {
        if consumed {
            throw ShmHostPeerError.alreadyConsumed
        }
        let runtime = try ShmHostRuntime.attach(
            segment: segment,
            peerId: peerId,
            doorbellFd: hostDoorbellFd,
            mmapControlFd: hostMmapControlFd
        )
        hostDoorbellFd = -1
        hostMmapControlFd = -1
        consumed = true
        return ShmHostTransport(runtime: runtime)
    }

    public func releaseReservation() {
        if !consumed {
            segment.releaseReservedPeer(peerId: peerId)
        }
        closeIfValid(&hostDoorbellFd)
        closeIfValid(&guestDoorbellFd)
        closeIfValid(&hostMmapControlFd)
        closeIfValid(&guestMmapControlFd)
    }
}

public final class ShmHostRuntime: @unchecked Sendable {
    public let peerId: UInt8
    public private(set) var region: ShmRegion
    public private(set) var header: ShmSegmentHeader

    private let segment: ShmHostSegment
    private var guestToHost: ShmBipBuffer
    private var hostToGuest: ShmBipBuffer
    private var slotPool: ShmVarSlotPool
    private let doorbell: ShmDoorbell?
    private let mmapAttachments: ShmMmapAttachments?
    private var mmapControlFd: Int32
    private let maxVarSlotPayload: UInt32
    private let outboundMmapAllocator = ShmOutboundMmapAllocator(pathPrefix: "vox-swift-host-mmap-")
    private var fatalError = false

    public static func attach(
        segment: ShmHostSegment,
        peerId: UInt8,
        doorbellFd: Int32,
        mmapControlFd: Int32
    ) throws -> ShmHostRuntime {
        let header = try ShmSegmentView(region: segment.region).header
        let buffers = try segment.bipBuffers(peerId: peerId)
        let pool = try segment.makeRuntimeSlotPool()
        let endpoints = makeShmControlEndpoints(doorbellFd: doorbellFd, mmapControlFd: mmapControlFd)

        return ShmHostRuntime(
            peerId: peerId,
            segment: segment,
            region: segment.region,
            header: header,
            guestToHost: buffers.guestToHost,
            hostToGuest: buffers.hostToGuest,
            slotPool: pool,
            doorbell: endpoints.doorbell,
            mmapAttachments: endpoints.mmapAttachments,
            mmapControlFd: mmapControlFd,
            maxVarSlotPayload: segment.maxVarSlotPayload
        )
    }

    private init(
        peerId: UInt8,
        segment: ShmHostSegment,
        region: ShmRegion,
        header: ShmSegmentHeader,
        guestToHost: ShmBipBuffer,
        hostToGuest: ShmBipBuffer,
        slotPool: ShmVarSlotPool,
        doorbell: ShmDoorbell?,
        mmapAttachments: ShmMmapAttachments?,
        mmapControlFd: Int32,
        maxVarSlotPayload: UInt32
    ) {
        self.peerId = peerId
        self.segment = segment
        self.region = region
        self.header = header
        self.guestToHost = guestToHost
        self.hostToGuest = hostToGuest
        self.slotPool = slotPool
        self.doorbell = doorbell
        self.mmapAttachments = mmapAttachments
        self.mmapControlFd = mmapControlFd
        self.maxVarSlotPayload = maxVarSlotPayload
    }

    deinit {
        closeMmapControlFd()
    }

    public func send(frame: ShmGuestFrame) throws {
        _ = try checkRemap()

        if fatalError || hostGoodbyeFlag() || peerGoodbyeFlag() {
            throw ShmHostSendError.peerClosed
        }
        try sendShmFrame(
            role: "host",
            frame: frame,
            header: header,
            outbox: hostToGuest,
            slotPool: slotPool,
            slotOwner: 0,
            doorbell: doorbell,
            maxVarSlotPayload: maxVarSlotPayload,
            mmapControlFd: mmapControlFd,
            errors: ShmSendErrors<ShmHostSendError>(
                payloadTooLarge: .payloadTooLarge,
                ringFull: .ringFull,
                slotExhausted: .slotExhausted,
                slotError: .slotError,
                mmapUnavailable: .mmapUnavailable
            )
        ) { payload, payloadLen in
            let allocation = try self.outboundMmapAllocator.allocateSlice(
                payloadCount: Int(payloadLen),
                mmapControlFd: self.mmapControlFd,
                allocationFailed: ShmHostSendError.mmapAllocationFailed,
                controlError: { ShmHostSendError.mmapControlError(errno: $0) }
            )

            payload.withUnsafeBytes { raw in
                if let base = raw.baseAddress {
                    memcpy(
                        allocation.region.basePointer().advanced(by: allocation.mapOffset),
                        base,
                        raw.count
                    )
                }
            }

            return ShmMmapRef(
                mapId: allocation.mapId,
                mapGeneration: allocation.mapGeneration,
                mapOffset: UInt64(allocation.mapOffset),
                payloadLen: payloadLen
            )
        }
    }

    public func receive() throws -> ShmGuestFrame? {
        _ = try checkRemap()

        if shouldTerminateShmReceive(
            fatalError: fatalError,
            sawHostGoodbye: hostGoodbyeFlag(),
            sawPeerGoodbye: peerGoodbyeFlag()
        ) {
            return nil
        }

        guard let readable = guestToHost.tryRead() else {
            return nil
        }

        do {
            return try receiveShmFrame(
                bytes: Array(readable),
                maxPayloadSize: self.header.maxPayloadSize,
                inbox: guestToHost,
                slotPool: slotPool,
                doorbell: doorbell,
                mmapAttachments: mmapAttachments,
                errors: ShmReceiveErrors<ShmHostReceiveError>(
                    malformedFrame: .malformedFrame,
                    slotError: .slotError,
                    payloadTooLarge: .payloadTooLarge
                )
            )
        } catch let error as ShmHostReceiveError {
            fatalError = true
            throw error
        }
    }

    public func detach() {
        detachShmRuntime(
            statePtr: try? segment.peerStatePointer(peerId: peerId),
            doorbell: doorbell,
            drain: { try self.receive() },
            closeMmapControl: { self.closeMmapControlFd() }
        )
    }

    public func isHostGoodbye() -> Bool {
        hostGoodbyeFlag()
    }

    public func isPeerGoodbye() -> Bool {
        peerGoodbyeFlag()
    }

    public func signalDoorbell() throws {
        try doorbell?.signal()
    }

    public func waitForDoorbell(timeoutMs: Int32? = nil) throws -> ShmDoorbellWaitResult? {
        try waitForShmDoorbell(doorbell: doorbell, timeoutMs: timeoutMs) {
            _ = try checkRemap()
        }
    }

    public func checkRemap() throws -> Bool {
        try checkShmRemap(region: region, header: &header) { _ in
            let buffers = try segment.bipBuffers(peerId: peerId)
            guestToHost = buffers.guestToHost
            hostToGuest = buffers.hostToGuest
            slotPool.updateRegion(region)
        }
    }

    private func hostGoodbyeFlag() -> Bool {
        shmHostGoodbyeFlag(region: region, header: header)
    }

    private func peerGoodbyeFlag() -> Bool {
        guard let state = try? segment.peerState(peerId: peerId) else {
            return true
        }
        return state == ShmPeerState.goodbye.rawValue
    }

    private func closeMmapControlFd() {
        closeShmMmapControlFd(&mmapControlFd)
    }
}

// r[impl transport.shm]
public final class ShmHostTransport: Link, @unchecked Sendable {
    public let negotiated: Negotiated

    private let lock = NSLock()
    private var runtime: ShmHostRuntime
    private var maxFrameSize: Int
    private var closed = false

    public init(runtime: ShmHostRuntime) {
        self.runtime = runtime
        self.maxFrameSize = Int(runtime.header.maxPayloadSize) + 64
        self.negotiated = Negotiated(
            maxPayloadSize: runtime.header.maxPayloadSize,
            initialCredit: 64 * 1024,
            maxConcurrentRequests: UInt32.max
        )
    }

    public func sendFrame(_ bytes: [UInt8]) async throws {
        try await sendRawPrologue(bytes)
    }

    public func sendRawPrologue(_ bytes: [UInt8]) async throws {
        try sendShmTransportFrame(
            bytes: bytes,
            negotiated: negotiated,
            maxFrameSize: maxFrameSize,
            sendErrorPrefix: "shm host send failed",
            mapSendError: { (err: ShmHostSendError) in
                if ProcessInfo.processInfo.environment["VOX_SHM_DEBUG"] == "1" {
                    fputs("[shm-host-transport] send error: \(err)\n", stderr)
                }
                switch err {
                case .ringFull, .slotExhausted:
                    return .wouldBlock
                case .peerClosed:
                    return .connectionClosed
                case .payloadTooLarge, .slotError, .mmapAllocationFailed, .mmapUnavailable, .mmapControlError:
                    return .transportIO("shm host send failed: \(err)")
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
            receiveErrorPrefix: "shm host receive failed",
            pollLockedReceive: {
                try lock.withShmLock {
                    if closed {
                        return ShmTransportReceivePoll(isClosed: true, frame: nil, sawGoodbye: false)
                    }
                    _ = try runtime.checkRemap()
                    return ShmTransportReceivePoll(
                        isClosed: false,
                        frame: try runtime.receive(),
                        sawGoodbye: runtime.isHostGoodbye() || runtime.isPeerGoodbye()
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
                runtime.isHostGoodbye() || runtime.isPeerGoodbye()
            }
        )
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        lock.withShmLock {
            maxFrameSize = size
        }
    }

    public func close() async throws {
        lock.withShmLock {
            if closed {
                return
            }
            closed = true
            runtime.detach()
        }
    }
}

@inline(__always)
private func makeStreamSocketPair() throws -> [Int32] {
    var fds = [Int32](repeating: -1, count: 2)
    guard socketpair(AF_UNIX, SOCK_STREAM, 0, &fds) == 0 else {
        throw ShmHostSegmentError.socketPairFailed(errno: errno)
    }
    return fds
}

@inline(__always)
private func makeDatagramSocketPair() throws -> [Int32] {
    var fds = [Int32](repeating: -1, count: 2)
    guard socketpair(AF_UNIX, SOCK_DGRAM, 0, &fds) == 0 else {
        throw ShmHostSegmentError.socketPairFailed(errno: errno)
    }
    return fds
}

private func writeV7SegmentHeader(
    to region: ShmRegion,
    totalSize: Int,
    config: ShmHostSegmentConfig,
    peerTableOffset: Int,
    varPoolOffset: Int
) throws {
    var header = [UInt8](repeating: 0, count: shmSegmentHeaderSize)
    for (idx, b) in shmSegmentMagic.enumerated() {
        header[idx] = b
    }
    writeU32LEHost(shmSegmentVersion, to: &header, at: 8)
    writeU32LEHost(UInt32(shmSegmentHeaderSize), to: &header, at: 12)
    writeU64LEHost(UInt64(totalSize), to: &header, at: 16)
    writeU32LEHost(config.maxPayloadSize, to: &header, at: 24)
    writeU32LEHost(config.inlineThreshold, to: &header, at: 28)
    writeU32LEHost(UInt32(config.maxGuests), to: &header, at: 32)
    writeU32LEHost(config.bipbufCapacity, to: &header, at: 36)
    writeU64LEHost(UInt64(peerTableOffset), to: &header, at: 40)
    writeU64LEHost(UInt64(varPoolOffset), to: &header, at: 48)
    writeU64LEHost(config.heartbeatInterval, to: &header, at: 56)
    writeU32LEHost(0, to: &header, at: 64)
    writeU32LEHost(UInt32(config.sizeClasses.count), to: &header, at: 68)
    writeU64LEHost(UInt64(totalSize), to: &header, at: 72)

    let headerBytes = try region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
    headerBytes.copyBytes(from: header)
}

@inline(__always)
private func writeU32LEHost(_ value: UInt32, to bytes: inout [UInt8], at index: Int) {
    let le = value.littleEndian
    bytes[index] = UInt8(truncatingIfNeeded: le)
    bytes[index + 1] = UInt8(truncatingIfNeeded: le >> 8)
    bytes[index + 2] = UInt8(truncatingIfNeeded: le >> 16)
    bytes[index + 3] = UInt8(truncatingIfNeeded: le >> 24)
}

@inline(__always)
private func writeU64LEHost(_ value: UInt64, to bytes: inout [UInt8], at index: Int) {
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

@inline(__always)
private func shmHostAlignUp(_ value: Int, to alignment: Int) -> Int {
    (value + alignment - 1) & ~(alignment - 1)
}

@inline(__always)
private func closeIfValid(_ fd: inout Int32) {
    if fd >= 0 {
        close(fd)
        fd = -1
    }
}

#else
import Foundation

public struct ShmHostSegmentConfig: Sendable {
    public init(
        maxGuests: UInt8 = 1,
        bipbufCapacity: UInt32 = 64 * 1024,
        maxPayloadSize: UInt32 = 1024 * 1024,
        inlineThreshold: UInt32 = shmDefaultInlineThreshold,
        heartbeatInterval: UInt64 = 0,
        sizeClasses: [ShmVarSlotClass] = [ShmVarSlotClass(slotSize: 4096, count: 8)]
    ) {
        _ = maxGuests
        _ = bipbufCapacity
        _ = maxPayloadSize
        _ = inlineThreshold
        _ = heartbeatInterval
        _ = sizeClasses
    }
}

public enum ShmHostSegmentError: Error, Equatable {
    case unsupportedPlatform
}

public enum ShmHostPeerError: Error, Equatable {
    case unsupportedPlatform
}

public enum ShmHostSendError: Error, Equatable {
    case unsupportedPlatform
}

public enum ShmHostReceiveError: Error, Equatable {
    case unsupportedPlatform
}

public final class ShmHostSegment: @unchecked Sendable {
    public let path: String = ""
    public var header: ShmSegmentHeader {
        get throws { throw ShmHostSegmentError.unsupportedPlatform }
    }
    public static func create(
        path: String,
        config: ShmHostSegmentConfig,
        cleanup: ShmFileCleanup = .manual
    ) throws -> ShmHostSegment {
        _ = path
        _ = config
        _ = cleanup
        throw ShmHostSegmentError.unsupportedPlatform
    }
    public func reservePeer() throws -> ShmPreparedHostPeer {
        throw ShmHostSegmentError.unsupportedPlatform
    }
    public func releaseReservedPeer(peerId: UInt8) {
        _ = peerId
    }
}

public final class ShmPreparedHostPeer: @unchecked Sendable {
    public let peerId: UInt8 = 0
    public func makeGuestTicket(hubPath: String? = nil) throws -> ShmBootstrapTicket {
        _ = hubPath
        throw ShmHostPeerError.unsupportedPlatform
    }
    public func sendBootstrapSuccess(
        controlFd: Int32,
        hubPath: String? = nil
    ) throws {
        _ = controlFd
        _ = hubPath
        throw ShmHostPeerError.unsupportedPlatform
    }
    public func closeGuestEndpoints() {}
    public func intoTransport() throws -> ShmHostTransport {
        throw ShmHostPeerError.unsupportedPlatform
    }
    public func releaseReservation() {}
}

public final class ShmHostRuntime: @unchecked Sendable {
    public let peerId: UInt8 = 0
    public var region: ShmRegion {
        get throws { throw ShmHostSegmentError.unsupportedPlatform }
    }
    public var header: ShmSegmentHeader {
        get throws { throw ShmHostSegmentError.unsupportedPlatform }
    }
    public static func attach(
        segment: ShmHostSegment,
        peerId: UInt8,
        doorbellFd: Int32,
        mmapControlFd: Int32
    ) throws -> ShmHostRuntime {
        _ = segment
        _ = peerId
        _ = doorbellFd
        _ = mmapControlFd
        throw ShmHostSegmentError.unsupportedPlatform
    }
    public func send(frame: ShmGuestFrame) throws {
        _ = frame
        throw ShmHostSendError.unsupportedPlatform
    }
    public func receive() throws -> ShmGuestFrame? {
        throw ShmHostReceiveError.unsupportedPlatform
    }
    public func detach() {}
    public func isHostGoodbye() -> Bool { true }
    public func isPeerGoodbye() -> Bool { true }
    public func signalDoorbell() throws {
        throw ShmHostSegmentError.unsupportedPlatform
    }
    public func waitForDoorbell(timeoutMs: Int32? = nil) throws -> ShmDoorbellWaitResult? {
        _ = timeoutMs
        throw ShmHostSegmentError.unsupportedPlatform
    }
    public func checkRemap() throws -> Bool {
        throw ShmHostSegmentError.unsupportedPlatform
    }
}

public final class ShmHostTransport: Link, @unchecked Sendable {
    public let negotiated = Negotiated(maxPayloadSize: 0, initialCredit: 0, maxConcurrentRequests: 0)
    public init(runtime: ShmHostRuntime) {
        _ = runtime
    }
    public func sendFrame(_ bytes: [UInt8]) async throws {
        _ = bytes
        throw TransportError.transportIO("unsupported platform")
    }
    public func recvFrame() async throws -> [UInt8]? {
        throw TransportError.transportIO("unsupported platform")
    }
    public func sendRawPrologue(_ bytes: [UInt8]) async throws {
        _ = bytes
        throw TransportError.transportIO("unsupported platform")
    }
    public func recvRawPrologue() async throws -> [UInt8]? {
        throw TransportError.transportIO("unsupported platform")
    }
    public func setMaxFrameSize(_ size: Int) async throws {
        _ = size
    }
    public func close() async throws {}
}
#endif
