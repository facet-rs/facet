import Foundation

// r[impl shm.framing.header]
public let shmFrameHeaderSize = 8
// r[impl shm.framing.slot-ref]
public let shmSlotRefSize = 12
// r[impl shm.framing.slot-ref]
public let shmSlotRefFrameSize = 20
// r[impl shm.framing.mmap-ref]
public let shmMmapRefSize = 24
// r[impl shm.framing.mmap-ref]
public let shmMmapRefFrameSize = 32
// r[impl shm.framing.threshold]
public let shmDefaultInlineThreshold: UInt32 = 256
// r[impl shm.framing.flags]
public let shmFlagSlotRef: UInt8 = 0x01
// r[impl shm.framing.flags]
public let shmFlagMmapRef: UInt8 = 0x02

public enum ShmFrameDecodeError: Error, Equatable {
    case shortHeader
    case shortFrame(required: Int, available: Int)
    case invalidTotalLength(UInt32)
    case invalidFlags(UInt8)
    case shortSlotRef
    case shortMmapRef
}

public struct ShmFrameHeader: Sendable, Equatable {
    public var totalLen: UInt32
    public var flags: UInt8
    /// For inline frames: the actual payload length (without padding).
    /// Zero means "unknown" (legacy writer); reader uses `totalLen - 8`.
    public var inlinePayloadLen: UInt16

    public init(totalLen: UInt32, flags: UInt8, inlinePayloadLen: UInt16 = 0) {
        self.totalLen = totalLen
        self.flags = flags
        self.inlinePayloadLen = inlinePayloadLen
    }

    public var hasSlotRef: Bool {
        flags & shmFlagSlotRef != 0
    }

    public var hasMmapRef: Bool {
        flags & shmFlagMmapRef != 0
    }

    public func write(to buffer: inout [UInt8]) {
        precondition(buffer.count >= shmFrameHeaderSize)
        writeU32LE(totalLen, to: &buffer, at: 0)
        buffer[4] = flags
        buffer[5] = 0  // reserved
        buffer[6] = UInt8(truncatingIfNeeded: inlinePayloadLen)
        buffer[7] = UInt8(truncatingIfNeeded: inlinePayloadLen >> 8)
    }

    public static func read(from buffer: [UInt8]) -> ShmFrameHeader? {
        guard buffer.count >= shmFrameHeaderSize else {
            return nil
        }

        return ShmFrameHeader(
            totalLen: readU32LE(from: buffer, at: 0),
            flags: buffer[4],
            inlinePayloadLen: UInt16(buffer[6]) | (UInt16(buffer[7]) << 8)
        )
    }
}

public struct ShmSlotRef: Sendable, Equatable {
    public var classIdx: UInt8
    public var extentIdx: UInt8
    public var slotIdx: UInt32
    public var slotGeneration: UInt32

    public init(classIdx: UInt8, extentIdx: UInt8, slotIdx: UInt32, slotGeneration: UInt32) {
        self.classIdx = classIdx
        self.extentIdx = extentIdx
        self.slotIdx = slotIdx
        self.slotGeneration = slotGeneration
    }

    public func write(to buffer: inout [UInt8]) {
        precondition(buffer.count >= shmSlotRefSize)
        buffer[0] = classIdx
        buffer[1] = extentIdx
        buffer[2] = 0
        buffer[3] = 0
        writeU32LE(slotIdx, to: &buffer, at: 4)
        writeU32LE(slotGeneration, to: &buffer, at: 8)
    }

    public static func read(from buffer: [UInt8]) -> ShmSlotRef? {
        guard buffer.count >= shmSlotRefSize else {
            return nil
        }
        return ShmSlotRef(
            classIdx: buffer[0],
            extentIdx: buffer[1],
            slotIdx: readU32LE(from: buffer, at: 4),
            slotGeneration: readU32LE(from: buffer, at: 8)
        )
    }
}

public struct ShmMmapRef: Sendable, Equatable {
    public var mapId: UInt32
    public var mapGeneration: UInt32
    public var mapOffset: UInt64
    public var payloadLen: UInt32

    public init(mapId: UInt32, mapGeneration: UInt32, mapOffset: UInt64, payloadLen: UInt32) {
        self.mapId = mapId
        self.mapGeneration = mapGeneration
        self.mapOffset = mapOffset
        self.payloadLen = payloadLen
    }
}

public enum ShmDecodedFrame: Sendable, Equatable {
    case inline(header: ShmFrameHeader, payload: [UInt8])
    case slotRef(header: ShmFrameHeader, slotRef: ShmSlotRef)
    case mmapRef(header: ShmFrameHeader, mmapRef: ShmMmapRef)
}

@inline(__always)
// r[impl shm.framing.inline]
// r[impl shm.framing.alignment]
public func shmInlineFrameSize(payloadLen: UInt32) -> UInt32 {
    (UInt32(shmFrameHeaderSize) + payloadLen + 3) & ~3
}

@inline(__always)
// r[impl shm.framing.threshold]
public func shmShouldInline(payloadLen: UInt32, threshold: UInt32) -> Bool {
    UInt32(shmFrameHeaderSize) + payloadLen <= threshold
}

// r[impl shm.framing.inline]
public func encodeShmInlineFrame(payload: [UInt8]) -> [UInt8] {
    let payloadLen = UInt32(payload.count)
    let totalLen = shmInlineFrameSize(payloadLen: payloadLen)
    var bytes = [UInt8](repeating: 0, count: Int(totalLen))

    let header = ShmFrameHeader(totalLen: totalLen, flags: 0, inlinePayloadLen: UInt16(payload.count))
    header.write(to: &bytes)
    bytes.replaceSubrange(shmFrameHeaderSize..<(shmFrameHeaderSize + payload.count), with: payload)
    return bytes
}

// r[impl shm.framing.slot-ref]
public func encodeShmSlotRefFrame(slotRef: ShmSlotRef) -> [UInt8] {
    var bytes = [UInt8](repeating: 0, count: shmSlotRefFrameSize)
    let header = ShmFrameHeader(totalLen: UInt32(shmSlotRefFrameSize), flags: shmFlagSlotRef)
    header.write(to: &bytes)

    var slotRefBuf = [UInt8](repeating: 0, count: shmSlotRefSize)
    slotRef.write(to: &slotRefBuf)
    bytes.replaceSubrange(shmFrameHeaderSize..<(shmFrameHeaderSize + shmSlotRefSize), with: slotRefBuf)
    return bytes
}

// r[impl shm.framing.mmap-ref]
public func encodeShmMmapRefFrame(mmapRef: ShmMmapRef) -> [UInt8] {
    var bytes = [UInt8](repeating: 0, count: shmMmapRefFrameSize)
    let header = ShmFrameHeader(totalLen: UInt32(shmMmapRefFrameSize), flags: shmFlagMmapRef)
    header.write(to: &bytes)

    writeU32LE(mmapRef.mapId, to: &bytes, at: shmFrameHeaderSize)
    writeU32LE(mmapRef.mapGeneration, to: &bytes, at: shmFrameHeaderSize + 4)
    writeU64LE(mmapRef.mapOffset, to: &bytes, at: shmFrameHeaderSize + 8)
    writeU32LE(mmapRef.payloadLen, to: &bytes, at: shmFrameHeaderSize + 16)
    writeU32LE(0, to: &bytes, at: shmFrameHeaderSize + 20) // reserved
    return bytes
}

// r[impl shm.framing]
// r[impl shm.framing.flags]
public func decodeShmFrame(_ frame: [UInt8]) throws -> ShmDecodedFrame {
    guard frame.count >= shmFrameHeaderSize else {
        throw ShmFrameDecodeError.shortHeader
    }
    guard let header = ShmFrameHeader.read(from: frame) else {
        throw ShmFrameDecodeError.shortHeader
    }
    guard header.totalLen >= UInt32(shmFrameHeaderSize) else {
        throw ShmFrameDecodeError.invalidTotalLength(header.totalLen)
    }
    guard Int(header.totalLen) <= frame.count else {
        throw ShmFrameDecodeError.shortFrame(required: Int(header.totalLen), available: frame.count)
    }

    if header.hasSlotRef && header.hasMmapRef {
        throw ShmFrameDecodeError.invalidFlags(header.flags)
    }

    if header.hasSlotRef {
        guard header.totalLen == UInt32(shmSlotRefFrameSize) else {
            throw ShmFrameDecodeError.invalidTotalLength(header.totalLen)
        }
        let start = shmFrameHeaderSize
        let end = start + shmSlotRefSize
        guard end <= frame.count else {
            throw ShmFrameDecodeError.shortSlotRef
        }
        guard let slotRef = ShmSlotRef.read(from: Array(frame[start..<end])) else {
            throw ShmFrameDecodeError.shortSlotRef
        }
        return .slotRef(header: header, slotRef: slotRef)
    }

    if header.hasMmapRef {
        guard header.totalLen == UInt32(shmMmapRefFrameSize) else {
            throw ShmFrameDecodeError.invalidTotalLength(header.totalLen)
        }
        let start = shmFrameHeaderSize
        let end = start + shmMmapRefSize
        guard end <= frame.count else {
            throw ShmFrameDecodeError.shortMmapRef
        }
        let mmapRef = ShmMmapRef(
            mapId: readU32LE(from: frame, at: start),
            mapGeneration: readU32LE(from: frame, at: start + 4),
            mapOffset: readU64LE(from: frame, at: start + 8),
            payloadLen: readU32LE(from: frame, at: start + 16)
        )
        return .mmapRef(header: header, mmapRef: mmapRef)
    }

    let payloadStart = shmFrameHeaderSize
    let frameEnd = Int(header.totalLen)
    guard frameEnd <= frame.count else {
        throw ShmFrameDecodeError.shortFrame(required: frameEnd, available: frame.count)
    }

    // Use inlinePayloadLen to strip alignment padding when available.
    let payloadEnd: Int
    if header.inlinePayloadLen > 0 {
        let exact = payloadStart + Int(header.inlinePayloadLen)
        payloadEnd = min(exact, frameEnd)
    } else {
        // Legacy writer — include padding.
        payloadEnd = frameEnd
    }

    return .inline(header: header, payload: Array(frame[payloadStart..<payloadEnd]))
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

@inline(__always)
private func readU32LE(from bytes: [UInt8], at index: Int) -> UInt32 {
    UInt32(bytes[index])
        | (UInt32(bytes[index + 1]) << 8)
        | (UInt32(bytes[index + 2]) << 16)
        | (UInt32(bytes[index + 3]) << 24)
}

@inline(__always)
private func readU64LE(from bytes: [UInt8], at index: Int) -> UInt64 {
    let b0 = UInt64(bytes[index])
    let b1 = UInt64(bytes[index + 1]) << 8
    let b2 = UInt64(bytes[index + 2]) << 16
    let b3 = UInt64(bytes[index + 3]) << 24
    let b4 = UInt64(bytes[index + 4]) << 32
    let b5 = UInt64(bytes[index + 5]) << 40
    let b6 = UInt64(bytes[index + 6]) << 48
    let b7 = UInt64(bytes[index + 7]) << 56
    return b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7
}
