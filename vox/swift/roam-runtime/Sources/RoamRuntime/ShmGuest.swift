import Foundation
import CRoamShmFfi
#if os(macOS)
import Darwin
#elseif os(Linux)
import Glibc
#endif

// r[impl shm.peer-table.states]
public enum ShmPeerState: UInt32, Sendable {
    case empty = 0
    case attached = 1
    case goodbye = 2
    case reserved = 3
}

// r[impl shm.varslot.slot-meta]
public enum ShmSlotState: UInt32, Sendable {
    case free = 0
    case allocated = 1
    case inFlight = 2
}

// r[impl shm.varslot.classes]
public struct ShmVarSlotClass: Sendable, Equatable {
    public let slotSize: UInt32
    public let count: UInt32

    public init(slotSize: UInt32, count: UInt32) {
        self.slotSize = slotSize
        self.count = count
    }
}

public struct ShmVarSlotHandle: Sendable, Equatable {
    public let classIdx: UInt8
    public let extentIdx: UInt8
    public let slotIdx: UInt32
    public let generation: UInt32

    public init(classIdx: UInt8, extentIdx: UInt8, slotIdx: UInt32, generation: UInt32) {
        self.classIdx = classIdx
        self.extentIdx = extentIdx
        self.slotIdx = slotIdx
        self.generation = generation
    }
}

public enum ShmVarSlotFreeError: Error, Equatable {
    case invalidClass
    case invalidIndex
    case generationMismatch(expected: UInt32, actual: UInt32)
    case invalidState(expected: ShmSlotState, actual: ShmSlotState)
    case ffiError
}

public enum ShmVarSlotPoolError: Error, Equatable {
    case attachFailed
}

// r[impl shm.varslot]
// r[impl shm.varslot.allocate]
// r[impl shm.varslot.free]
// r[impl shm.varslot.selection]
// r[impl shm.varslot.freelist]
public final class ShmVarSlotPool: @unchecked Sendable {
    private let pool: OpaquePointer
    private var region: ShmRegion

    public init(region: ShmRegion, baseOffset: Int, classes: [ShmVarSlotClass]) throws {
        self.region = region

        let ffiClasses = classes.map { RoamSizeClass(slot_size: $0.slotSize, count: $0.count) }
        guard let pool = ffiClasses.withUnsafeBufferPointer({ buf in
            roam_var_slot_pool_attach(
                region.basePointer().assumingMemoryBound(to: UInt8.self),
                UInt(region.length),
                UInt64(baseOffset),
                buf.baseAddress,
                UInt(buf.count)
            )
        }) else {
            throw ShmVarSlotPoolError.attachFailed
        }
        self.pool = pool
    }

    deinit {
        roam_var_slot_pool_destroy(pool)
    }

    public static func calculateSize(classes: [ShmVarSlotClass]) -> Int {
        let ffiClasses = classes.map { RoamSizeClass(slot_size: $0.slotSize, count: $0.count) }
        return ffiClasses.withUnsafeBufferPointer { buf in
            Int(roam_var_slot_pool_calculate_size(buf.baseAddress, UInt(buf.count)))
        }
    }

    public func updateRegion(_ region: ShmRegion) {
        self.region = region
        roam_var_slot_pool_update_region(
            pool,
            region.basePointer().assumingMemoryBound(to: UInt8.self),
            UInt(region.length)
        )
    }

    public func initialize() {
        roam_var_slot_pool_init(pool)
    }

    // r[impl shm.varslot.allocate]
    public func alloc(size: UInt32, owner: UInt8) -> ShmVarSlotHandle? {
        var out = RoamVarSlotHandle(class_idx: 0, extent_idx: 0, slot_idx: 0, generation: 0)
        let result = roam_var_slot_pool_alloc(pool, size, owner, &out)
        if result == 1 {
            return ShmVarSlotHandle(
                classIdx: out.class_idx,
                extentIdx: out.extent_idx,
                slotIdx: out.slot_idx,
                generation: out.generation
            )
        }
        return nil
    }

    public func markInFlight(_ handle: ShmVarSlotHandle) throws {
        let result = roam_var_slot_pool_mark_in_flight(pool, toFfi(handle))
        if result != 0 {
            throw ShmVarSlotFreeError.ffiError
        }
    }

    // r[impl shm.varslot.free]
    public func free(_ handle: ShmVarSlotHandle) throws {
        let result = roam_var_slot_pool_free(pool, toFfi(handle))
        if result != 0 {
            throw ShmVarSlotFreeError.ffiError
        }
    }

    public func freeAllocated(_ handle: ShmVarSlotHandle) throws {
        let result = roam_var_slot_pool_free_allocated(pool, toFfi(handle))
        if result != 0 {
            throw ShmVarSlotFreeError.ffiError
        }
    }

    public func slotSize(classIdx: UInt8) -> UInt32? {
        let size = roam_var_slot_pool_slot_size(pool, classIdx)
        return size > 0 ? size : nil
    }

    public func payloadPointer(_ handle: ShmVarSlotHandle) -> UnsafeMutableRawPointer? {
        guard let ptr = roam_var_slot_pool_payload_ptr(pool, toFfi(handle)) else {
            return nil
        }
        return UnsafeMutableRawPointer(ptr)
    }

    public func slotState(_ handle: ShmVarSlotHandle) -> ShmSlotState {
        let raw = roam_var_slot_pool_slot_state(pool, toFfi(handle))
        return ShmSlotState(rawValue: UInt32(max(raw, 0))) ?? .free
    }

    public func recoverPeer(ownerPeer: UInt8) {
        roam_var_slot_pool_recover_peer(pool, ownerPeer)
    }

    @inline(__always)
    private func toFfi(_ handle: ShmVarSlotHandle) -> RoamVarSlotHandle {
        RoamVarSlotHandle(
            class_idx: handle.classIdx,
            extent_idx: handle.extentIdx,
            slot_idx: handle.slotIdx,
            generation: handle.generation
        )
    }
}

#if os(macOS) || os(Linux)
public enum ShmDoorbellWaitResult: Sendable, Equatable {
    case signaled
    case timeout
    case peerDead
}

public enum ShmDoorbellError: Error, Equatable {
    case waitFailed(errno: Int32)
    case signalFailed(errno: Int32)
    case unsupportedPlatform
}

// r[impl shm.signal.doorbell]
// r[impl shm.signal.doorbell.signal]
// r[impl shm.signal.doorbell.wait]
public final class ShmDoorbell: @unchecked Sendable {
    public let fd: Int32
    private let ownsFd: Bool

    public init(fd: Int32, ownsFd: Bool = false) {
        self.fd = fd
        self.ownsFd = ownsFd
    }

    deinit {
        if ownsFd {
            close(fd)
        }
    }

    public func signal() throws {
        var byte: UInt8 = 1
        let flags = Int32(MSG_DONTWAIT | MSG_NOSIGNAL)
        while true {
            let written = withUnsafePointer(to: &byte) { ptr in
                send(fd, ptr, 1, flags)
            }
            if written == 1 {
                return
            }
            if written < 0 && errno == EINTR {
                continue
            }
            // Doorbell is level-trigger-ish for us: if the buffer is full, peer already has
            // pending wakeups to drain, so additional signals can be coalesced.
            if written < 0 && (errno == EAGAIN || errno == EWOULDBLOCK) {
                return
            }
            if written < 0 && (errno == EPIPE || errno == ECONNRESET || errno == ENOTCONN) {
                return
            }
            throw ShmDoorbellError.signalFailed(errno: errno)
        }
    }

    private func drainForWait() throws -> ShmDoorbellWaitResult? {
        var buf = [UInt8](repeating: 0, count: 64)
        var drained = false
        while true {
            let n = buf.withUnsafeMutableBytes { raw in
                recv(fd, raw.baseAddress, raw.count, Int32(MSG_DONTWAIT))
            }
            if n > 0 {
                drained = true
                continue
            }
            if n == 0 {
                return drained ? .signaled : .peerDead
            }
            if errno == EINTR {
                continue
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                return drained ? .signaled : nil
            }
            if errno == ECONNRESET || errno == ENOTCONN || errno == EPIPE {
                return drained ? .signaled : .peerDead
            }
            throw ShmDoorbellError.waitFailed(errno: errno)
        }
    }

    @discardableResult
    public func drain() -> Bool {
        do {
            return try drainForWait() == .signaled
        } catch {
            return false
        }
    }

    public func wait(timeoutMs: Int32? = nil) throws -> ShmDoorbellWaitResult {
        if let immediate = try drainForWait() {
            return immediate
        }

        var pfd = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        let timeout = timeoutMs ?? -1

        while true {
            let n = withUnsafeMutablePointer(to: &pfd) { ptr in
                poll(ptr, 1, timeout)
            }
            if n > 0 {
                if let result = try drainForWait() {
                    return result
                }
                if (pfd.revents & Int16(POLLHUP | POLLERR | POLLNVAL)) != 0 {
                    // r[impl shm.signal.doorbell.death]
                    return .peerDead
                }
                continue
            }
            if n == 0 {
                return .timeout
            }
            if errno == EINTR {
                continue
            }
            throw ShmDoorbellError.waitFailed(errno: errno)
        }
    }
}
#else
public enum ShmDoorbellWaitResult: Sendable, Equatable {
    case signaled
    case timeout
    case peerDead
}

public enum ShmDoorbellError: Error, Equatable {
    case unsupportedPlatform
}

public final class ShmDoorbell: @unchecked Sendable {
    public let fd: Int32

    public init(fd: Int32, ownsFd: Bool = false) {
        self.fd = fd
        _ = ownsFd
    }

    public func signal() throws {
        throw ShmDoorbellError.unsupportedPlatform
    }

    public func drain() {}

    public func wait(timeoutMs: Int32? = nil) throws -> ShmDoorbellWaitResult {
        _ = timeoutMs
        throw ShmDoorbellError.unsupportedPlatform
    }
}
#endif

public struct ShmGuestFrame: Sendable, Equatable {
    public let payload: [UInt8]

    public init(payload: [UInt8]) {
        self.payload = payload
    }
}

public enum ShmGuestAttachError: Error, Equatable {
    case invalidHeader(ShmLayoutError)
    case noPeerSlots
    case hostGoodbye
    case invalidTicketPeer(UInt8)
    case slotNotReserved
    case missingVarSlotClasses
    case unsupportedVersion(UInt32)
}

public enum ShmGuestSendError: Error, Equatable {
    case hostGoodbye
    case payloadTooLarge
    case ringFull
    case slotExhausted
    case slotError
    case mmapAllocationFailed
    case mmapUnavailable
    case mmapControlError(errno: Int32)
    case doorbellPeerDead
}

public enum ShmGuestReceiveError: Error, Equatable {
    case malformedFrame
    case slotError
    case payloadTooLarge
}

private struct OutboundMmapRegion {
    let region: ShmRegion
    let mapId: UInt32
    let mapGeneration: UInt32
    let mappingLength: Int
    var nextOffset: Int
}

// r[impl shm.mmap.attach]
// r[impl shm.mmap.bounds]
// r[impl shm.mmap.aba]
public final class ShmMmapAttachments: @unchecked Sendable {
    private let raw: OpaquePointer

    public init?(controlFd: Int32) {
        guard controlFd >= 0, let raw = roam_mmap_attachments_create(controlFd) else {
            return nil
        }
        self.raw = raw
    }

    deinit {
        roam_mmap_attachments_destroy(raw)
    }

    public func drainControl() -> Bool {
        roam_mmap_attachments_drain_control(raw) >= 0
    }

    public func resolve(mmapRef: ShmMmapRef) -> [UInt8]? {
        var ptr: UnsafePointer<UInt8>?
        let rc = roam_mmap_attachments_resolve_ptr(
            raw,
            mmapRef.mapId,
            mmapRef.mapGeneration,
            mmapRef.mapOffset,
            mmapRef.payloadLen,
            &ptr
        )
        guard rc == 0, let ptr else {
            return nil
        }
        return Array(UnsafeBufferPointer(start: ptr, count: Int(mmapRef.payloadLen)))
    }
}

// r[impl shm.guest.attach]
// r[impl shm.guest.detach]
// r[impl shm.host.goodbye]
// r[impl shm.architecture]
// r[impl shm.signal]
// r[impl shm.topology]
// r[impl shm.topology.peer-id]
// r[impl shm.topology.max-guests]
// r[impl shm.topology.communication]
// r[impl shm.topology.bidirectional]
// r[impl shm.framing.threshold]
// r[impl zerocopy.send.shm]
// r[impl zerocopy.recv.shm.inline]
// r[impl zerocopy.recv.shm.slotref]
// r[impl zerocopy.recv.shm.mmap]
public final class ShmGuestRuntime: @unchecked Sendable {
    public let peerId: UInt8
    public private(set) var region: ShmRegion
    public private(set) var header: ShmSegmentHeader

    private var guestToHost: ShmBipBuffer
    private var hostToGuest: ShmBipBuffer
    private var slotPool: ShmVarSlotPool
    private let doorbell: ShmDoorbell?
    private let mmapAttachments: ShmMmapAttachments?
    private var mmapControlFd: Int32
    private let maxVarSlotPayload: UInt32
    private var nextMmapId: UInt32 = 1
    private var outboundMmapRegion: OutboundMmapRegion?
    private var retiredOutboundMmapRegions: [ShmRegion] = []
    private var fatalError = false

    deinit {
        closeMmapControlFd()
    }

    // r[impl shm.guest.attach]
    // r[impl shm.guest.attach-failure]
    /// Attach to an SHM segment, discovering var slot classes from the segment itself.
    public static func attach(ticket: ShmBootstrapTicket) throws -> ShmGuestRuntime {
        let region: ShmRegion
        if ticket.shmFd >= 0 {
            region = try ShmRegion.attach(fd: ticket.shmFd, pathHint: ticket.hubPath)
        } else {
            region = try ShmRegion.attach(path: ticket.hubPath)
        }
        let classes = try discoverClasses(from: region)
        return try attach(region: region, ticket: ticket, classes: classes)
    }

    // r[impl shm.guest.attach]
    // r[impl shm.guest.attach-failure]
    /// Attach to an SHM segment by path, discovering var slot classes from segment metadata.
    public static func attach(path: String) throws -> ShmGuestRuntime {
        let region = try ShmRegion.attach(path: path)
        let classes = try discoverClasses(from: region)
        return try attach(region: region, ticket: nil, classes: classes)
    }

    private static func discoverClasses(from region: ShmRegion) throws -> [ShmVarSlotClass] {
        let view = try ShmSegmentView(region: region)
        let header = view.header
        let numClasses = Int(header.numVarSlotClasses)
        guard numClasses > 0 else {
            throw ShmGuestAttachError.missingVarSlotClasses
        }
        var classes: [ShmVarSlotClass] = []
        for i in 0..<numClasses {
            // SizeClassHeader layout: slot_size: u32 at offset 0, slot_count: u32 at offset 4, rest padding
            let offset = Int(header.varSlotPoolOffset) + i * 64
            let rawBuf = try region.mutableBytes(at: offset, count: 8)
            let bytes = Array(rawBuf)
            let slotSize = readShmU32LE(bytes, 0)
            let slotCount = readShmU32LE(bytes, 4)
            classes.append(ShmVarSlotClass(slotSize: slotSize, count: slotCount))
        }
        return classes
    }

    // r[impl shm.guest.attach]
    // r[impl shm.guest.attach-failure]
    private static func attach(
        region: ShmRegion,
        ticket: ShmBootstrapTicket?,
        classes: [ShmVarSlotClass]
    ) throws -> ShmGuestRuntime {
        let view = try ShmSegmentView(region: region)
        let header = view.header

        if header.hostGoodbye != 0 {
            throw ShmGuestAttachError.hostGoodbye
        }
        if classes.isEmpty {
            throw ShmGuestAttachError.missingVarSlotClasses
        }

        let pool = try ShmVarSlotPool(region: region, baseOffset: Int(header.varSlotPoolOffset), classes: classes)

        let chosenPeerId: UInt8
        if let ticket {
            guard ticket.peerId >= 1, UInt32(ticket.peerId) <= header.maxGuests else {
                throw ShmGuestAttachError.invalidTicketPeer(ticket.peerId)
            }
            let statePtr = try shmPeerStatePointer(region: region, header: header, peerId: ticket.peerId)
            if !transitionShmPeerState(statePtr: statePtr, from: .reserved, to: .attached) {
                throw ShmGuestAttachError.slotNotReserved
            }
            chosenPeerId = ticket.peerId
        } else {
            guard let peer = try claimEmptyShmPeer(region: region, header: header, to: .attached) else {
                throw ShmGuestAttachError.noPeerSlots
            }
            chosenPeerId = peer
        }

        let buffers = try attachShmPeerBuffers(region: region, view: view, peerId: chosenPeerId)
        let mmapControlFd = ticket?.mmapControlFd ?? -1
        let endpoints = makeShmControlEndpoints(
            doorbellFd: ticket?.doorbellFd,
            mmapControlFd: mmapControlFd
        )

        return ShmGuestRuntime(
            peerId: chosenPeerId,
            region: region,
            header: header,
            guestToHost: buffers.guestToHost,
            hostToGuest: buffers.hostToGuest,
            slotPool: pool,
            doorbell: endpoints.doorbell,
            mmapAttachments: endpoints.mmapAttachments,
            mmapControlFd: mmapControlFd,
            maxVarSlotPayload: shmMaxVarSlotPayload(classes: classes)
        )
    }

    private init(
        peerId: UInt8,
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

    // r[impl zerocopy.send.shm]
    // r[impl shm.signal.doorbell.integration]
    // r[impl shm.signal.doorbell.optional]
    // r[impl shm.framing.inline]
    // r[impl shm.framing.slot-ref]
    // r[impl shm.framing.threshold]
    // r[impl shm.varslot.extents.notification]
    public func send(frame: ShmGuestFrame) throws {
        _ = try checkRemap()

        if fatalError || hostGoodbyeFlag() {
            throw ShmGuestSendError.hostGoodbye
        }
        try sendShmFrame(
            role: "guest",
            frame: frame,
            header: header,
            outbox: guestToHost,
            slotPool: slotPool,
            slotOwner: peerId,
            doorbell: doorbell,
            maxVarSlotPayload: maxVarSlotPayload,
            mmapControlFd: mmapControlFd,
            errors: ShmSendErrors<ShmGuestSendError>(
                payloadTooLarge: .payloadTooLarge,
                ringFull: .ringFull,
                slotExhausted: .slotExhausted,
                slotError: .slotError,
                mmapUnavailable: .mmapUnavailable
            )
        ) { payload, payloadLen in
            let allocation = try self.allocateOutboundMmapSlice(payloadCount: Int(payloadLen))
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

    private func allocateMmapId() -> UInt32 {
        let mapId = nextMmapId
        nextMmapId &+= 1
        if nextMmapId == 0 {
            nextMmapId = 1
        }
        return mapId
    }

    private func allocateOutboundMmapSlice(payloadCount: Int) throws
        -> (region: ShmRegion, mapId: UInt32, mapGeneration: UInt32, mapOffset: Int)
    {
        if var active = outboundMmapRegion, active.nextOffset + payloadCount <= active.mappingLength {
            let offset = active.nextOffset
            active.nextOffset += payloadCount
            outboundMmapRegion = active
            return (active.region, active.mapId, active.mapGeneration, offset)
        }

        let pageSize = max(Int(getpagesize()), 4096)
        let minRegionSize = 4 * 1024 * 1024
        let regionSize = alignUp(max(payloadCount, minRegionSize), pageSize)
        let mmapPath = "\(NSTemporaryDirectory())roam-swift-mmap-\(UUID().uuidString).bin"
        let region: ShmRegion
        do {
            region = try ShmRegion.create(path: mmapPath, size: regionSize, cleanup: .auto)
        } catch {
            throw ShmGuestSendError.mmapAllocationFailed
        }

        let mapId = allocateMmapId()
        let mapGeneration: UInt32 = 1
        let sendRc = roam_mmap_control_send(
            mmapControlFd,
            region.rawFd,
            mapId,
            mapGeneration,
            UInt64(regionSize)
        )
        guard sendRc == 0 else {
            throw ShmGuestSendError.mmapControlError(errno: errno)
        }

        if let previous = outboundMmapRegion {
            retiredOutboundMmapRegions.append(previous.region)
        }
        outboundMmapRegion = OutboundMmapRegion(
            region: region,
            mapId: mapId,
            mapGeneration: mapGeneration,
            mappingLength: regionSize,
            nextOffset: payloadCount
        )
        return (region, mapId, mapGeneration, 0)
    }

    // r[impl zerocopy.recv.shm.inline]
    // r[impl zerocopy.recv.shm.slotref]
    // r[impl shm.signal.doorbell.integration]
    // r[impl shm.signal.doorbell.optional]
    // r[impl shm.framing.inline]
    // r[impl shm.framing.slot-ref]
    // r[impl shm.varslot.extents]
    // r[impl shm.varslot.extents.notification]
    public func receive() throws -> ShmGuestFrame? {
        _ = try checkRemap()

        if fatalError || hostGoodbyeFlag() {
            return nil
        }

        guard let readable = hostToGuest.tryRead() else {
            return nil
        }

        do {
            return try receiveShmFrame(
                bytes: Array(readable),
                maxPayloadSize: self.header.maxPayloadSize,
                inbox: hostToGuest,
                slotPool: slotPool,
                doorbell: doorbell,
                mmapAttachments: mmapAttachments,
                errors: ShmReceiveErrors<ShmGuestReceiveError>(
                    malformedFrame: .malformedFrame,
                    slotError: .slotError,
                    payloadTooLarge: .payloadTooLarge
                )
            )
        } catch let error as ShmGuestReceiveError {
            fatalError = true
            throw error
        }
    }

    public func checkRemap() throws -> Bool {
        try checkShmRemap(region: region, header: &header) { view in
            let peerEntry = try view.peerEntry(peerId: peerId)
            let ringOffset = Int(peerEntry.ringOffset)
            guestToHost = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset)
            hostToGuest = try ShmBipBuffer.attach(
                region: region,
                headerOffset: ringOffset + shmBipbufHeaderSize + Int(guestToHost.capacity)
            )
            slotPool.updateRegion(region)
        }
    }

    // r[impl shm.guest.detach]
    public func detach() {
        if let statePtr = try? peerStatePointer() {
            atomicStoreU32Release(statePtr, ShmPeerState.goodbye.rawValue)
        }
        try? doorbell?.signal()
        while (try? receive()) != nil {}
    }

    public func isHostGoodbye() -> Bool {
        fatalError || hostGoodbyeFlag()
    }

    public func signalDoorbell() throws {
        try doorbell?.signal()
    }

    public func waitForDoorbell(timeoutMs: Int32? = nil) throws -> ShmDoorbellWaitResult? {
        try waitForShmDoorbell(doorbell: doorbell, timeoutMs: timeoutMs) {
            _ = try checkRemap()
        }
    }

    public func peerState() throws -> ShmPeerState {
        let ptr = try peerStatePointer()
        let raw = atomicLoadU32Acquire(UnsafeRawPointer(ptr))
        return ShmPeerState(rawValue: raw) ?? .empty
    }

    private func closeMmapControlFd() {
        closeShmMmapControlFd(&mmapControlFd)
    }

    private func hostGoodbyeFlag() -> Bool {
        shmHostGoodbyeFlag(region: region, header: header)
    }

    private func peerStatePointer() throws -> UnsafeMutableRawPointer {
        try shmPeerStatePointer(region: region, header: header, peerId: peerId)
    }

}

@inline(__always)
private func alignUp(_ value: Int, _ alignment: Int) -> Int {
    ((value + alignment - 1) / alignment) * alignment
}
