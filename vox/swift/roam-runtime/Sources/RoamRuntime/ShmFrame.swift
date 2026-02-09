import Foundation

public let shmFrameHeaderSize = 24
public let shmSlotRefSize = 12
public let shmSlotRefFrameSize = 36
public let shmDefaultInlineThreshold: UInt32 = 256
public let shmFlagSlotRef: UInt8 = 0x01

public enum ShmFrameDecodeError: Error, Equatable {
    case shortHeader
    case shortFrame(required: Int, available: Int)
    case invalidTotalLength(UInt32)
    case inlineLengthMismatch(totalLen: UInt32, payloadLen: UInt32)
    case invalidSlotRefFrameLength(UInt32)
    case shortSlotRef
}

public struct ShmFrameHeader: Sendable, Equatable {
    public var totalLen: UInt32
    public var msgType: UInt8
    public var flags: UInt8
    public var id: UInt32
    public var methodId: UInt64
    public var payloadLen: UInt32

    public init(totalLen: UInt32, msgType: UInt8, flags: UInt8, id: UInt32, methodId: UInt64, payloadLen: UInt32)
    {
        self.totalLen = totalLen
        self.msgType = msgType
        self.flags = flags
        self.id = id
        self.methodId = methodId
        self.payloadLen = payloadLen
    }

    public var hasSlotRef: Bool {
        flags & shmFlagSlotRef != 0
    }

    public func write(to buffer: inout [UInt8]) {
        precondition(buffer.count >= shmFrameHeaderSize)
        writeU32LE(totalLen, to: &buffer, at: 0)
        buffer[4] = msgType
        buffer[5] = flags
        buffer[6] = 0
        buffer[7] = 0
        writeU32LE(id, to: &buffer, at: 8)
        writeU64LE(methodId, to: &buffer, at: 12)
        writeU32LE(payloadLen, to: &buffer, at: 20)
    }

    public static func read(from buffer: [UInt8]) -> ShmFrameHeader? {
        guard buffer.count >= shmFrameHeaderSize else {
            return nil
        }

        return ShmFrameHeader(
            totalLen: readU32LE(from: buffer, at: 0),
            msgType: buffer[4],
            flags: buffer[5],
            id: readU32LE(from: buffer, at: 8),
            methodId: readU64LE(from: buffer, at: 12),
            payloadLen: readU32LE(from: buffer, at: 20)
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

public enum ShmDecodedFrame: Sendable, Equatable {
    case inline(header: ShmFrameHeader, payload: [UInt8])
    case slotRef(header: ShmFrameHeader, slotRef: ShmSlotRef)
}

@inline(__always)
public func shmInlineFrameSize(payloadLen: UInt32) -> UInt32 {
    (UInt32(shmFrameHeaderSize) + payloadLen + 3) & ~3
}

@inline(__always)
public func shmShouldInline(payloadLen: UInt32, threshold: UInt32) -> Bool {
    UInt32(shmFrameHeaderSize) + payloadLen <= threshold
}

public func encodeShmInlineFrame(
    msgType: UInt8,
    id: UInt32,
    methodId: UInt64,
    payload: [UInt8]
) -> [UInt8] {
    let payloadLen = UInt32(payload.count)
    let totalLen = shmInlineFrameSize(payloadLen: payloadLen)
    var bytes = [UInt8](repeating: 0, count: Int(totalLen))

    let header = ShmFrameHeader(
        totalLen: totalLen,
        msgType: msgType,
        flags: 0,
        id: id,
        methodId: methodId,
        payloadLen: payloadLen
    )
    header.write(to: &bytes)
    bytes.replaceSubrange(shmFrameHeaderSize..<(shmFrameHeaderSize + payload.count), with: payload)
    return bytes
}

public func encodeShmSlotRefFrame(
    msgType: UInt8,
    id: UInt32,
    methodId: UInt64,
    payloadLen: UInt32,
    slotRef: ShmSlotRef
) -> [UInt8] {
    var bytes = [UInt8](repeating: 0, count: shmSlotRefFrameSize)
    let header = ShmFrameHeader(
        totalLen: UInt32(shmSlotRefFrameSize),
        msgType: msgType,
        flags: shmFlagSlotRef,
        id: id,
        methodId: methodId,
        payloadLen: payloadLen
    )
    header.write(to: &bytes)

    var slotRefBuf = [UInt8](repeating: 0, count: shmSlotRefSize)
    slotRef.write(to: &slotRefBuf)
    bytes.replaceSubrange(shmFrameHeaderSize..<(shmFrameHeaderSize + shmSlotRefSize), with: slotRefBuf)
    return bytes
}

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

    if header.hasSlotRef {
        guard header.totalLen == UInt32(shmSlotRefFrameSize) else {
            throw ShmFrameDecodeError.invalidSlotRefFrameLength(header.totalLen)
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

    let expectedLen = shmInlineFrameSize(payloadLen: header.payloadLen)
    guard expectedLen == header.totalLen else {
        throw ShmFrameDecodeError.inlineLengthMismatch(totalLen: header.totalLen, payloadLen: header.payloadLen)
    }

    let payloadStart = shmFrameHeaderSize
    let payloadEnd = payloadStart + Int(header.payloadLen)
    guard payloadEnd <= frame.count else {
        throw ShmFrameDecodeError.shortFrame(required: payloadEnd, available: frame.count)
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
