import Foundation
import CRoamShmFfi
#if os(macOS)
import Darwin
#endif

public enum ShmPeerState: UInt32, Sendable {
    case empty = 0
    case attached = 1
    case goodbye = 2
    case reserved = 3
}

public enum ShmSlotState: UInt32, Sendable {
    case free = 0
    case allocated = 1
    case inFlight = 2
}

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

public final class ShmVarSlotPool: @unchecked Sendable {
    private static let sizeClassHeaderSize = 64

    private let pool: OpaquePointer
    private var region: ShmRegion

    public init(region: ShmRegion, baseOffset: Int, classes: [ShmVarSlotClass]) {
        self.region = region

        let ffiClasses = classes.map { RoamSizeClass(slot_size: $0.slotSize, count: $0.count) }
        self.pool = ffiClasses.withUnsafeBufferPointer { buf in
            roam_var_slot_pool_attach(
                region.basePointer().assumingMemoryBound(to: UInt8.self),
                UInt(region.length),
                UInt64(baseOffset),
                buf.baseAddress,
                UInt(buf.count)
            )!
        }
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

    public static func loadClasses(region: ShmRegion, header: ShmSegmentHeader) throws -> [ShmVarSlotClass] {
        let base = Int(header.varSlotPoolOffset)
        let numClasses = Int(header.numVarSlotClasses)
        guard numClasses > 0 else {
            return []
        }

        var classes: [ShmVarSlotClass] = []
        classes.reserveCapacity(numClasses)

        for idx in 0..<numClasses {
            let classOffset = base + idx * sizeClassHeaderSize
            let bytes = Array(try region.mutableBytes(at: classOffset, count: sizeClassHeaderSize))
            let slotSize = readU32LE(bytes, 0)
            let count = readU32LE(bytes, 4)
            classes.append(ShmVarSlotClass(slotSize: slotSize, count: count))
        }

        return classes
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

#if os(macOS)
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
        while true {
            let written = withUnsafePointer(to: &byte) { ptr in
                send(fd, ptr, 1, MSG_DONTWAIT)
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

    public func drain() {
        var buf = [UInt8](repeating: 0, count: 64)
        while true {
            let n = buf.withUnsafeMutableBytes { raw in
                recv(fd, raw.baseAddress, raw.count, MSG_DONTWAIT)
            }
            if n > 0 {
                continue
            }
            if n == 0 {
                return
            }
            if errno == EINTR {
                continue
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                return
            }
            return
        }
    }

    public func wait(timeoutMs: Int32? = nil) throws -> ShmDoorbellWaitResult {
        var pfd = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        let timeout = timeoutMs ?? -1

        while true {
            let n = withUnsafeMutablePointer(to: &pfd) { ptr in
                poll(ptr, 1, timeout)
            }
            if n > 0 {
                if (pfd.revents & Int16(POLLHUP | POLLERR | POLLNVAL)) != 0 {
                    return .peerDead
                }
                if (pfd.revents & Int16(POLLIN)) != 0 {
                    drain()
                    return .signaled
                }
                return .timeout
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
    public let msgType: UInt8
    public let id: UInt32
    public let methodId: UInt64
    public let payload: [UInt8]

    public init(msgType: UInt8, id: UInt32, methodId: UInt64, payload: [UInt8]) {
        self.msgType = msgType
        self.id = id
        self.methodId = methodId
        self.payload = payload
    }
}

public enum ShmGuestAttachError: Error, Equatable {
    case invalidHeader(ShmLayoutError)
    case noPeerSlots
    case hostGoodbye
    case invalidTicketPeer(UInt8)
    case slotNotReserved
    case unsupportedVersion(UInt32)
}

public enum ShmGuestSendError: Error, Equatable {
    case hostGoodbye
    case payloadTooLarge
    case ringFull
    case slotExhausted
    case slotError
    case doorbellPeerDead
}

public enum ShmGuestReceiveError: Error, Equatable {
    case malformedFrame
    case slotError
    case payloadTooLarge
}

public final class ShmGuestRuntime: @unchecked Sendable {
    public let peerId: UInt8
    public private(set) var region: ShmRegion
    public private(set) var header: ShmSegmentHeader

    private var guestToHost: ShmBipBuffer
    private var hostToGuest: ShmBipBuffer
    private var slotPool: ShmVarSlotPool
    private let doorbell: ShmDoorbell?
    private var fatalError = false

    public static func attach(path: String) throws -> ShmGuestRuntime {
        let region = try ShmRegion.attach(path: path)
        return try attach(region: region, ticket: nil)
    }

    public static func attach(ticket: ShmBootstrapTicket) throws -> ShmGuestRuntime {
        let region: ShmRegion
        if ticket.shmFd >= 0 {
            region = try ShmRegion.attach(fd: ticket.shmFd, pathHint: ticket.hubPath)
        } else {
            region = try ShmRegion.attach(path: ticket.hubPath)
        }
        return try attach(region: region, ticket: ticket)
    }

    private static func attach(region: ShmRegion, ticket: ShmBootstrapTicket?) throws -> ShmGuestRuntime {
        let view = try ShmSegmentView(region: region)
        let header = view.header

        if header.hostGoodbye != 0 {
            throw ShmGuestAttachError.hostGoodbye
        }

        let classes = try ShmVarSlotPool.loadClasses(region: region, header: header)
        let pool = ShmVarSlotPool(region: region, baseOffset: Int(header.varSlotPoolOffset), classes: classes)

        let chosenPeerId: UInt8
        if let ticket {
            guard ticket.peerId >= 1, UInt32(ticket.peerId) <= header.maxGuests else {
                throw ShmGuestAttachError.invalidTicketPeer(ticket.peerId)
            }
            let offset = Int(header.peerTableOffset) + Int(ticket.peerId - 1) * shmPeerEntrySize
            let statePtr = try region.pointer(at: offset)
            var expected = ShmPeerState.reserved.rawValue
            if !atomicCompareExchangeU32(statePtr, expected: &expected, desired: ShmPeerState.attached.rawValue) {
                throw ShmGuestAttachError.slotNotReserved
            }
            chosenPeerId = ticket.peerId
        } else {
            var claimed: UInt8?
            for id in UInt8(1)...UInt8(header.maxGuests) {
                let offset = Int(header.peerTableOffset) + Int(id - 1) * shmPeerEntrySize
                let statePtr = try region.pointer(at: offset)
                var expected = ShmPeerState.empty.rawValue
                if atomicCompareExchangeU32(statePtr, expected: &expected, desired: ShmPeerState.attached.rawValue) {
                    let epochPtr = statePtr.advanced(by: 4)
                    _ = atomicFetchAddU32(epochPtr, 1)
                    claimed = id
                    break
                }
            }
            guard let peer = claimed else {
                throw ShmGuestAttachError.noPeerSlots
            }
            chosenPeerId = peer
        }

        let peerEntry = try view.peerEntry(peerId: chosenPeerId)
        let ringOffset = Int(peerEntry.ringOffset)
        let g2h = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset)
        let h2g = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset + shmBipbufHeaderSize + Int(g2h.capacity))
        let doorbell = ticket.map { ShmDoorbell(fd: $0.doorbellFd) }

        return ShmGuestRuntime(
            peerId: chosenPeerId,
            region: region,
            header: header,
            guestToHost: g2h,
            hostToGuest: h2g,
            slotPool: pool,
            doorbell: doorbell
        )
    }

    private init(
        peerId: UInt8,
        region: ShmRegion,
        header: ShmSegmentHeader,
        guestToHost: ShmBipBuffer,
        hostToGuest: ShmBipBuffer,
        slotPool: ShmVarSlotPool,
        doorbell: ShmDoorbell?
    ) {
        self.peerId = peerId
        self.region = region
        self.header = header
        self.guestToHost = guestToHost
        self.hostToGuest = hostToGuest
        self.slotPool = slotPool
        self.doorbell = doorbell
    }

    public func send(frame: ShmGuestFrame) throws {
        if fatalError || hostGoodbyeFlag() {
            throw ShmGuestSendError.hostGoodbye
        }

        let payloadLen = UInt32(frame.payload.count)
        if payloadLen > header.maxPayloadSize {
            throw ShmGuestSendError.payloadTooLarge
        }

        let threshold = header.inlineThreshold == 0 ? shmDefaultInlineThreshold : header.inlineThreshold

        if shmShouldInline(payloadLen: payloadLen, threshold: threshold) {
            let bytes = encodeShmInlineFrame(
                msgType: frame.msgType,
                id: frame.id,
                methodId: frame.methodId,
                payload: frame.payload
            )

            if let grant = try guestToHost.tryGrant(UInt32(bytes.count)) {
                grant.copyBytes(from: bytes)
                try guestToHost.commit(UInt32(bytes.count))
                try doorbell?.signal()
                return
            }

            throw ShmGuestSendError.ringFull
        }

        guard let handle = slotPool.alloc(size: payloadLen, owner: peerId) else {
            throw ShmGuestSendError.slotExhausted
        }

        guard let payloadPtr = slotPool.payloadPointer(handle) else {
            try? slotPool.freeAllocated(handle)
            throw ShmGuestSendError.slotError
        }

        frame.payload.withUnsafeBytes { raw in
            if let base = raw.baseAddress {
                memcpy(payloadPtr, base, raw.count)
            }
        }

        do {
            try slotPool.markInFlight(handle)
        } catch {
            try? slotPool.freeAllocated(handle)
            throw ShmGuestSendError.slotError
        }

        let slotFrame = encodeShmSlotRefFrame(
            msgType: frame.msgType,
            id: frame.id,
            methodId: frame.methodId,
            payloadLen: payloadLen,
            slotRef: ShmSlotRef(
                classIdx: handle.classIdx,
                extentIdx: handle.extentIdx,
                slotIdx: handle.slotIdx,
                slotGeneration: handle.generation
            )
        )

        if let grant = try guestToHost.tryGrant(UInt32(slotFrame.count)) {
            grant.copyBytes(from: slotFrame)
            try guestToHost.commit(UInt32(slotFrame.count))
            try doorbell?.signal()
            return
        }

        try? slotPool.free(handle)
        throw ShmGuestSendError.ringFull
    }

    public func receive() throws -> ShmGuestFrame? {
        if fatalError || hostGoodbyeFlag() {
            return nil
        }

        guard let readable = hostToGuest.tryRead() else {
            return nil
        }

        let bytes = Array(readable)
        let decoded: ShmDecodedFrame
        do {
            decoded = try decodeShmFrame(bytes)
        } catch {
            fatalError = true
            throw ShmGuestReceiveError.malformedFrame
        }

        switch decoded {
        case .inline(let header, let payload):
            try hostToGuest.release(header.totalLen)
            return ShmGuestFrame(msgType: header.msgType, id: header.id, methodId: header.methodId, payload: payload)

        case .slotRef(let header, let slotRef):
            let handle = ShmVarSlotHandle(
                classIdx: slotRef.classIdx,
                extentIdx: slotRef.extentIdx,
                slotIdx: slotRef.slotIdx,
                generation: slotRef.slotGeneration
            )

            guard let clsSize = slotPool.slotSize(classIdx: slotRef.classIdx) else {
                fatalError = true
                throw ShmGuestReceiveError.slotError
            }
            if header.payloadLen > clsSize {
                fatalError = true
                throw ShmGuestReceiveError.payloadTooLarge
            }

            guard let payloadPtr = slotPool.payloadPointer(handle) else {
                fatalError = true
                throw ShmGuestReceiveError.slotError
            }

            let payload = Array(
                UnsafeRawBufferPointer(start: UnsafeRawPointer(payloadPtr), count: Int(header.payloadLen))
            )

            do {
                try slotPool.free(handle)
            } catch {
                fatalError = true
                throw ShmGuestReceiveError.slotError
            }

            try hostToGuest.release(header.totalLen)
            try doorbell?.signal()

            return ShmGuestFrame(msgType: header.msgType, id: header.id, methodId: header.methodId, payload: payload)
        }
    }

    public func checkRemap() throws -> Bool {
        let currentSizePtr = try region.pointer(at: 88)
        let currentSize = Int(atomicLoadU64Acquire(UnsafeRawPointer(currentSizePtr)))
        if currentSize <= region.length {
            return false
        }

        try region.resize(newSize: currentSize)
        let view = try ShmSegmentView(region: region)
        header = view.header

        let peerEntry = try view.peerEntry(peerId: peerId)
        let ringOffset = Int(peerEntry.ringOffset)
        guestToHost = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset)
        hostToGuest = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset + shmBipbufHeaderSize + Int(guestToHost.capacity))
        slotPool.updateRegion(region)

        return true
    }

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
        guard let doorbell else {
            return nil
        }
        return try doorbell.wait(timeoutMs: timeoutMs)
    }

    public func peerState() throws -> ShmPeerState {
        let ptr = try peerStatePointer()
        let raw = atomicLoadU32Acquire(UnsafeRawPointer(ptr))
        return ShmPeerState(rawValue: raw) ?? .empty
    }

    private func hostGoodbyeFlag() -> Bool {
        guard let ptr = try? region.pointer(at: 68) else {
            return true
        }
        return atomicLoadU32Acquire(UnsafeRawPointer(ptr)) != 0
    }

    private func peerStatePointer() throws -> UnsafeMutableRawPointer {
        let offset = Int(header.peerTableOffset) + Int(peerId - 1) * shmPeerEntrySize
        return try region.pointer(at: offset)
    }

}

@inline(__always)
private func readU32LE(_ bytes: [UInt8], _ at: Int) -> UInt32 {
    UInt32(bytes[at])
        | (UInt32(bytes[at + 1]) << 8)
        | (UInt32(bytes[at + 2]) << 16)
        | (UInt32(bytes[at + 3]) << 24)
}
