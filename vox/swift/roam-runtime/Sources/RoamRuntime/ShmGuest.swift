import Foundation
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
}

public final class ShmVarSlotPool: @unchecked Sendable {
    private static let sizeClassHeaderSize = 64
    private static let varSlotMetaSize = 16
    private static let freeListEnd = UInt64.max
    private static let maxExtentsPerClass = 3

    private let baseOffset: Int
    private let classes: [ShmVarSlotClass]
    private let extent0MetaOffsets: [Int]
    private let extent0DataOffsets: [Int]

    private var region: ShmRegion

    public init(region: ShmRegion, baseOffset: Int, classes: [ShmVarSlotClass]) {
        self.region = region
        self.baseOffset = baseOffset
        self.classes = classes

        var metaOffsets: [Int] = []
        var dataOffsets: [Int] = []
        metaOffsets.reserveCapacity(classes.count)
        dataOffsets.reserveCapacity(classes.count)

        var offset = baseOffset + classes.count * Self.sizeClassHeaderSize
        for cls in classes {
            offset = alignUp(offset, to: Self.varSlotMetaSize)
            metaOffsets.append(offset)
            offset += Int(cls.count) * Self.varSlotMetaSize

            offset = alignUp(offset, to: 64)
            dataOffsets.append(offset)
            offset += Int(cls.count) * Int(cls.slotSize)
        }

        self.extent0MetaOffsets = metaOffsets
        self.extent0DataOffsets = dataOffsets
    }

    public static func calculateSize(classes: [ShmVarSlotClass]) -> Int {
        var size = classes.count * sizeClassHeaderSize
        for cls in classes {
            size = alignUp(size, to: varSlotMetaSize)
            size += Int(cls.count) * varSlotMetaSize
            size = alignUp(size, to: 64)
            size += Int(cls.count) * Int(cls.slotSize)
        }
        return alignUp(size, to: 64)
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
    }

    public func initialize() throws {
        for classIdx in classes.indices {
            try initializeClassHeader(classIdx)
        }

        for classIdx in classes.indices {
            try initializeExtentSlots(classIdx: classIdx, extentIdx: 0)
        }
    }

    public func alloc(size: UInt32, owner: UInt8) throws -> ShmVarSlotHandle? {
        for (classIdx, cls) in classes.enumerated() where cls.slotSize >= size {
            if let handle = try allocFromClass(classIdx: classIdx, owner: owner) {
                return handle
            }
        }
        return nil
    }

    public func allocFromClass(classIdx: Int, owner: UInt8) throws -> ShmVarSlotHandle? {
        guard classIdx >= 0 && classIdx < classes.count else {
            return nil
        }

        let headerOffset = classHeaderOffset(classIdx)
        let extentCountPtr = try region.pointer(at: headerOffset + 8)
        let extentCount = Int(atomicLoadU32Acquire(UnsafeRawPointer(extentCountPtr)))

        for extentIdx in 0..<extentCount {
            if let handle = try allocFromExtent(classIdx: classIdx, extentIdx: extentIdx, owner: owner) {
                return handle
            }
        }

        return nil
    }

    public func markInFlight(_ handle: ShmVarSlotHandle) throws {
        try validateHandle(handle)
        let meta = try metaPointer(handle)

        let actual = atomicLoadU32Acquire(UnsafeRawPointer(meta))
        if actual != handle.generation {
            throw ShmVarSlotFreeError.generationMismatch(expected: handle.generation, actual: actual)
        }

        let statePtr = meta.advanced(by: 4)
        var expected = ShmSlotState.allocated.rawValue
        let ok = atomicCompareExchangeU32(statePtr, expected: &expected, desired: ShmSlotState.inFlight.rawValue)
        if !ok {
            throw ShmVarSlotFreeError.invalidState(
                expected: .allocated,
                actual: ShmSlotState(rawValue: expected) ?? .free
            )
        }
    }

    public func free(_ handle: ShmVarSlotHandle) throws {
        try free(handle, expectedState: .inFlight)
    }

    public func freeAllocated(_ handle: ShmVarSlotHandle) throws {
        try free(handle, expectedState: .allocated)
    }

    public func slotSize(classIdx: UInt8) -> UInt32? {
        classes[safe: Int(classIdx)]?.slotSize
    }

    public func payloadPointer(_ handle: ShmVarSlotHandle) throws -> UnsafeMutableRawPointer? {
        guard let cls = classes[safe: Int(handle.classIdx)] else {
            return nil
        }
        guard handle.slotIdx < cls.count else {
            return nil
        }

        let extentIdx = Int(handle.extentIdx)
        if extentIdx == 0 {
            let offset = extent0DataOffsets[Int(handle.classIdx)] + Int(handle.slotIdx) * Int(cls.slotSize)
            return try region.pointer(at: offset)
        }

        guard extentIdx < Self.maxExtentsPerClass else {
            return nil
        }

        let headerOffset = classHeaderOffset(Int(handle.classIdx))
        let extentOffsetPtr = try region.pointer(at: headerOffset + 40 + (extentIdx - 1) * 8)
        let extentOffset = atomicLoadU64Acquire(UnsafeRawPointer(extentOffsetPtr))
        guard extentOffset > 0 else {
            return nil
        }

        let metaSize = Int(cls.count) * Self.varSlotMetaSize
        let dataStart = Int(extentOffset) + 64 + alignUp(metaSize, to: 64)
        let offset = dataStart + Int(handle.slotIdx) * Int(cls.slotSize)
        return try region.pointer(at: offset)
    }

    public func slotState(_ handle: ShmVarSlotHandle) throws -> ShmSlotState {
        let meta = try metaPointer(handle)
        let value = atomicLoadU32Acquire(UnsafeRawPointer(meta.advanced(by: 4)))
        return ShmSlotState(rawValue: value) ?? .free
    }

    public func recoverPeer(ownerPeer: UInt8) throws {
        for classIdx in classes.indices {
            let headerOffset = classHeaderOffset(classIdx)
            let extentCount = Int(atomicLoadU32Acquire(UnsafeRawPointer(try region.pointer(at: headerOffset + 8))))
            for extentIdx in 0..<extentCount {
                for slotIdx in 0..<classes[classIdx].count {
                    let handle = ShmVarSlotHandle(
                        classIdx: UInt8(classIdx),
                        extentIdx: UInt8(extentIdx),
                        slotIdx: slotIdx,
                        generation: 0
                    )
                    guard let meta = try payloadMetaPointer(for: handle) else {
                        continue
                    }
                    let owner = atomicLoadU32Acquire(UnsafeRawPointer(meta.advanced(by: 8)))
                    let state = atomicLoadU32Acquire(UnsafeRawPointer(meta.advanced(by: 4)))
                    if owner == UInt32(ownerPeer), state != ShmSlotState.free.rawValue {
                        atomicStoreU32Release(meta.advanced(by: 4), ShmSlotState.free.rawValue)
                        try pushToFreeList(classIdx: classIdx, extentIdx: extentIdx, slotIdx: slotIdx)
                    }
                }
            }
        }
    }

    private func initializeClassHeader(_ classIdx: Int) throws {
        let cls = classes[classIdx]
        let headerOffset = classHeaderOffset(classIdx)
        var header = [UInt8](repeating: 0, count: Self.sizeClassHeaderSize)
        writeU32LE(cls.slotSize, to: &header, at: 0)
        writeU32LE(cls.count, to: &header, at: 4)
        writeU32LE(1, to: &header, at: 8)

        let bytes = try region.mutableBytes(at: headerOffset, count: Self.sizeClassHeaderSize)
        bytes.copyBytes(from: header)

        for extent in 0..<Self.maxExtentsPerClass {
            let freeHeadPtr = try region.pointer(at: headerOffset + 16 + extent * 8)
            atomicStoreU64Release(freeHeadPtr, Self.freeListEnd)
        }
        for extent in 0..<(Self.maxExtentsPerClass - 1) {
            let offsetPtr = try region.pointer(at: headerOffset + 40 + extent * 8)
            atomicStoreU64Release(offsetPtr, 0)
        }
    }

    private func initializeExtentSlots(classIdx: Int, extentIdx: Int) throws {
        let cls = classes[classIdx]
        for slot in 0..<cls.count {
            guard let meta = try payloadMetaPointer(for: ShmVarSlotHandle(
                classIdx: UInt8(classIdx),
                extentIdx: UInt8(extentIdx),
                slotIdx: slot,
                generation: 0
            )) else {
                continue
            }
            atomicStoreU32Release(meta, 0)
            atomicStoreU32Release(meta.advanced(by: 4), ShmSlotState.free.rawValue)
            atomicStoreU32Release(meta.advanced(by: 8), 0)
            let next = slot + 1 < cls.count ? slot + 1 : UInt32.max
            atomicStoreU32Release(meta.advanced(by: 12), next)
        }

        let freeHeadPtr = try region.pointer(at: classHeaderOffset(classIdx) + 16 + extentIdx * 8)
        if cls.count > 0 {
            atomicStoreU64Release(freeHeadPtr, pack(index: 0, generation: 0))
        }
    }

    private func allocFromExtent(classIdx: Int, extentIdx: Int, owner: UInt8) throws -> ShmVarSlotHandle? {
        let freeHeadPtr = try region.pointer(at: classHeaderOffset(classIdx) + 16 + extentIdx * 8)

        while true {
            let head = atomicLoadU64Acquire(UnsafeRawPointer(freeHeadPtr))
            if head == Self.freeListEnd {
                return nil
            }

            let (index, tag) = unpack(head)
            guard let meta = try payloadMetaPointer(for: ShmVarSlotHandle(
                classIdx: UInt8(classIdx),
                extentIdx: UInt8(extentIdx),
                slotIdx: index,
                generation: 0
            )) else {
                return nil
            }

            let next = atomicLoadU32Acquire(UnsafeRawPointer(meta.advanced(by: 12)))
            let nextPacked: UInt64 = next == UInt32.max
                ? Self.freeListEnd
                : pack(index: next, generation: tag &+ 1)

            var expected = head
            let popped = atomicCompareExchangeU64(freeHeadPtr, expected: &expected, desired: nextPacked)
            if !popped {
                continue
            }

            let generation = atomicFetchAddU32(meta, 1) &+ 1
            atomicStoreU32Release(meta.advanced(by: 4), ShmSlotState.allocated.rawValue)
            atomicStoreU32Release(meta.advanced(by: 8), UInt32(owner))

            return ShmVarSlotHandle(
                classIdx: UInt8(classIdx),
                extentIdx: UInt8(extentIdx),
                slotIdx: index,
                generation: generation
            )
        }
    }

    private func free(_ handle: ShmVarSlotHandle, expectedState: ShmSlotState) throws {
        try validateHandle(handle)
        let meta = try metaPointer(handle)

        let actualGen = atomicLoadU32Acquire(UnsafeRawPointer(meta))
        if actualGen != handle.generation {
            throw ShmVarSlotFreeError.generationMismatch(expected: handle.generation, actual: actualGen)
        }

        let statePtr = meta.advanced(by: 4)
        var expected = expectedState.rawValue
        let ok = atomicCompareExchangeU32(statePtr, expected: &expected, desired: ShmSlotState.free.rawValue)
        if !ok {
            throw ShmVarSlotFreeError.invalidState(
                expected: expectedState,
                actual: ShmSlotState(rawValue: expected) ?? .free
            )
        }

        try pushToFreeList(classIdx: Int(handle.classIdx), extentIdx: Int(handle.extentIdx), slotIdx: handle.slotIdx)
    }

    private func pushToFreeList(classIdx: Int, extentIdx: Int, slotIdx: UInt32) throws {
        let freeHeadPtr = try region.pointer(at: classHeaderOffset(classIdx) + 16 + extentIdx * 8)
        guard let meta = try payloadMetaPointer(for: ShmVarSlotHandle(
            classIdx: UInt8(classIdx),
            extentIdx: UInt8(extentIdx),
            slotIdx: slotIdx,
            generation: 0
        )) else {
            return
        }

        while true {
            let head = atomicLoadU64Acquire(UnsafeRawPointer(freeHeadPtr))
            let (headIndex, headGen): (UInt32, UInt32)
            if head == Self.freeListEnd {
                headIndex = UInt32.max
                headGen = 0
            } else {
                (headIndex, headGen) = unpack(head)
            }

            atomicStoreU32Release(meta.advanced(by: 12), headIndex)
            let newHead = pack(index: slotIdx, generation: headGen &+ 1)

            var expected = head
            if atomicCompareExchangeU64(freeHeadPtr, expected: &expected, desired: newHead) {
                return
            }
        }
    }

    private func validateHandle(_ handle: ShmVarSlotHandle) throws {
        guard Int(handle.classIdx) < classes.count else {
            throw ShmVarSlotFreeError.invalidClass
        }
        let cls = classes[Int(handle.classIdx)]
        if handle.slotIdx >= cls.count {
            throw ShmVarSlotFreeError.invalidIndex
        }
    }

    private func classHeaderOffset(_ classIdx: Int) -> Int {
        baseOffset + classIdx * Self.sizeClassHeaderSize
    }

    private func payloadMetaPointer(for handle: ShmVarSlotHandle) throws -> UnsafeMutableRawPointer? {
        guard let cls = classes[safe: Int(handle.classIdx)] else {
            return nil
        }
        guard handle.slotIdx < cls.count else {
            return nil
        }

        let extentIdx = Int(handle.extentIdx)
        if extentIdx == 0 {
            let offset = extent0MetaOffsets[Int(handle.classIdx)] + Int(handle.slotIdx) * Self.varSlotMetaSize
            return try region.pointer(at: offset)
        }

        guard extentIdx < Self.maxExtentsPerClass else {
            return nil
        }

        let headerOffset = classHeaderOffset(Int(handle.classIdx))
        let extentOffsetPtr = try region.pointer(at: headerOffset + 40 + (extentIdx - 1) * 8)
        let extentOffset = atomicLoadU64Acquire(UnsafeRawPointer(extentOffsetPtr))
        guard extentOffset > 0 else {
            return nil
        }

        let metaOffset = Int(extentOffset) + 64 + Int(handle.slotIdx) * Self.varSlotMetaSize
        return try region.pointer(at: metaOffset)
    }

    private func metaPointer(_ handle: ShmVarSlotHandle) throws -> UnsafeMutableRawPointer {
        guard let ptr = try payloadMetaPointer(for: handle) else {
            throw ShmVarSlotFreeError.invalidIndex
        }
        return ptr
    }

    @inline(__always)
    private func pack(index: UInt32, generation: UInt32) -> UInt64 {
        (UInt64(index) << 32) | UInt64(generation)
    }

    @inline(__always)
    private func unpack(_ value: UInt64) -> (UInt32, UInt32) {
        (UInt32(value >> 32), UInt32(truncatingIfNeeded: value))
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
                write(fd, ptr, 1)
            }
            if written == 1 {
                return
            }
            if written < 0 && errno == EINTR {
                continue
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

        while true {
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

                if try waitForRetry() {
                    continue
                }
                throw ShmGuestSendError.ringFull
            }

            guard let handle = try slotPool.alloc(size: payloadLen, owner: peerId) else {
                if try waitForRetry() {
                    continue
                }
                throw ShmGuestSendError.slotExhausted
            }

            guard let payloadPtr = try slotPool.payloadPointer(handle) else {
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
            if try waitForRetry() {
                continue
            }
            throw ShmGuestSendError.ringFull
        }
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

            guard let payloadPtr = try slotPool.payloadPointer(handle) else {
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

    private func waitForRetry() throws -> Bool {
        guard let doorbell else {
            return false
        }
        let waitResult = try doorbell.wait(timeoutMs: 100)
        switch waitResult {
        case .signaled:
            return true
        case .timeout:
            return false
        case .peerDead:
            throw ShmGuestSendError.doorbellPeerDead
        }
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        guard index >= 0 && index < count else {
            return nil
        }
        return self[index]
    }
}

@inline(__always)
private func alignUp(_ value: Int, to alignment: Int) -> Int {
    let mask = alignment - 1
    return (value + mask) & ~mask
}

@inline(__always)
private func readU32LE(_ bytes: [UInt8], _ at: Int) -> UInt32 {
    UInt32(bytes[at])
        | (UInt32(bytes[at + 1]) << 8)
        | (UInt32(bytes[at + 2]) << 16)
        | (UInt32(bytes[at + 3]) << 24)
}

@inline(__always)
private func writeU32LE(_ value: UInt32, to bytes: inout [UInt8], at index: Int) {
    let le = value.littleEndian
    bytes[index] = UInt8(truncatingIfNeeded: le)
    bytes[index + 1] = UInt8(truncatingIfNeeded: le >> 8)
    bytes[index + 2] = UInt8(truncatingIfNeeded: le >> 16)
    bytes[index + 3] = UInt8(truncatingIfNeeded: le >> 24)
}
