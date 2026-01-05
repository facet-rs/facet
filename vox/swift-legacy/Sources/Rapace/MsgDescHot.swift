import Foundation

/// Size of inline payload in bytes.
public let inlinePayloadSize: Int = 16

/// Sentinel value indicating payload is inline (not in a slot).
///
/// Spec: `[impl frame.sentinel.values]`
public let inlinePayloadSlot: UInt32 = UInt32.max

/// Sentinel value indicating no deadline.
public let noDeadline: UInt64 = UInt64.max

/// Errors that can occur during MsgDescHot parsing.
public enum MsgDescHotError: Error {
    /// The input data is not exactly 64 bytes.
    case invalidSize(expected: Int, actual: Int)
}

/// Flags carried in each frame descriptor.
public struct FrameFlags: OptionSet, Sendable {
    public let rawValue: UInt32

    public init(rawValue: UInt32) {
        self.rawValue = rawValue
    }

    /// Regular data frame.
    public static let data = FrameFlags(rawValue: 0b0000_0001)
    /// Control frame (channel 0).
    public static let control = FrameFlags(rawValue: 0b0000_0010)
    /// End of stream (half-close).
    public static let eos = FrameFlags(rawValue: 0b0000_0100)
    /// Cancel this channel.
    public static let cancel = FrameFlags(rawValue: 0b0000_1000)
    /// Error response.
    public static let error = FrameFlags(rawValue: 0b0001_0000)
    /// Priority scheduling hint.
    public static let highPriority = FrameFlags(rawValue: 0b0010_0000)
    /// Contains credit grant.
    public static let credits = FrameFlags(rawValue: 0b0100_0000)
    /// Headers/trailers only, no body.
    public static let metadataOnly = FrameFlags(rawValue: 0b1000_0000)
    /// Don't send a reply frame for this request.
    public static let noReply = FrameFlags(rawValue: 0b0001_0000_0000)
}

/// Hot-path message descriptor (64 bytes, one cache line).
///
/// This is the primary descriptor used for frame dispatch.
/// Fits in a single cache line for performance.
///
/// Binary layout (all little-endian):
/// ```
/// offset  size  field
/// 0       8     msgId: UInt64
/// 8       4     channelId: UInt32
/// 12      4     methodId: UInt32
/// 16      4     payloadSlot: UInt32
/// 20      4     payloadGeneration: UInt32
/// 24      4     payloadOffset: UInt32
/// 28      4     payloadLen: UInt32
/// 32      4     flags: UInt32 (FrameFlags bitfield)
/// 36      4     creditGrant: UInt32
/// 40      8     deadlineNs: UInt64
/// 48      16    inlinePayload: [UInt8] (fixed 16 bytes)
/// ```
///
/// Spec: `[impl frame.desc.encoding]`
public struct MsgDescHot: Sendable {
    /// Size of the serialized descriptor in bytes.
    ///
    /// Spec: `[impl frame.desc.size]`
    public static let size: Int = 64

    // Identity (16 bytes)
    /// Unique message ID per session, monotonic.
    public var msgId: UInt64
    /// Logical stream (0 = control channel).
    public var channelId: UInt32
    /// For RPC dispatch, or control verb.
    public var methodId: UInt32

    // Payload location (16 bytes)
    /// Slot index (UInt32.max = inline).
    public var payloadSlot: UInt32
    /// Generation counter for ABA safety.
    public var payloadGeneration: UInt32
    /// Offset within slot.
    public var payloadOffset: UInt32
    /// Actual payload length.
    public var payloadLen: UInt32

    // Flow control & timing (16 bytes)
    /// Frame flags (EOS, CANCEL, ERROR, etc.).
    public var flags: FrameFlags
    /// Credits being granted to peer.
    public var creditGrant: UInt32
    /// Deadline in nanoseconds (monotonic clock). noDeadline = no deadline.
    public var deadlineNs: UInt64

    // Inline payload for small messages (16 bytes)
    /// When payloadSlot == UInt32.max, payload lives here.
    public var inlinePayload: (
        UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
        UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8
    )

    /// Create a new descriptor with default values.
    public init() {
        self.msgId = 0
        self.channelId = 0
        self.methodId = 0
        self.payloadSlot = inlinePayloadSlot
        self.payloadGeneration = 0
        self.payloadOffset = 0
        self.payloadLen = 0
        self.flags = []
        self.creditGrant = 0
        self.deadlineNs = noDeadline
        self.inlinePayload = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    }

    /// Parse from 64 bytes of raw data.
    ///
    /// - Parameter data: Exactly 64 bytes of little-endian data.
    /// - Throws: `MsgDescHotError.invalidSize` if data is not 64 bytes.
    public init(from data: Data) throws {
        guard data.count == Self.size else {
            throw MsgDescHotError.invalidSize(expected: Self.size, actual: data.count)
        }

        // Parse using withUnsafeBytes, extracting values to local variables first
        let parsed: (
            msgId: UInt64, channelId: UInt32, methodId: UInt32,
            payloadSlot: UInt32, payloadGeneration: UInt32, payloadOffset: UInt32, payloadLen: UInt32,
            flags: UInt32, creditGrant: UInt32, deadlineNs: UInt64,
            inlinePayload: (UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
                           UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8)
        ) = data.withUnsafeBytes { buffer in
            let ptr = buffer.baseAddress!

            return (
                msgId: ptr.load(fromByteOffset: 0, as: UInt64.self).littleEndian,
                channelId: ptr.load(fromByteOffset: 8, as: UInt32.self).littleEndian,
                methodId: ptr.load(fromByteOffset: 12, as: UInt32.self).littleEndian,
                payloadSlot: ptr.load(fromByteOffset: 16, as: UInt32.self).littleEndian,
                payloadGeneration: ptr.load(fromByteOffset: 20, as: UInt32.self).littleEndian,
                payloadOffset: ptr.load(fromByteOffset: 24, as: UInt32.self).littleEndian,
                payloadLen: ptr.load(fromByteOffset: 28, as: UInt32.self).littleEndian,
                flags: ptr.load(fromByteOffset: 32, as: UInt32.self).littleEndian,
                creditGrant: ptr.load(fromByteOffset: 36, as: UInt32.self).littleEndian,
                deadlineNs: ptr.load(fromByteOffset: 40, as: UInt64.self).littleEndian,
                inlinePayload: (
                    buffer[48], buffer[49], buffer[50], buffer[51],
                    buffer[52], buffer[53], buffer[54], buffer[55],
                    buffer[56], buffer[57], buffer[58], buffer[59],
                    buffer[60], buffer[61], buffer[62], buffer[63]
                )
            )
        }

        // Assign parsed values to self
        self.msgId = parsed.msgId
        self.channelId = parsed.channelId
        self.methodId = parsed.methodId
        self.payloadSlot = parsed.payloadSlot
        self.payloadGeneration = parsed.payloadGeneration
        self.payloadOffset = parsed.payloadOffset
        self.payloadLen = parsed.payloadLen
        self.flags = FrameFlags(rawValue: parsed.flags)
        self.creditGrant = parsed.creditGrant
        self.deadlineNs = parsed.deadlineNs
        self.inlinePayload = parsed.inlinePayload
    }

    /// Serialize to 64 bytes of raw data.
    ///
    /// - Returns: 64 bytes of little-endian data.
    public func serialize() -> Data {
        var data = Data(count: Self.size)

        data.withUnsafeMutableBytes { buffer in
            let ptr = buffer.baseAddress!

            // Identity (16 bytes)
            ptr.storeBytes(of: msgId.littleEndian, toByteOffset: 0, as: UInt64.self)
            ptr.storeBytes(of: channelId.littleEndian, toByteOffset: 8, as: UInt32.self)
            ptr.storeBytes(of: methodId.littleEndian, toByteOffset: 12, as: UInt32.self)

            // Payload location (16 bytes)
            ptr.storeBytes(of: payloadSlot.littleEndian, toByteOffset: 16, as: UInt32.self)
            ptr.storeBytes(of: payloadGeneration.littleEndian, toByteOffset: 20, as: UInt32.self)
            ptr.storeBytes(of: payloadOffset.littleEndian, toByteOffset: 24, as: UInt32.self)
            ptr.storeBytes(of: payloadLen.littleEndian, toByteOffset: 28, as: UInt32.self)

            // Flow control & timing (16 bytes)
            ptr.storeBytes(of: flags.rawValue.littleEndian, toByteOffset: 32, as: UInt32.self)
            ptr.storeBytes(of: creditGrant.littleEndian, toByteOffset: 36, as: UInt32.self)
            ptr.storeBytes(of: deadlineNs.littleEndian, toByteOffset: 40, as: UInt64.self)

            // Inline payload (16 bytes)
            buffer[48] = inlinePayload.0
            buffer[49] = inlinePayload.1
            buffer[50] = inlinePayload.2
            buffer[51] = inlinePayload.3
            buffer[52] = inlinePayload.4
            buffer[53] = inlinePayload.5
            buffer[54] = inlinePayload.6
            buffer[55] = inlinePayload.7
            buffer[56] = inlinePayload.8
            buffer[57] = inlinePayload.9
            buffer[58] = inlinePayload.10
            buffer[59] = inlinePayload.11
            buffer[60] = inlinePayload.12
            buffer[61] = inlinePayload.13
            buffer[62] = inlinePayload.14
            buffer[63] = inlinePayload.15
        }

        return data
    }

    /// Returns true if this frame has a deadline set.
    public var hasDeadline: Bool {
        deadlineNs != noDeadline
    }

    /// Returns true if payload is inline (not in a slot).
    public var isInline: Bool {
        payloadSlot == inlinePayloadSlot
    }

    /// Returns true if this is a control frame (channel 0).
    public var isControl: Bool {
        channelId == 0
    }

    /// Get inline payload as Data (only valid if isInline).
    public var inlinePayloadData: Data {
        let bytes: [UInt8] = [
            inlinePayload.0, inlinePayload.1, inlinePayload.2, inlinePayload.3,
            inlinePayload.4, inlinePayload.5, inlinePayload.6, inlinePayload.7,
            inlinePayload.8, inlinePayload.9, inlinePayload.10, inlinePayload.11,
            inlinePayload.12, inlinePayload.13, inlinePayload.14, inlinePayload.15
        ]
        return Data(bytes.prefix(Int(payloadLen)))
    }

    /// Set inline payload from Data.
    ///
    /// - Parameter data: Up to 16 bytes of payload data.
    /// - Precondition: data.count <= 16
    public mutating func setInlinePayload(_ data: Data) {
        precondition(data.count <= inlinePayloadSize, "Inline payload must be at most \(inlinePayloadSize) bytes")

        // Zero-initialize
        inlinePayload = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)

        // Copy data
        if data.count > 0 { inlinePayload.0 = data[data.startIndex] }
        if data.count > 1 { inlinePayload.1 = data[data.startIndex + 1] }
        if data.count > 2 { inlinePayload.2 = data[data.startIndex + 2] }
        if data.count > 3 { inlinePayload.3 = data[data.startIndex + 3] }
        if data.count > 4 { inlinePayload.4 = data[data.startIndex + 4] }
        if data.count > 5 { inlinePayload.5 = data[data.startIndex + 5] }
        if data.count > 6 { inlinePayload.6 = data[data.startIndex + 6] }
        if data.count > 7 { inlinePayload.7 = data[data.startIndex + 7] }
        if data.count > 8 { inlinePayload.8 = data[data.startIndex + 8] }
        if data.count > 9 { inlinePayload.9 = data[data.startIndex + 9] }
        if data.count > 10 { inlinePayload.10 = data[data.startIndex + 10] }
        if data.count > 11 { inlinePayload.11 = data[data.startIndex + 11] }
        if data.count > 12 { inlinePayload.12 = data[data.startIndex + 12] }
        if data.count > 13 { inlinePayload.13 = data[data.startIndex + 13] }
        if data.count > 14 { inlinePayload.14 = data[data.startIndex + 14] }
        if data.count > 15 { inlinePayload.15 = data[data.startIndex + 15] }

        payloadLen = UInt32(data.count)
        payloadSlot = inlinePayloadSlot
    }
}

extension MsgDescHot: CustomDebugStringConvertible {
    public var debugDescription: String {
        """
        MsgDescHot {
            msgId: \(msgId)
            channelId: \(channelId)
            methodId: \(methodId)
            payloadSlot: \(payloadSlot == inlinePayloadSlot ? "INLINE" : String(payloadSlot))
            payloadGeneration: \(payloadGeneration)
            payloadOffset: \(payloadOffset)
            payloadLen: \(payloadLen)
            flags: \(flags.rawValue)
            creditGrant: \(creditGrant)
            deadlineNs: \(deadlineNs == noDeadline ? "NONE" : String(deadlineNs))
            isInline: \(isInline)
        }
        """
    }
}
