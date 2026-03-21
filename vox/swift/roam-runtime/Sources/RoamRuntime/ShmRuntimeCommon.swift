import Foundation

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
