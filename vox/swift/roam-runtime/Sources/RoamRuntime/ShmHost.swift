#if os(macOS)
import Darwin
import Foundation
import CRoamShmFfi

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
        let maxVarSlotPayload = (config.sizeClasses.map(\.slotSize).max() ?? 0) >= 4
            ? (config.sizeClasses.map(\.slotSize).max() ?? 0) - 4
            : 0

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
            var expected = ShmPeerState.empty.rawValue
            if atomicCompareExchangeU32(statePtr, expected: &expected, desired: ShmPeerState.reserved.rawValue) {
                let epochPtr = statePtr.advanced(by: 4)
                _ = atomicFetchAddU32(epochPtr, 1)

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
            var expected = ShmPeerState.reserved.rawValue
            _ = atomicCompareExchangeU32(statePtr, expected: &expected, desired: ShmPeerState.empty.rawValue)
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
        let offset = Int(header.peerTableOffset) + Int(peerId - 1) * shmPeerEntrySize
        return try region.pointer(at: offset)
    }

    fileprivate func bipBuffers(peerId: UInt8) throws -> (guestToHost: ShmBipBuffer, hostToGuest: ShmBipBuffer) {
        guard peerId >= 1, UInt32(peerId) <= header.maxGuests else {
            throw ShmHostSegmentError.invalidPeerId(peerId)
        }
        let ringOffset = ringBaseOffset + Int(peerId - 1) * ringStride
        let guestToHost = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset)
        let hostToGuest = try ShmBipBuffer.attach(
            region: region,
            headerOffset: ringOffset + shmBipbufHeaderSize + Int(guestToHost.capacity)
        )
        return (guestToHost: guestToHost, hostToGuest: hostToGuest)
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
            roam_shm_bootstrap_response_send_unix(
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
    private var nextMmapId: UInt32 = 1
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
        let doorbell = ShmDoorbell(fd: doorbellFd, ownsFd: true)
        let attachments: ShmMmapAttachments?
        if mmapControlFd >= 0 {
            attachments = ShmMmapAttachments(controlFd: mmapControlFd)
        } else {
            attachments = nil
        }

        return ShmHostRuntime(
            peerId: peerId,
            segment: segment,
            region: segment.region,
            header: header,
            guestToHost: buffers.guestToHost,
            hostToGuest: buffers.hostToGuest,
            slotPool: pool,
            doorbell: doorbell,
            mmapAttachments: attachments,
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

        let payloadLen = UInt32(frame.payload.count)
        if payloadLen > header.maxPayloadSize {
            throw ShmHostSendError.payloadTooLarge
        }

        let threshold = header.inlineThreshold == 0 ? shmDefaultInlineThreshold : header.inlineThreshold
        if shmShouldInline(payloadLen: payloadLen, threshold: threshold) {
            let bytes = encodeShmInlineFrame(payload: frame.payload)
            if let grant = try hostToGuest.tryGrant(UInt32(bytes.count)) {
                grant.copyBytes(from: bytes)
                try hostToGuest.commit(UInt32(bytes.count))
                try doorbell?.signal()
                return
            }
            throw ShmHostSendError.ringFull
        }

        let slotPayloadLen = payloadLen &+ 4
        guard slotPayloadLen >= payloadLen else {
            throw ShmHostSendError.payloadTooLarge
        }
        if payloadLen <= maxVarSlotPayload {
            try sendViaSlot(frame, payloadLen: payloadLen, slotPayloadLen: slotPayloadLen)
            return
        }

        try sendViaMmap(frame, payloadLen: payloadLen)
    }

    public func receive() throws -> ShmGuestFrame? {
        _ = try checkRemap()

        if fatalError || hostGoodbyeFlag() || peerGoodbyeFlag() {
            return nil
        }

        guard let readable = guestToHost.tryRead() else {
            return nil
        }

        let bytes = Array(readable)
        let decoded: ShmDecodedFrame
        do {
            decoded = try decodeShmFrame(bytes)
        } catch {
            fatalError = true
            throw ShmHostReceiveError.malformedFrame
        }

        switch decoded {
        case .inline(let header, let payload):
            try guestToHost.release(header.totalLen)
            return ShmGuestFrame(payload: payload)

        case .slotRef(let header, let slotRef):
            let handle = ShmVarSlotHandle(
                classIdx: slotRef.classIdx,
                extentIdx: slotRef.extentIdx,
                slotIdx: slotRef.slotIdx,
                generation: slotRef.slotGeneration
            )

            guard let clsSize = slotPool.slotSize(classIdx: slotRef.classIdx), clsSize >= 4 else {
                fatalError = true
                throw ShmHostReceiveError.slotError
            }
            guard let payloadPtr = slotPool.payloadPointer(handle) else {
                fatalError = true
                throw ShmHostReceiveError.slotError
            }

            let slotBytes = UnsafeRawBufferPointer(start: UnsafeRawPointer(payloadPtr), count: Int(clsSize))
            let payloadLen = readU32LEHost(Array(slotBytes.prefix(4)), 0)
            if payloadLen > clsSize - 4 {
                fatalError = true
                throw ShmHostReceiveError.payloadTooLarge
            }
            let payload = Array(
                UnsafeRawBufferPointer(
                    start: UnsafeRawPointer(payloadPtr.advanced(by: 4)),
                    count: Int(payloadLen)
                )
            )

            do {
                try slotPool.free(handle)
            } catch {
                fatalError = true
                throw ShmHostReceiveError.slotError
            }

            try guestToHost.release(header.totalLen)
            try doorbell?.signal()
            return ShmGuestFrame(payload: payload)

        case .mmapRef(let header, let mmapRef):
            guard mmapRef.payloadLen <= self.header.maxPayloadSize else {
                fatalError = true
                throw ShmHostReceiveError.payloadTooLarge
            }
            guard let mmapAttachments, mmapAttachments.drainControl(),
                  let payload = mmapAttachments.resolve(mmapRef: mmapRef)
            else {
                fatalError = true
                throw ShmHostReceiveError.malformedFrame
            }
            try guestToHost.release(header.totalLen)
            try doorbell?.signal()
            return ShmGuestFrame(payload: payload)
        }
    }

    public func detach() {
        if let statePtr = try? segment.peerStatePointer(peerId: peerId) {
            atomicStoreU32Release(statePtr, ShmPeerState.goodbye.rawValue)
        }
        try? doorbell?.signal()
        while (try? receive()) != nil {}
        closeMmapControlFd()
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
        guard let doorbell else {
            return nil
        }
        let result = try doorbell.wait(timeoutMs: timeoutMs)
        if result == .signaled {
            _ = try checkRemap()
        }
        return result
    }

    public func checkRemap() throws -> Bool {
        let currentSizePtr = try region.pointer(at: 72)
        let currentSize = Int(atomicLoadU64Acquire(UnsafeRawPointer(currentSizePtr)))
        if currentSize <= region.length {
            return false
        }

        try region.resize(newSize: currentSize)
        let view = try ShmSegmentView(region: region)
        header = view.header

        let buffers = try segment.bipBuffers(peerId: peerId)
        guestToHost = buffers.guestToHost
        hostToGuest = buffers.hostToGuest
        slotPool.updateRegion(region)
        return true
    }

    private func sendViaSlot(_ frame: ShmGuestFrame, payloadLen: UInt32, slotPayloadLen: UInt32) throws {
        guard let handle = slotPool.alloc(size: slotPayloadLen, owner: 0) else {
            throw ShmHostSendError.slotExhausted
        }

        guard let payloadPtr = slotPool.payloadPointer(handle) else {
            try? slotPool.freeAllocated(handle)
            throw ShmHostSendError.slotError
        }

        frame.payload.withUnsafeBytes { raw in
            if let base = raw.baseAddress {
                payloadPtr.storeBytes(of: payloadLen.littleEndian, as: UInt32.self)
                memcpy(payloadPtr.advanced(by: 4), base, raw.count)
            }
        }

        do {
            try slotPool.markInFlight(handle)
        } catch {
            try? slotPool.freeAllocated(handle)
            throw ShmHostSendError.slotError
        }

        let slotFrame = encodeShmSlotRefFrame(
            slotRef: ShmSlotRef(
                classIdx: handle.classIdx,
                extentIdx: handle.extentIdx,
                slotIdx: handle.slotIdx,
                slotGeneration: handle.generation
            )
        )

        if let grant = try hostToGuest.tryGrant(UInt32(slotFrame.count)) {
            grant.copyBytes(from: slotFrame)
            try hostToGuest.commit(UInt32(slotFrame.count))
            try doorbell?.signal()
            return
        }

        try? slotPool.free(handle)
        throw ShmHostSendError.ringFull
    }

    private func sendViaMmap(_ frame: ShmGuestFrame, payloadLen: UInt32) throws {
        guard mmapControlFd >= 0 else {
            throw ShmHostSendError.mmapUnavailable
        }

        let frameSize = UInt32(shmFrameHeaderSize + shmMmapRefSize)
        guard let grant = try hostToGuest.tryGrant(frameSize) else {
            throw ShmHostSendError.ringFull
        }

        let payloadCount = Int(payloadLen)
        let pageSize = max(Int(getpagesize()), 4096)
        let mappingLength = max(((payloadCount + pageSize - 1) / pageSize) * pageSize, payloadCount)
        let mmapPath = "\(NSTemporaryDirectory())roam-swift-host-mmap-\(UUID().uuidString).bin"
        let mapping: ShmRegion
        do {
            mapping = try ShmRegion.create(path: mmapPath, size: mappingLength, cleanup: .auto)
        } catch {
            throw ShmHostSendError.mmapAllocationFailed
        }

        frame.payload.withUnsafeBytes { raw in
            if let base = raw.baseAddress {
                memcpy(mapping.basePointer(), base, raw.count)
            }
        }

        let mapId = allocateMmapId()
        let mapGeneration: UInt32 = 1
        let sendRc = roam_mmap_control_send(
            mmapControlFd,
            mapping.rawFd,
            mapId,
            mapGeneration,
            UInt64(mappingLength)
        )
        guard sendRc == 0 else {
            throw ShmHostSendError.mmapControlError(errno: errno)
        }

        let mmapFrame = encodeShmMmapRefFrame(
            mmapRef: ShmMmapRef(
                mapId: mapId,
                mapGeneration: mapGeneration,
                mapOffset: 0,
                payloadLen: payloadLen
            )
        )
        grant.copyBytes(from: mmapFrame)
        try hostToGuest.commit(UInt32(mmapFrame.count))
        try doorbell?.signal()
    }

    private func allocateMmapId() -> UInt32 {
        let mapId = nextMmapId
        nextMmapId &+= 1
        if nextMmapId == 0 {
            nextMmapId = 1
        }
        return mapId
    }

    private func hostGoodbyeFlag() -> Bool {
        guard let ptr = try? region.pointer(at: 64) else {
            return true
        }
        return atomicLoadU32Acquire(UnsafeRawPointer(ptr)) != 0
    }

    private func peerGoodbyeFlag() -> Bool {
        guard let state = try? segment.peerState(peerId: peerId) else {
            return true
        }
        return state == ShmPeerState.goodbye.rawValue
    }

    private func closeMmapControlFd() {
        if mmapControlFd >= 0 {
            close(mmapControlFd)
            mmapControlFd = -1
        }
    }
}

// r[impl transport.shm]
public final class ShmHostTransport: MessageTransport, @unchecked Sendable {
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

    public func send(_ message: MessageV7) async throws {
        let frame = try messageToShmFrame(message)
        do {
            try withHostLock(lock) {
                if closed {
                    throw TransportError.connectionClosed
                }
                if frame.payload.count > Int(negotiated.maxPayloadSize) {
                    throw TransportError.protocolViolation("payload exceeds negotiated maxPayloadSize")
                }
                if frame.payload.count + 64 > maxFrameSize {
                    throw TransportError.frameEncoding("frame exceeds max frame size")
                }

                _ = try runtime.checkRemap()
                try runtime.send(frame: frame)
            }
        } catch let err as TransportError {
            throw err
        } catch let err as ShmHostSendError {
            if ProcessInfo.processInfo.environment["ROAM_SHM_DEBUG"] == "1" {
                fputs("[shm-host-transport] send error: \(err)\n", stderr)
            }
            switch err {
            case .ringFull, .slotExhausted:
                throw TransportError.wouldBlock
            case .peerClosed:
                throw TransportError.connectionClosed
            case .payloadTooLarge, .slotError, .mmapAllocationFailed, .mmapUnavailable, .mmapControlError:
                throw TransportError.transportIO("shm host send failed: \(err)")
            }
        } catch {
            throw TransportError.transportIO("shm host send failed: \(error)")
        }
    }

    public func recv() async throws -> MessageV7? {
        while true {
            var frameToDecode: ShmGuestFrame?
            var sawGoodbye = false
            var isClosed = false

            do {
                try withHostLock(lock) {
                    isClosed = closed
                    if isClosed {
                        return
                    }
                    _ = try runtime.checkRemap()
                    frameToDecode = try runtime.receive()
                    sawGoodbye = runtime.isHostGoodbye() || runtime.isPeerGoodbye()
                }
            } catch let err as TransportError {
                throw err
            } catch {
                if ProcessInfo.processInfo.environment["ROAM_SHM_DEBUG"] == "1" {
                    fputs("[shm-host-transport] recv error: \(error)\n", stderr)
                }
                throw TransportError.transportIO("shm host receive failed: \(error)")
            }

            if isClosed {
                return nil
            }

            if let frame = frameToDecode {
                do {
                    try withHostLock(lock) {
                        try runtime.signalDoorbell()
                    }
                } catch {
                    throw TransportError.transportIO("doorbell signal failed: \(error)")
                }
                return try shmFrameToMessage(frame)
            }

            if sawGoodbye {
                return nil
            }

            do {
                let wait = try runtime.waitForDoorbell(timeoutMs: 100)

                if let wait {
                    if wait == .peerDead {
                        if runtime.isHostGoodbye() || runtime.isPeerGoodbye() {
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

    public func setMaxFrameSize(_ size: Int) async throws {
        withHostLock(lock) {
            maxFrameSize = size
        }
    }

    public func close() async throws {
        withHostLock(lock) {
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
private func readU32LEHost(_ bytes: [UInt8], _ at: Int) -> UInt32 {
    UInt32(bytes[at])
        | (UInt32(bytes[at + 1]) << 8)
        | (UInt32(bytes[at + 2]) << 16)
        | (UInt32(bytes[at + 3]) << 24)
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

@inline(__always)
private func withHostLock<T>(_ lock: NSLock, _ body: () throws -> T) rethrows -> T {
    lock.lock()
    defer { lock.unlock() }
    return try body()
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

public final class ShmHostTransport: MessageTransport, @unchecked Sendable {
    public let negotiated = Negotiated(maxPayloadSize: 0, initialCredit: 0, maxConcurrentRequests: 0)
    public init(runtime: ShmHostRuntime) {
        _ = runtime
    }
    public func send(_ message: MessageV7) async throws {
        _ = message
        throw TransportError.transportIO("unsupported platform")
    }
    public func recv() async throws -> MessageV7? {
        throw TransportError.transportIO("unsupported platform")
    }
    public func setMaxFrameSize(_ size: Int) async throws {
        _ = size
    }
    public func close() async throws {}
}
#endif
