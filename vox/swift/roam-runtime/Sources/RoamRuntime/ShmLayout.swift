import Foundation

public let shmSegmentMagic = Array("RAPAHUB\u{01}".utf8)
public let shmSegmentHeaderSize = 128
public let shmSegmentVersion: UInt32 = 2
public let shmPeerEntrySize = 64
public let shmChannelEntrySize = 16
public let shmBipbufHeaderSize = 128

public enum ShmLayoutError: Error, Equatable {
    case headerTooShort(Int)
    case invalidMagic([UInt8])
    case unsupportedVersion(UInt32)
    case invalidHeaderSize(UInt32)
    case invalidSlotSize(UInt32)
    case missingVarSlotPool
    case invalidMaxGuests(UInt32)
    case currentSizeLessThanTotal(current: UInt64, total: UInt64)
    case offsetOutOfBounds(field: String, offset: UInt64, size: UInt64, regionSize: UInt64)
    case reservedByteNotZero(index: Int)
    case invalidPeerId(UInt8)
    case invalidChannelIndex(UInt32)
}

public struct ShmSegmentHeader: Sendable, Equatable {
    public let magic: [UInt8]
    public let version: UInt32
    public let headerSize: UInt32
    public let totalSize: UInt64
    public let maxPayloadSize: UInt32
    public let initialCredit: UInt32
    public let maxGuests: UInt32
    public let bipbufCapacity: UInt32
    public let peerTableOffset: UInt64
    public let slotRegionOffset: UInt64
    public let slotSize: UInt32
    public let inlineThreshold: UInt32
    public let maxChannels: UInt32
    public let hostGoodbye: UInt32
    public let heartbeatInterval: UInt64
    public let varSlotPoolOffset: UInt64
    public let currentSize: UInt64
    public let guestAreasOffset: UInt64
    public let numVarSlotClasses: UInt32
    public let reserved: [UInt8]

    public static func decode(from bytes: [UInt8]) throws -> ShmSegmentHeader {
        guard bytes.count >= shmSegmentHeaderSize else {
            throw ShmLayoutError.headerTooShort(bytes.count)
        }

        return ShmSegmentHeader(
            magic: Array(bytes[0..<8]),
            version: readU32LE(bytes, 8),
            headerSize: readU32LE(bytes, 12),
            totalSize: readU64LE(bytes, 16),
            maxPayloadSize: readU32LE(bytes, 24),
            initialCredit: readU32LE(bytes, 28),
            maxGuests: readU32LE(bytes, 32),
            bipbufCapacity: readU32LE(bytes, 36),
            peerTableOffset: readU64LE(bytes, 40),
            slotRegionOffset: readU64LE(bytes, 48),
            slotSize: readU32LE(bytes, 56),
            inlineThreshold: readU32LE(bytes, 60),
            maxChannels: readU32LE(bytes, 64),
            hostGoodbye: readU32LE(bytes, 68),
            heartbeatInterval: readU64LE(bytes, 72),
            varSlotPoolOffset: readU64LE(bytes, 80),
            currentSize: readU64LE(bytes, 88),
            guestAreasOffset: readU64LE(bytes, 96),
            numVarSlotClasses: readU32LE(bytes, 104),
            reserved: Array(bytes[108..<128])
        )
    }

    public func validateV2(mappedSize: UInt64? = nil) throws {
        if magic != shmSegmentMagic {
            throw ShmLayoutError.invalidMagic(magic)
        }
        if version != shmSegmentVersion {
            throw ShmLayoutError.unsupportedVersion(version)
        }
        if headerSize != UInt32(shmSegmentHeaderSize) {
            throw ShmLayoutError.invalidHeaderSize(headerSize)
        }
        if slotSize != 0 {
            throw ShmLayoutError.invalidSlotSize(slotSize)
        }
        if varSlotPoolOffset == 0 {
            throw ShmLayoutError.missingVarSlotPool
        }
        if maxGuests == 0 || maxGuests > 255 {
            throw ShmLayoutError.invalidMaxGuests(maxGuests)
        }
        if currentSize < totalSize {
            throw ShmLayoutError.currentSizeLessThanTotal(current: currentSize, total: totalSize)
        }
        for (i, byte) in reserved.enumerated() where byte != 0 {
            throw ShmLayoutError.reservedByteNotZero(index: i)
        }

        let regionLimit = mappedSize ?? currentSize
        try ensureBounds(field: "peer_table", offset: peerTableOffset, size: UInt64(maxGuests) * UInt64(shmPeerEntrySize),
                         regionSize: regionLimit)
        try ensureBounds(field: "var_slot_pool", offset: varSlotPoolOffset, size: 1, regionSize: regionLimit)
        try ensureBounds(field: "guest_areas", offset: guestAreasOffset, size: 1, regionSize: regionLimit)
    }
}

public struct ShmPeerEntryView: Sendable, Equatable {
    public let state: UInt32
    public let epoch: UInt32
    public let guestToHostHead: UInt32
    public let guestToHostTail: UInt32
    public let hostToGuestHead: UInt32
    public let hostToGuestTail: UInt32
    public let lastHeartbeat: UInt64
    public let ringOffset: UInt64
    public let slotPoolOffset: UInt64
    public let channelTableOffset: UInt64
    public let reserved: [UInt8]

    static func decode(from bytes: [UInt8]) -> ShmPeerEntryView {
        ShmPeerEntryView(
            state: readU32LE(bytes, 0),
            epoch: readU32LE(bytes, 4),
            guestToHostHead: readU32LE(bytes, 8),
            guestToHostTail: readU32LE(bytes, 12),
            hostToGuestHead: readU32LE(bytes, 16),
            hostToGuestTail: readU32LE(bytes, 20),
            lastHeartbeat: readU64LE(bytes, 24),
            ringOffset: readU64LE(bytes, 32),
            slotPoolOffset: readU64LE(bytes, 40),
            channelTableOffset: readU64LE(bytes, 48),
            reserved: Array(bytes[56..<64])
        )
    }
}

public struct ShmChannelEntryView: Sendable, Equatable {
    public let state: UInt32
    public let grantedTotal: UInt32
    public let reserved: [UInt8]

    static func decode(from bytes: [UInt8]) -> ShmChannelEntryView {
        ShmChannelEntryView(
            state: readU32LE(bytes, 0),
            grantedTotal: readU32LE(bytes, 4),
            reserved: Array(bytes[8..<16])
        )
    }
}

public struct ShmSegmentView: Sendable {
    public let region: ShmRegion
    public let header: ShmSegmentHeader

    public init(region: ShmRegion) throws {
        let headerBytes = try Array(region.mutableBytes(at: 0, count: shmSegmentHeaderSize))
        let header = try ShmSegmentHeader.decode(from: headerBytes)
        try header.validateV2(mappedSize: UInt64(region.length))
        self.region = region
        self.header = header
    }

    public func peerEntryOffset(peerId: UInt8) throws -> Int {
        guard peerId >= 1, UInt32(peerId) <= header.maxGuests else {
            throw ShmLayoutError.invalidPeerId(peerId)
        }
        let idx = UInt64(peerId - 1)
        let offset = header.peerTableOffset + (idx * UInt64(shmPeerEntrySize))
        try ensureBounds(
            field: "peer_entry",
            offset: offset,
            size: UInt64(shmPeerEntrySize),
            regionSize: UInt64(region.length)
        )
        return Int(offset)
    }

    public func peerEntry(peerId: UInt8) throws -> ShmPeerEntryView {
        let offset = try peerEntryOffset(peerId: peerId)
        let bytes = Array(try region.mutableBytes(at: offset, count: shmPeerEntrySize))
        return ShmPeerEntryView.decode(from: bytes)
    }

    public func channelEntryOffset(peerId: UInt8, channelIndex: UInt32) throws -> Int {
        guard channelIndex < header.maxChannels else {
            throw ShmLayoutError.invalidChannelIndex(channelIndex)
        }

        let peer = try peerEntry(peerId: peerId)
        let offset = peer.channelTableOffset + UInt64(channelIndex) * UInt64(shmChannelEntrySize)
        try ensureBounds(
            field: "channel_entry",
            offset: offset,
            size: UInt64(shmChannelEntrySize),
            regionSize: UInt64(region.length)
        )
        return Int(offset)
    }

    public func channelEntry(peerId: UInt8, channelIndex: UInt32) throws -> ShmChannelEntryView {
        let offset = try channelEntryOffset(peerId: peerId, channelIndex: channelIndex)
        let bytes = Array(try region.mutableBytes(at: offset, count: shmChannelEntrySize))
        return ShmChannelEntryView.decode(from: bytes)
    }
}

@inline(__always)
private func readU32LE(_ bytes: [UInt8], _ at: Int) -> UInt32 {
    UInt32(bytes[at])
        | (UInt32(bytes[at + 1]) << 8)
        | (UInt32(bytes[at + 2]) << 16)
        | (UInt32(bytes[at + 3]) << 24)
}

@inline(__always)
private func readU64LE(_ bytes: [UInt8], _ at: Int) -> UInt64 {
    let b0 = UInt64(bytes[at])
    let b1 = UInt64(bytes[at + 1]) << 8
    let b2 = UInt64(bytes[at + 2]) << 16
    let b3 = UInt64(bytes[at + 3]) << 24
    let b4 = UInt64(bytes[at + 4]) << 32
    let b5 = UInt64(bytes[at + 5]) << 40
    let b6 = UInt64(bytes[at + 6]) << 48
    let b7 = UInt64(bytes[at + 7]) << 56
    return b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7
}

private func ensureBounds(field: String, offset: UInt64, size: UInt64, regionSize: UInt64) throws {
    if offset > regionSize || size > regionSize || offset + size > regionSize {
        throw ShmLayoutError.offsetOutOfBounds(
            field: field,
            offset: offset,
            size: size,
            regionSize: regionSize
        )
    }
}
