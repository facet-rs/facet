import Foundation
import CVoxShmFfi

#if os(macOS)
import Darwin
#elseif os(Linux)
import Glibc
#endif

func shmMaxVarSlotPayload(classes: [ShmVarSlotClass]) -> UInt32 {
    let maxSlotSize = classes.map(\.slotSize).max() ?? 0
    return maxSlotSize >= 4 ? maxSlotSize - 4 : 0
}

func makeShmControlEndpoints(
    doorbellFd: Int32?,
    mmapControlFd: Int32
) -> (doorbell: ShmDoorbell?, mmapAttachments: ShmMmapAttachments?) {
    let doorbell = doorbellFd.map { ShmDoorbell(fd: $0, ownsFd: true) }
    let mmapAttachments = mmapControlFd >= 0 ? ShmMmapAttachments(controlFd: mmapControlFd) : nil
    return (doorbell, mmapAttachments)
}

func attachShmPeerBuffers(
    region: ShmRegion,
    view: ShmSegmentView,
    peerId: UInt8
) throws -> (guestToHost: ShmBipBuffer, hostToGuest: ShmBipBuffer) {
    let peerEntry = try view.peerEntry(peerId: peerId)
    let ringOffset = Int(peerEntry.ringOffset)
    let guestToHost = try ShmBipBuffer.attach(region: region, headerOffset: ringOffset)
    let hostToGuest = try ShmBipBuffer.attach(
        region: region,
        headerOffset: ringOffset + shmBipbufHeaderSize + Int(guestToHost.capacity)
    )
    return (guestToHost, hostToGuest)
}

func shmPeerStatePointer(
    region: ShmRegion,
    header: ShmSegmentHeader,
    peerId: UInt8
) throws -> UnsafeMutableRawPointer {
    guard peerId >= 1, UInt32(peerId) <= header.maxGuests else {
        throw ShmLayoutError.invalidPeerId(peerId)
    }
    let offset = Int(header.peerTableOffset) + Int(peerId - 1) * shmPeerEntrySize
    return try region.pointer(at: offset)
}

func transitionShmPeerState(
    statePtr: UnsafeMutableRawPointer,
    from expectedState: ShmPeerState,
    to desiredState: ShmPeerState,
    bumpEpochOnSuccess: Bool = false
) -> Bool {
    var expected = expectedState.rawValue
    let swapped = atomicCompareExchangeU32(statePtr, expected: &expected, desired: desiredState.rawValue)
    if swapped && bumpEpochOnSuccess {
        let epochPtr = statePtr.advanced(by: 4)
        _ = atomicFetchAddU32(epochPtr, 1)
    }
    return swapped
}

func claimEmptyShmPeer(
    region: ShmRegion,
    header: ShmSegmentHeader,
    to desiredState: ShmPeerState
) throws -> UInt8? {
    for peerId in UInt8(1)...UInt8(header.maxGuests) {
        let statePtr = try shmPeerStatePointer(region: region, header: header, peerId: peerId)
        if transitionShmPeerState(
            statePtr: statePtr,
            from: .empty,
            to: desiredState,
            bumpEpochOnSuccess: true
        ) {
            return peerId
        }
    }
    return nil
}

@inline(__always)
func shmCurrentSizeOffset(version: UInt32) -> Int {
    version == 7 ? 72 : 88
}

@inline(__always)
func shmHostGoodbyeOffset(version: UInt32) -> Int {
    version == 7 ? 64 : 68
}

func checkShmRemap(
    region: ShmRegion,
    header: inout ShmSegmentHeader,
    reattach: (ShmSegmentView) throws -> Void
) throws -> Bool {
    let currentSizePtr = try region.pointer(at: shmCurrentSizeOffset(version: header.version))
    let currentSize = Int(atomicLoadU64Acquire(UnsafeRawPointer(currentSizePtr)))
    if currentSize <= region.length {
        return false
    }

    try region.resize(newSize: currentSize)
    let view = try ShmSegmentView(region: region)
    header = view.header
    try reattach(view)
    return true
}

func shmHostGoodbyeFlag(region: ShmRegion, header: ShmSegmentHeader) -> Bool {
    guard let ptr = try? region.pointer(at: shmHostGoodbyeOffset(version: header.version)) else {
        return true
    }
    return atomicLoadU32Acquire(UnsafeRawPointer(ptr)) != 0
}

func waitForShmDoorbell(
    doorbell: ShmDoorbell?,
    timeoutMs: Int32? = nil,
    onSignal: () throws -> Void
) throws -> ShmDoorbellWaitResult? {
    guard let doorbell else {
        return nil
    }
    let result = try doorbell.wait(timeoutMs: timeoutMs)
    if result == .signaled {
        try onSignal()
    }
    return result
}

@inline(__always)
func closeShmMmapControlFd(_ fd: inout Int32) {
    guard fd >= 0 else {
        return
    }
    close(fd)
    fd = -1
}

@inline(__always)
func shmAlignUp(_ value: Int, _ alignment: Int) -> Int {
    ((value + alignment - 1) / alignment) * alignment
}

@inline(__always)
func shouldTerminateShmReceive(
    fatalError: Bool,
    sawHostGoodbye: Bool,
    sawPeerGoodbye: Bool = false
) -> Bool {
    fatalError || sawHostGoodbye || sawPeerGoodbye
}

func detachShmRuntime(
    statePtr: UnsafeMutableRawPointer?,
    doorbell: ShmDoorbell?,
    drain: () throws -> ShmGuestFrame?,
    closeMmapControl: (() -> Void)? = nil
) {
    if let statePtr {
        atomicStoreU32Release(statePtr, ShmPeerState.goodbye.rawValue)
    }
    try? doorbell?.signal()
    while (try? drain()) != nil {}
    closeMmapControl?()
}

private struct ShmOutboundMmapRegion {
    let region: ShmRegion
    let mapId: UInt32
    let mapGeneration: UInt32
    let mappingLength: Int
    var nextOffset: Int
}

final class ShmOutboundMmapAllocator: @unchecked Sendable {
    private let pathPrefix: String
    private let minRegionSize: Int
    private var nextMmapId: UInt32 = 1
    private var activeRegion: ShmOutboundMmapRegion?
    private var retiredRegions: [ShmRegion] = []

    init(pathPrefix: String, minRegionSize: Int = 0) {
        self.pathPrefix = pathPrefix
        self.minRegionSize = minRegionSize
    }

    func allocateSlice<E: Error>(
        payloadCount: Int,
        mmapControlFd: Int32,
        allocationFailed: E,
        controlError: (Int32) -> E
    ) throws -> (region: ShmRegion, mapId: UInt32, mapGeneration: UInt32, mapOffset: Int) {
        if var active = activeRegion, active.nextOffset + payloadCount <= active.mappingLength {
            let offset = active.nextOffset
            active.nextOffset += payloadCount
            activeRegion = active
            return (active.region, active.mapId, active.mapGeneration, offset)
        }

        let pageSize = max(Int(getpagesize()), 4096)
        let regionSize = shmAlignUp(max(payloadCount, minRegionSize), pageSize)
        let mmapPath = "\(NSTemporaryDirectory())\(pathPrefix)\(UUID().uuidString).bin"
        let region: ShmRegion
        do {
            region = try ShmRegion.create(path: mmapPath, size: regionSize, cleanup: .auto)
        } catch {
            throw allocationFailed
        }

        let mapId = allocateMmapId()
        let mapGeneration: UInt32 = 1
        let sendRc = vox_mmap_control_send(
            mmapControlFd,
            region.rawFd,
            mapId,
            mapGeneration,
            UInt64(regionSize)
        )
        guard sendRc == 0 else {
            throw controlError(errno)
        }

        if let previous = activeRegion {
            retiredRegions.append(previous.region)
        }
        activeRegion = ShmOutboundMmapRegion(
            region: region,
            mapId: mapId,
            mapGeneration: mapGeneration,
            mappingLength: regionSize,
            nextOffset: payloadCount
        )
        return (region, mapId, mapGeneration, 0)
    }

    private func allocateMmapId() -> UInt32 {
        let mapId = nextMmapId
        nextMmapId &+= 1
        if nextMmapId == 0 {
            nextMmapId = 1
        }
        return mapId
    }
}
